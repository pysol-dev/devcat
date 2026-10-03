import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { resolve, join } from 'node:path';

const directory = resolve(process.argv[2] ?? '.omo/evidence/terminal-resize');
await mkdir(directory, { recursive: true });
const runs = [];
const run = (binary, args, expectedStatus = 0) => {
  const result = spawnSync(binary, args, { encoding: 'utf8', timeout: 45000 });
  runs.push({ binary, args, status: result.status, stdout: result.stdout, stderr: result.stderr,
    error: result.error?.message ?? null });
  assert.equal(result.status, expectedStatus, result.stderr);
  return result.stdout;
};
const scenarios = [];
try {
  for (const [name, initial, sizes, record] of [
    ['roundtrip-video', [120, 40], [[80, 24], [120, 40]], true],
    ['expansion-video', [80, 24], [[120, 40], [80, 24]], true],
    ['shrink-png', [120, 40], [[80, 24]], false],
  ]) {
    const path = join(directory, name);
    const tokens = ['a', 'b', 'c', `{Resize:${sizes[0].join('x')}}`, 'd',
      ...sizes.slice(1).map(size => `{Resize:${size.join('x')}}`), '{Enter}'];
    const invocation = ['script/qa/web-terminal-visual-qa.mjs', '--command', 'node script/qa/resize-fixture.mjs',
      '--wait-for', 'RESIZE_READY', '--cols', String(initial[0]), '--rows', String(initial[1]),
      '--input-delay-ms', '400', '--settle-ms', '300', '--scale', '1', '--evidence-dir', path,
      ...(record ? ['--record-video'] : []), ...tokens.flatMap(token => ['--input', token])];
    run(process.execPath, invocation);
    const metadata = JSON.parse(await readFile(join(path, 'metadata.json'), 'utf8'));
    assert.equal(metadata.error, null);
    assert.equal(metadata.ptyExit.exitCode, 0);
    assert.equal(metadata.resizeEvents.length, sizes.length);
    let previous = { cols: initial[0], rows: initial[1] };
    for (const [index, size] of sizes.entries()) {
      const event = metadata.resizeEvents[index];
      const expected = { cols: size[0], rows: size[1] };
      assert.deepEqual(event.old, { pty: previous, xterm: previous });
      assert.deepEqual(event.new, expected);
      assert.deepEqual(event.applied, { pty: expected, xterm: expected });
      assert.ok(event.requestedMs >= (index === 0 ? 1200 : 2000));
      assert.ok(event.appliedMs >= event.requestedMs);
      assert.ok(event.observedMs >= event.appliedMs + 390);
      assert.equal(event.lines.length, size[1]);
      assert.ok(event.lines.includes(`RESIZED ${size.join('x')} draft=${index === 0 ? 'abc' : 'abcd'}`));
      assert.equal(event.lines.at(-1), 'L' + '-'.repeat(size[0] - 2) + 'R');
      previous = expected;
    }
    const text = await readFile(join(path, 'terminal.txt'), 'utf8');
    const ansi = await readFile(join(path, 'terminal-ansi.txt'), 'utf8');
    for (const line of [`START ${initial.join('x')} draft=`, `KEY ${initial.join('x')} draft=abc`,
      `KEY ${sizes[0].join('x')} draft=abcd`, `EXIT ${sizes.at(-1).join('x')} draft=abcd`]) {
      assert.ok(text.includes(line), line);
      assert.ok(ansi.includes(line), line);
    }
    assert.equal(metadata.capture.cols, previous.cols);
    assert.equal(metadata.capture.rows, previous.rows);
    assert.equal(metadata.capture.lines.at(-1), 'L' + '-'.repeat(previous.cols - 2) + 'R');
    const png = await readFile(join(path, 'terminal.png'));
    assert.deepEqual([...png.subarray(0, 8)], [137, 80, 78, 71, 13, 10, 26, 10]);
    const dimensions = [png.readUInt32BE(16), png.readUInt32BE(20)];
    assert.deepEqual(dimensions, [Math.ceil(metadata.capture.cssWidth), Math.ceil(metadata.capture.cssHeight)]);
    assert.deepEqual(metadata.cleanup.errors, []);
    for (const key of ['ptyExited', 'ptyProcessGroupExited', 'browserExited', 'serverClosed', 'portRebindVerified',
      ...(record ? ['videoFinalized'] : [])]) assert.equal(metadata.cleanup[key], true, key);
    for (const pid of [metadata.ptyPid, -metadata.ptyPid, metadata.browserPid]) {
      assert.throws(() => process.kill(pid, 0), { code: 'ESRCH' });
    }
    if (record) {
      const probe = JSON.parse(run('ffprobe', ['-v', 'error', '-show_streams', '-of', 'json', metadata.video.path]));
      const stream = probe.streams.find(stream => stream.codec_type === 'video');
      assert.equal(stream.width, metadata.video.viewport.width);
      assert.equal(stream.height, metadata.video.viewport.height);
      assert.ok(stream.width >= dimensions[0] && stream.height >= dimensions[1]);
      assert.ok(metadata.video.durationSeconds >= tokens.length * 0.4);
      assert.ok((await readFile(metadata.video.path)).length > 0);
      run('ffmpeg', ['-v', 'error', '-i', metadata.video.path, '-vf', 'fps=2', '-y', join(path, 'frame-%02d.png')]);
    }
    scenarios.push({ name, invocation, result: 'PASS', dimensions, sizes, cleanup: metadata.cleanup });
  }
  for (const token of ['{Resize:1x24}', '{Resize:501x24}', '{Resize:80x201}', '{Resize:80x0}',
    '{Resize:080x24}', '{Resize:80X24}', '{Resize:80.5x24}', '{Resize:80x24', '{Resize:}']) {
    const stderrBefore = runs.length;
    run(process.execPath, ['script/qa/web-terminal-visual-qa.mjs', '--command', 'exit 99', '--input', token], 1);
    assert.match(runs[stderrBefore].stderr, /Resize must be/);
  }
  await writeFile(join(directory, 'verified.json'), JSON.stringify({ result: 'PASS', scenarios,
    malformedTokensRejected: 9 }, null, 2) + '\n');
  console.log('PASS: real PTY/xterm timed resize, composed draft, final PNG, video geometry, invalid tokens and cleanup');
} finally {
  await writeFile(join(directory, 'runner.json'), JSON.stringify(runs, null, 2) + '\n');
}
