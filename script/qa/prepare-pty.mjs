import { chmod, access } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';

// node-pty 1.1.0 publishes its POSIX prebuilt spawn-helper without an executable bit.
if (process.platform !== 'win32') {
  const root = dirname(createRequire(import.meta.url).resolve('node-pty/package.json'));
  for (const folder of [`prebuilds/${process.platform}-${process.arch}`, 'build/Release']) {
    const helper = join(root, folder, 'spawn-helper');
    try { await access(helper); }
    catch (error) { if (error.code === 'ENOENT') continue; throw error; }
    await chmod(helper, 0o755);
  }
}
