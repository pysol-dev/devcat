import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { resolve, join } from 'node:path';

const directory = resolve(process.argv[2] ?? '.omo/evidence/forking-cleanup');
await mkdir(directory, { recursive: true });
const invocation = ['script/qa/web-terminal-visual-qa.mjs', '--command', 'node script/qa/forking-fixture.mjs',
  '--wait-for', 'FORK_READY', '--settle-ms', '100', '--cols', '60', '--rows', '10', '--evidence-dir', directory];
// Given a real PTY whose group leader exits before its SIGTERM-resistant child.
const result = spawnSync(process.execPath, invocation, { encoding: 'utf8', timeout: 20000 });
await writeFile(join(directory, 'runner.json'), JSON.stringify({ invocation, status: result.status,
  stdout: result.stdout, stderr: result.stderr, error: result.error?.message ?? null }, null, 2) + '\n');
// When the capture harness closes the process group.
const metadata = JSON.parse(await readFile(join(directory, 'metadata.json'), 'utf8'));
const text = await readFile(join(directory, 'terminal.txt'), 'utf8');
const match = /FORK_READY parent=(\d+) child=(\d+)/.exec(text);
// Then both PIDs and the complete process group are gone before successful return.
assert.equal(result.status, 0, result.stderr);
assert.ok(match, 'Both real process IDs must be captured');
for (const pid of [Number(match[1]), Number(match[2]), -metadata.ptyPid, metadata.browserPid]) {
  assert.throws(() => process.kill(pid, 0), { code: 'ESRCH' });
}
assert.equal(metadata.cleanup.ptyExited, true);
assert.equal(metadata.cleanup.ptyProcessGroupExited, true);
assert.deepEqual(metadata.cleanup.ptySignals, ['SIGTERM', 'SIGKILL']);
assert.equal(metadata.cleanup.browserExited, true);
assert.equal(metadata.cleanup.portRebindVerified, true);
assert.deepEqual(metadata.cleanup.errors, []);
await writeFile(join(directory, 'verified.json'), JSON.stringify({ result: 'PASS', invocation,
  absentPids: [Number(match[1]), Number(match[2]), metadata.browserPid], absentGroup: metadata.ptyPid,
  cleanup: metadata.cleanup }, null, 2) + '\n');
console.log('PASS: forking fixture and stubborn child reaped before success');
