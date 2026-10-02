import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';

const [captureDir, replayDir] = process.argv.slice(2);
if (!captureDir || !replayDir) throw new TypeError('Usage: node script/qa/verify-fixture.mjs CAPTURE_DIR REPLAY_DIR');
const report = [];
for (const directory of [captureDir, replayDir]) {
  // Given a freshly captured PTY fixture or its ANSI replay.
  const metadata = JSON.parse(await readFile(join(directory, 'metadata.json'), 'utf8'));
  const png = await readFile(join(directory, 'terminal.png'));
  const text = await readFile(join(directory, 'terminal.txt'), 'utf8');
  const ansi = await readFile(join(directory, 'terminal-ansi.txt'));
  // When examining the renderer's observable screen and cleanup receipt.
  const dimensions = [png.readUInt32BE(16), png.readUInt32BE(20)];
  const wide = metadata.capture.widths.flat().filter(cell => cell.text === '界');
  // Then input editing, screen geometry, truecolor bytes, and teardown are preserved.
  assert.equal(metadata.error, null);
  assert.deepEqual([...png.subarray(0, 8)], [137, 80, 78, 71, 13, 10, 26, 10]);
  assert.deepEqual(dimensions, [Math.ceil(metadata.capture.cssWidth * metadata.scale), Math.ceil(metadata.capture.cssHeight * metadata.scale)]);
  assert.equal(metadata.capture.lines.length, 10);
  assert.equal(metadata.cols, 40);
  assert.equal(metadata.capture.lines[5], 'draft: hello');
  assert.equal(metadata.capture.lines[6], 'ENTER_RECEIVED');
  assert.deepEqual(wide, [{ column: 12, text: '界', width: 2 }]);
  assert.ok(metadata.capture.widths.flat().every(cell => cell.column + cell.width <= 40));
  assert.ok(ansi.includes(Buffer.from('\x1b[38;2;29;211;176m')));
  assert.equal(text.includes('\x1b'), false);
  assert.equal(metadata.cleanup.browserExited, true);
  assert.equal(metadata.cleanup.serverClosed, true);
  assert.equal(metadata.cleanup.portRebindVerified, true);
  assert.deepEqual(metadata.cleanup.errors, []);
  if (metadata.command) {
    assert.equal(metadata.cleanup.ptyExited, true);
    assert.equal(metadata.cleanup.ptyProcessGroupExited, true);
  }
  report.push({ directory, dimensions, rows: 10, columns: 40, wide, cleanup: metadata.cleanup });
}
for (const filename of ['terminal.png', 'terminal.txt', 'terminal-ansi.txt']) {
  assert.deepEqual(await readFile(join(captureDir, filename)), await readFile(join(replayDir, filename)));
}
await writeFile(join(captureDir, 'verification.json'), JSON.stringify({ result: 'PASS', invocation: process.argv,
  scenarios: report, replayIdentical: ['terminal.png', 'terminal.txt', 'terminal-ansi.txt'] }, null, 2) + '\n');
console.log('PASS: real PTY fixture, input editing, 40x10 geometry, wide cells, replay identity, cleanup');
