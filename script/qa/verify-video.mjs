import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdir, mkdtemp, readFile, readdir, writeFile } from 'node:fs/promises';
import { resolve, join } from 'node:path';

const directory = resolve(process.argv[2] ?? '.omo/evidence/terminal-video');
await mkdir(directory, { recursive: true });
const runs = [];
const run = (binary, args, expectedStatus = 0) => {
  const result = spawnSync(binary, args, { encoding: 'utf8', timeout: 30000 });
  runs.push({ binary, args, status: result.status, stdout: result.stdout, stderr: result.stderr,
    error: result.error?.message ?? null });
  assert.equal(result.status, expectedStatus, result.stderr);
  return result.stdout;
};
try {
  const invocation = ['script/qa/web-terminal-visual-qa.mjs', '--record-video',
    '--command', 'node script/qa/fixture.mjs', '--wait-for', 'PTY_READY',
    '--cols', '40', '--rows', '10', '--input-delay-ms', '650', '--settle-ms', '500',
    ...['h', 'e', 'l', 'l', 'o', '界', '{Backspace}', '{Enter}'].flatMap(key => ['--input', key]),
    '--evidence-dir', directory];
  run(process.execPath, invocation);
  const metadata = JSON.parse(await readFile(join(directory, 'metadata.json'), 'utf8'));
  assert.equal(metadata.error, null);
  assert.equal(metadata.video.requested, true);
  assert.equal(metadata.video.source, 'live-pty-browser');
  const path = join(directory, 'terminal.webm');
  assert.equal(metadata.video.path, path);
  const bytes = await readFile(path);
  assert.equal(metadata.video.sizeBytes, bytes.length);
  assert.ok(bytes.length > 0);
  assert.equal(metadata.video.sha256, createHash('sha256').update(bytes).digest('hex'));
  const probeText = run('ffprobe', ['-v', 'error', '-show_format', '-show_streams', '-of', 'json', path]);
  await writeFile(join(directory, 'ffprobe.json'), probeText);
  const probe = JSON.parse(probeText);
  assert.equal(metadata.video.durationSeconds, Number(probe.format.duration));
  assert.ok(metadata.video.durationSeconds >= 5.2, 'Video must span all eight slow inputs');
  assert.equal(metadata.video.codec, 'vp8');
  assert.equal(metadata.capture.lines[5], 'draft: hello');
  assert.equal(metadata.capture.lines[6], 'ENTER_RECEIVED');
  for (const key of ['ptyExited', 'ptyProcessGroupExited', 'browserExited', 'serverClosed', 'portRebindVerified', 'videoFinalized']) {
    assert.equal(metadata.cleanup[key], true, key);
  }
  assert.deepEqual(metadata.cleanup.errors, []);
  for (const pid of [metadata.ptyPid, -metadata.ptyPid, metadata.browserPid]) {
    assert.throws(() => process.kill(pid, 0), { code: 'ESRCH' });
  }
  const frameDir = await mkdtemp(join(directory, 'frames-'));
  run('ffmpeg', ['-v', 'error', '-i', path, '-vf', 'fps=2', '-y', join(frameDir, 'frame-%03d.png')]);
  const frames = [];
  for (const name of (await readdir(frameDir)).filter(name => /^frame-\d+\.png$/.test(name)).sort()) {
    const frame = await readFile(join(frameDir, name));
    frames.push({ path: join(frameDir, name), sha256: createHash('sha256').update(frame).digest('hex') });
  }
  assert.ok(new Set(frames.map(frame => frame.sha256)).size >= 3, 'At least three distinct decoded frames');
  run(process.execPath, ['script/qa/web-terminal-visual-qa.mjs', '--record-video', '--from-file', join(directory, 'terminal-ansi.txt')], 1);
  await writeFile(join(directory, 'verified.json'), JSON.stringify({ result: 'PASS', invocation,
    scenario: 'Six slow live keys, Backspace, Enter; decoded motion; opt-in replay rejected; cleanup',
    video: metadata.video, frames, cleanup: metadata.cleanup }, null, 2) + '\n');
  console.log('PASS: opt-in live WebM, six slow keys/Backspace, motion frames, metadata and cleanup');
} finally {
  await writeFile(join(directory, 'runner.json'), JSON.stringify(runs, null, 2) + '\n');
}
