# Terminal capture (development only)

Install with `pnpm --dir script/qa install --frozen-lockfile`, then run
`pnpm --dir script/qa run postinstall` (some pnpm versions skip project hooks).
Requires Node.js,
a POSIX host, and installed Google Chrome. Use `--browser /path/to/chromium`
to select another Chromium executable. These packages are independent of the
Rust build and are never needed to run `bongo`.

From the repository root:

```sh
node script/qa/web-terminal-visual-qa.mjs --title 'PTY fixture' \
  --command 'node script/qa/fixture.mjs' --wait-for PTY_READY \
  --input 'hello界' --input '{Backspace}' --input '{Enter}' \
  --cols 40 --rows 10 --evidence-dir .omo/evidence/terminal-qa
```

`--from-file path/to/raw.ansi` replays bytes through the same xterm.js renderer.
`--input` accepts literal text or the key notation listed by `--help`; shell
ANSI quoting can send bracketed paste. `--settle-ms` controls the delay before
capture, and `--input-delay-ms` controls time between inputs. `--wait-for` is an
optional raw-output readiness marker, bounded by `--timeout-ms`.

The harness snapshots the active screen before terminating the command. It
writes `terminal.png`, `terminal.txt`, `terminal-ansi.txt`, and `metadata.json`.
Metadata includes the invocation, dependency/browser versions, PTY exit,
xterm cell widths, and a cleanup receipt verifying browser/PTY termination and
successful rebinding of the closed ephemeral HTTP port. It exits nonzero when
capture or cleanup fails. ANSI is a raw stream; text is the final viewport,
not a regex-stripped transcript. xterm.js reproduces terminal cell rendering;
it is not evidence of Kitty/iTerm raster protocol support.

References: [node-pty](https://github.com/microsoft/node-pty),
[xterm.js](https://xtermjs.org/docs/),
[Playwright BrowserType](https://playwright.dev/docs/api/class-browsertype).
