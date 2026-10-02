#!/usr/bin/env node
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { execFile } from 'node:child_process';
import { createRequire } from 'node:module';
import { createServer } from 'node:http';
import { createServer as createProbe } from 'node:net';
import { once } from 'node:events';
import { resolve } from 'node:path';
import { parseArgs, promisify } from 'node:util';
import { setTimeout as delay } from 'node:timers/promises';
import { chromium } from 'playwright-core';
import pty from 'node-pty';

const require = createRequire(import.meta.url);
const { values } = parseArgs({ options: {
  title: { type: 'string', default: 'Terminal visual QA' },
  command: { type: 'string' }, 'from-file': { type: 'string' },
  input: { type: 'string', multiple: true, default: [] },
  'evidence-dir': { type: 'string', default: '.omo/evidence/terminal-qa' },
  cols: { type: 'string', default: '80' }, rows: { type: 'string', default: '24' },
  'font-size': { type: 'string', default: '16' }, scale: { type: 'string', default: '2' },
  'settle-ms': { type: 'string', default: '500' },
  'input-delay-ms': { type: 'string', default: '200' },
  'wait-for': { type: 'string' }, 'timeout-ms': { type: 'string', default: '15000' },
  'record-video': { type: 'boolean', default: false },
  browser: { type: 'string' }, help: { type: 'boolean', default: false },
} });
if (values.help) {
  console.log(`Usage: node script/qa/web-terminal-visual-qa.mjs --command '<shell command>' | --from-file capture.ansi
  --title TEXT --evidence-dir DIR --cols 80 --rows 24 --font-size 16 --scale 2
  --input TEXT --input '{Enter}' --input '{Backspace}' --input '{ArrowDown}'
  --wait-for TEXT --timeout-ms 15000 --settle-ms 500 --input-delay-ms 200
  --record-video (opt-in live WebM; requires Playwright ffmpeg and ffprobe)
  --browser /path/to/chrome (otherwise installed Google Chrome)
Captures the active screen before terminating the PTY. --wait-for matches raw output.
Keys: Enter, Backspace, Tab, Escape, arrows, Home, End, Delete, Ctrl+A..Z.
Plain --input is written literally. Use shell quoting for escape sequences/paste.
POSIX only. Run pnpm --dir script/qa install before use. Development tooling only.`);
  process.exit(0);
}
if (Boolean(values.command) === Boolean(values['from-file'])) {
  throw new TypeError('Specify exactly one of --command or --from-file');
}
if (values['from-file'] && values['record-video']) throw new TypeError('--record-video requires a live --command, not --from-file');
if (values['from-file'] && values.input.length) throw new TypeError('Replay cannot accept --input');
const numeric = (key, min, max) => {
  const value = Number(values[key]);
  if (!Number.isInteger(value) || value < min || value > max) {
    throw new RangeError(`--${key} must be an integer in ${min}..${max}`);
  }
  return value;
};
const cols = numeric('cols', 2, 500);
const rows = numeric('rows', 2, 200);
const fontSize = numeric('font-size', 8, 48);
const scale = numeric('scale', 1, 4);
const settleMs = numeric('settle-ms', 0, 60000);
const inputDelayMs = numeric('input-delay-ms', 0, 60000);
const timeoutMs = numeric('timeout-ms', 1, 300000);
const keys = { Enter: '\r', Backspace: '\x7f', Tab: '\t', Escape: '\x1b',
  ArrowUp: '\x1b[A', ArrowDown: '\x1b[B', ArrowRight: '\x1b[C', ArrowLeft: '\x1b[D',
  Home: '\x1b[H', End: '\x1b[F', Delete: '\x1b[3~' };
const inputs = values.input.map(input => {
  const match = /^\{(.+)\}$/.exec(input);
  if (!match) return input;
  const key = match[1];
  if (Object.hasOwn(keys, key)) return keys[key];
  if (/^Ctrl\+[A-Z]$/i.test(key)) return String.fromCharCode(key.at(-1).toUpperCase().charCodeAt(0) - 64);
  throw new TypeError(`Unknown input key ${input}; plain text is accepted without braces`);
});
const evidenceDir = resolve(values['evidence-dir']);
await mkdir(evidenceDir, { recursive: true });
const metadata = {
  title: values.title, command: values.command ?? null, fromFile: values['from-file'] ?? null,
  cwd: process.cwd(), invocation: process.argv, cols, rows, fontSize, scale,
  startedAt: new Date().toISOString(), versions: { node: process.version },
  video: { requested: values['record-video'], source: values['record-video'] ? 'live-pty-browser' : null },
  capture: null, cleanup: {}, error: null,
};
const raw = [];
let browserServer, browser, context, video, terminal, server, port;
let exitObserved = false;
let writeQueue = Promise.resolve();
let renderError;
let interrupted;
const interrupt = signal => { interrupted = signal; };
const onInt = () => interrupt('SIGINT');
const onTerm = () => interrupt('SIGTERM');
process.on('SIGINT', onInt);
process.on('SIGTERM', onTerm);
const assertRunning = () => {
  if (interrupted) throw new Error(`Interrupted by ${interrupted}`);
  if (renderError) throw renderError;
};
const alive = pid => {
  try { process.kill(pid, 0); return true; }
  catch (error) {
    if (error.code === 'ESRCH') return false;
    if (error.code === 'EPERM') return true;
    throw error;
  }
};
try {
  if (values['record-video']) await promisify(execFile)('ffprobe', ['-version'], { timeout: 10000 });
  const js = await readFile(require.resolve('@xterm/xterm'));
  const css = await readFile(require.resolve('@xterm/xterm/css/xterm.css'));
  for (const name of ['@xterm/xterm', 'node-pty', 'playwright-core']) {
    metadata.versions[name] = JSON.parse(await readFile(require.resolve(`${name}/package.json`), 'utf8')).version;
  }
  server = createServer((request, response) => {
    const resources = {
      '/': ['text/html', '<!doctype html><meta charset="utf-8"><link rel="stylesheet" href="/xterm.css"><style>body{margin:0;background:#101216}#terminal{display:inline-block;padding:16px}</style><div id="terminal"></div><script src="/xterm.js"></script>'],
      '/xterm.js': ['text/javascript', js], '/xterm.css': ['text/css', css],
    };
      const resource = Object.hasOwn(resources, request.url) ? resources[request.url] : undefined;
    response.writeHead(resource ? 200 : 404, { 'Content-Type': resource?.[0] ?? 'text/plain' });
    response.end(resource?.[1] ?? 'Not found');
  });
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  port = server.address().port;
  metadata.server = { host: '127.0.0.1', port };
  browserServer = await chromium.launchServer({ headless: true,
    ...(values.browser ? { executablePath: values.browser } : { channel: 'chrome' }) });
  metadata.browserPid = browserServer.process().pid;
  browser = await chromium.connect(browserServer.wsEndpoint());
  metadata.versions.browser = browser.version();
  const openTerminal = async context => {
    const page = await context.newPage();
    await page.goto(`http://127.0.0.1:${port}`);
    await page.exposeFunction('ptyInput', data => { if (terminal && !exitObserved) terminal.write(data); });
    await page.evaluate(({ cols, rows, fontSize, title }) => {
      document.title = title;
      window.term = new Terminal({ cols, rows, fontSize, fontFamily: 'Menlo, "DejaVu Sans Mono", monospace',
        cursorBlink: false, scrollback: 10000, theme: { background: '#101216', foreground: '#e8eaed' } });
      window.term.open(document.querySelector('#terminal'));
      window.term.onData(data => window.ptyInput(data));
    }, { cols, rows, fontSize, title: values.title });
    await page.evaluate(() => document.fonts.ready);
    return page;
  };
  context = await browser.newContext({ viewport: { width: 1200, height: 800 }, deviceScaleFactor: scale });
  let page = await openTerminal(context);
  if (values['record-video']) {
    const bounds = await page.locator('#terminal').boundingBox();
    const viewport = { width: Math.ceil(bounds.width / 2) * 2, height: Math.ceil(bounds.height / 2) * 2 };
    await context.close();
    context = await browser.newContext({ viewport, deviceScaleFactor: scale,
      recordVideo: { dir: evidenceDir, size: viewport } });
    page = await openTerminal(context);
    video = page.video();
  }
  const write = bytes => page.evaluate(bytes => new Promise(resolve => {
    window.term.write(new Uint8Array(bytes), resolve);
  }), Array.from(bytes));
  if (values.command) {
    terminal = pty.spawn('/bin/sh', ['-c', values.command], { name: 'xterm-256color', cols, rows,
      cwd: process.cwd(), env: { ...process.env, TERM: 'xterm-256color', COLORTERM: 'truecolor' }, encoding: null });
    metadata.ptyPid = terminal.pid;
    terminal.onExit(event => { exitObserved = true; metadata.ptyExit = event; });
    terminal.onData(bytes => {
      const chunk = Buffer.from(bytes); raw.push(chunk);
      writeQueue = writeQueue.then(() => write(chunk)).catch(error => { renderError = error; });
    });
    if (values['wait-for']) {
      const deadline = Date.now() + timeoutMs;
      while (!Buffer.concat(raw).toString('utf8').includes(values['wait-for'])) {
        assertRunning();
        if (exitObserved || Date.now() >= deadline) throw new Error(`PTY did not emit --wait-for ${values['wait-for']}`);
        await delay(20);
      }
    } else await delay(settleMs);
    for (const input of inputs) {
      assertRunning();
      if (exitObserved) throw new Error('PTY exited before all input was delivered');
      terminal.write(input);
      await delay(inputDelayMs);
    }
  } else {
    const bytes = await readFile(values['from-file']); raw.push(bytes); await write(bytes);
  }
  await delay(settleMs);
  assertRunning();
  // Pause collection at one precise stream boundary so PNG, text and ANSI describe the same frame.
  const captureBytes = Buffer.concat(raw);
  if (terminal && !exitObserved) terminal.pause();
  await writeQueue;
  assertRunning();
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  const screen = await page.evaluate(() => {
    const buffer = window.term.buffer.active;
    return { type: buffer.type, cursorX: buffer.cursorX, cursorY: buffer.cursorY,
      lines: Array.from({ length: window.term.rows }, (_, row) => buffer.getLine(buffer.viewportY + row)?.translateToString(true) ?? ''),
      widths: Array.from({ length: window.term.rows }, (_, row) => {
        const line = buffer.getLine(buffer.viewportY + row);
        const cells = [];
        for (let column = 0; column < window.term.cols; column++) {
          const cell = line?.getCell(column);
          if (cell?.getChars()) cells.push({ column, text: cell.getChars(), width: cell.getWidth() });
        }
        return cells;
      }) };
  });
  const bounds = await page.locator('#terminal').boundingBox();
  if (!video) await page.setViewportSize({ width: Math.ceil(bounds.width), height: Math.ceil(bounds.height) });
  await page.locator('#terminal').screenshot({ path: resolve(evidenceDir, 'terminal.png') });
  await writeFile(resolve(evidenceDir, 'terminal.txt'), screen.lines.join('\n') + '\n');
  await writeFile(resolve(evidenceDir, 'terminal-ansi.txt'), captureBytes);
  metadata.capture = { ...screen, cssWidth: bounds.width, cssHeight: bounds.height, ansiBytes: captureBytes.length };
} catch (error) {
  metadata.error = error instanceof Error ? error.message : String(error);
  process.exitCode = 1;
} finally {
  const cleanup = await Promise.allSettled([
    (async () => {
      if (!terminal) return;
      terminal.resume();
      const signalGroup = signal => {
        try { process.kill(-terminal.pid, signal); }
        catch (error) {
          // An exiting macOS process group can transiently reject a signal.
          // Only disappearance below proves cleanup; EPERM never counts as exit.
          if (error.code !== 'ESRCH' && error.code !== 'EPERM') throw error;
        }
      };
      const waitForGroupExit = async () => {
        const deadline = Date.now() + 1500;
        while (!exitObserved || alive(terminal.pid) || alive(-terminal.pid)) {
          if (Date.now() >= deadline) return false;
          await delay(20);
        }
        return true;
      };
      metadata.cleanup.ptySignals = [];
      if (alive(-terminal.pid)) {
        metadata.cleanup.ptySignals.push('SIGTERM');
        signalGroup('SIGTERM');
      }
      if (!await waitForGroupExit()) {
        metadata.cleanup.ptySignals.push('SIGKILL');
        signalGroup('SIGKILL');
        await waitForGroupExit();
      }
      metadata.cleanup.ptyExited = exitObserved && !alive(terminal.pid);
      metadata.cleanup.ptyProcessGroupExited = !alive(-terminal.pid);
      if (!metadata.cleanup.ptyExited) throw new Error('PTY failed to exit');
      if (!metadata.cleanup.ptyProcessGroupExited) throw new Error('PTY process group failed to exit');
    })(),
    (async () => {
      try {
        if (context) await context.close();
        if (video) {
          const path = resolve(evidenceDir, 'terminal.webm');
          await video.saveAs(path);
          await video.delete();
          const bytes = await readFile(path);
          const { stdout } = await promisify(execFile)('ffprobe', ['-v', 'error', '-show_format',
            '-show_streams', '-of', 'json', path], { timeout: 10000 });
          const probe = JSON.parse(stdout);
          const durationSeconds = Number(probe.format?.duration);
          const stream = probe.streams?.find(stream => stream.codec_type === 'video');
          if (!bytes.length || !Number.isFinite(durationSeconds) || durationSeconds <= 0 || !stream) {
            throw new Error('Recording has no playable video or positive duration');
          }
          Object.assign(metadata.video, { path, sha256: createHash('sha256').update(bytes).digest('hex'),
            durationSeconds, sizeBytes: bytes.length, width: stream.width, height: stream.height,
            codec: stream.codec_name });
          metadata.cleanup.videoFinalized = true;
        }
      } finally {
        try {
          if (browser) await browser.close();
        } finally {
          if (browserServer) {
            await browserServer.close();
            metadata.cleanup.browserExited = !alive(metadata.browserPid);
            if (!metadata.cleanup.browserExited) throw new Error('Browser failed to exit');
          }
        }
      }
    })(),
    (async () => {
      if (!server?.listening) return;
      server.closeAllConnections();
      await new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
      const probe = createProbe();
      probe.listen(port, '127.0.0.1');
      await once(probe, 'listening');
      await new Promise((resolve, reject) => probe.close(error => error ? reject(error) : resolve()));
      metadata.cleanup.serverClosed = true;
      metadata.cleanup.portRebindVerified = true;
    })(),
  ]);
  metadata.cleanup.errors = cleanup.filter(result => result.status === 'rejected').map(result => String(result.reason));
  metadata.cleanup.completedAt = new Date().toISOString();
  if (metadata.cleanup.errors.length) process.exitCode = 1;
  await writeFile(resolve(evidenceDir, 'metadata.json'), JSON.stringify(metadata, null, 2) + '\n');
  process.off('SIGINT', onInt); process.off('SIGTERM', onTerm);
}
if (metadata.error) console.error(metadata.error);
if (metadata.cleanup.errors.length) console.error(metadata.cleanup.errors.join('\n'));
console.log(evidenceDir);
