import { fork } from 'node:child_process';

if (!process.stdin.isTTY || !process.stdout.isTTY) throw new Error('Fixture requires a real PTY');
if (process.argv[2] === 'child') {
  process.on('SIGTERM', () => {});
  process.on('SIGHUP', () => {});
  setInterval(() => {}, 1000);
  process.send({ ready: true, pid: process.pid });
} else {
  const child = fork(new URL(import.meta.url), ['child'], { stdio: ['inherit', 'inherit', 'inherit', 'ipc'] });
  child.once('message', message => {
    process.stdout.write(`FORK_READY parent=${process.pid} child=${message.pid}\r\n`);
  });
  // The group leader exits before its deliberately stubborn descendant.
  process.on('SIGTERM', () => process.exit(0));
}
