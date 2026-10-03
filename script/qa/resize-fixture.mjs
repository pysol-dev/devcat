#!/usr/bin/env node
if (!process.stdin.isTTY || !process.stdout.isTTY) throw new Error('Fixture requires a real PTY');
process.stdin.setRawMode(true);
let draft = '';
const history = [];
const render = event => {
  const { columns, rows } = process.stdout;
  history.push(`${event} ${columns}x${rows} draft=${draft}`);
  process.stdout.write('\x1b[2J\x1b[H\x1b[?25lRESIZE_READY\r\n' + history.join('\r\n') +
    `\x1b[${rows};1H` + 'L' + '-'.repeat(columns - 2) + 'R');
};
process.stdout.on('resize', () => render('RESIZED'));
process.stdin.on('data', bytes => {
  for (const character of bytes.toString('utf8')) {
    if (character === '\r') {
      render('EXIT');
      process.stdin.setRawMode(false);
      process.exit(0);
    }
    if (!/^[a-z]$/.test(character)) throw new Error('Unexpected fixture input');
    draft += character;
    render('KEY');
  }
});
render('START');
