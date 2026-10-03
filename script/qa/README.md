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

Video is **off by default**. Add `--record-video` to a live `--command` run to
save an unedited `terminal.webm` of that browser session, including the actual
PTY output and timed inputs. Replay with `--from-file` cannot record video.
Use synthetic text: opting in records everything visible during the session.
Install the development-only encoder once with
`node script/qa/node_modules/playwright-core/cli.js install ffmpeg` and have
`ffprobe` on PATH. No additional dependency is needed by `bongo`.
The recording includes browser setup through the final screen; the video canvas is sized
to the largest starting or requested resize geometry before the PTY starts, so later
expansion is not clipped. Smaller terminal states leave unused canvas space. The separate PNG keeps the requested pixel scale.
Metadata records the video path, SHA-256, duration in seconds, byte size, codec,
and dimensions. Context closure finalizes the WebM before browser teardown;
recording or probe failures produce a nonzero exit and still run cleanup.

Add `--record-timing` to a live run to save `timing.json`: elapsed times and
byte offsets for PTY output chunks, plus input write times and byte lengths.
It records no input text beyond the existing opt-in invocation/ANSI artifacts.
Use the offsets with `terminal-ansi.txt` to see whether an erase and replacement
frame arrived in different terminal chunks. Replay cannot record timing.

Verify recording with `node script/qa/verify-video.mjs <evidence-dir>` (requires
`ffmpeg` and `ffprobe`); this runs six slow keys plus Backspace/Enter in the safe
fixture, checks decoded motion frames and metadata, and records cleanup.

`--from-file path/to/raw.ansi` replays bytes through the same xterm.js renderer.
`--input` accepts literal text or the key notation listed by `--help`; shell
ANSI quoting can send bracketed paste. `--settle-ms` controls the delay before
capture, and `--input-delay-ms` controls time between inputs. `--wait-for` is an
optional raw-output readiness marker, bounded by `--timeout-ms`.

Insert `--input '{Resize:80x24}'` between keys to resize both the live PTY and
xterm.js at that point in the sequence. The exact grammar is `{Resize:COLSxROWS}`,
with decimal integers (no leading zeros), 2–500 columns and 2–200 rows. Invalid
resize tokens fail before spawning resources. Each action uses the same input
delay. For example, start at `--cols 120 --rows 40`, then pass
`--input a --input b --input c --input '{Resize:80x24}' --input d --input '{Resize:120x40}'`.
Metadata `resizeEvents` records old and applied PTY/xterm sizes, input index,
wall-clock time, elapsed request/application/observation times from sequence start,
and the screen after the action delay. `capture.cols/rows` identify the final
geometry; top-level `cols/rows` remain the starting geometry. Raw ANSI alone does
not encode PTY resize events, so replaying a resized run requires those events.
Run `node script/qa/verify-resize.mjs <evidence-dir>` for the controlled composer
resize, recording, malformed-token and cleanup checks.

The harness snapshots the active screen before terminating the command. It
writes `terminal.png`, `terminal.txt`, `terminal-ansi.txt`, and `metadata.json`.
Metadata includes the invocation, dependency/browser versions, PTY exit,
xterm cell widths, and a cleanup receipt verifying browser/PTY termination and
successful rebinding of the closed ephemeral HTTP port. It exits nonzero when
capture or cleanup fails. ANSI is a raw stream; text is the final viewport,
not a regex-stripped transcript. xterm.js reproduces terminal cell rendering;
it is not evidence of Kitty/iTerm raster protocol support.

PTY teardown waits up to 1.5 seconds after SIGTERM for both the direct child
and its process group to disappear, then up to 1.5 seconds after SIGKILL.
The direct child's exit alone does not prove descendant cleanup. Run the
forking regression with `node script/qa/verify-forking-cleanup.mjs <evidence-dir>`;
it launches a real PTY whose child ignores SIGTERM, and checks both fixture PIDs,
the group, browser, and port cleanup before recording `verified.json`.

References: [node-pty](https://github.com/microsoft/node-pty),
[xterm.js](https://xtermjs.org/docs/),
[Playwright BrowserType](https://playwright.dev/docs/api/class-browsertype).
