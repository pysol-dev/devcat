#!/usr/bin/env node
import { StringDecoder } from 'node:string_decoder';
if (!process.stdin.isTTY || !process.stdout.isTTY) throw new Error('Fixture requires a real PTY');
process.stdin.setRawMode(true);
const decoder = new StringDecoder('utf8');
let draft = '';
process.stdout.write('\x1b[2J\x1b[H\x1b[?25l┌──────────────────┐\r\n│ \x1b[38;2;29;211;176mTRUECOLOR\x1b[0m 界     │\r\n└──────────────────┘\r\nPTY_READY\r\n');
process.stdin.on('data', bytes => {
  for (const character of decoder.write(bytes)) {
    if (character === '\x03') process.exit(0);
    if (character === '\x7f') draft = Array.from(draft).slice(0, -1).join('');
    else if (character === '\r') process.stdout.write('\x1b[7;1HENTER_RECEIVED');
    else draft += character;
  }
  process.stdout.write(`\x1b[6;1H\x1b[2Kdraft: ${draft}`);
});
