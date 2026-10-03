# Runtime source map

This directory owns the `bongo` executable's terminal behavior. Read the root `AGENTS.md` and `PLAN.md` for release gates; this file describes the seams inside `src/`.

## Entry and ownership

| File | Owns | Do not move into it |
| --- | --- | --- |
| `main.rs` | CLI dispatch and error-to-exit mapping | Host-specific screen rules |
| `cli.rs` | Wrapper flags and stock argument vector | Reinterpreting child flags after `--` |
| `doctor.rs` | Observed host/terminal capability report | Unproven compatibility claims |
| `terminal.rs` | Parent TTY raw-mode guard, restoration, size | Child rendering state |
| `pty.rs` | Child PTY, exact byte forwarding, signals/resize, sole parent-output loop | Persistent input text or a second stdout writer |
| `input.rs` | Incremental key category classifier | Retained character payloads |
| `animation.rs` | Bounded alternating paw state and idle deadline | A backlog of hits |
| `kitty.rs` | Bounded graphics negotiation and Bongo-owned image cleanup | Claims of untested native rendering |
| `screen.rs` | Protocol safety guard | Silent acceptance of unsupported terminal modes |
| `sync_batch.rs` | Bounded atomic forwarding for synchronized child redraws | Delaying input or unbounded output queues |
| `profile.rs` | Confidence-based safe rectangle for a known host layout | Prompt-body parsing or arbitrary offsets that cover controls |
| `compositor.rs` | Restore prior cat bounds, forward child output, draw frame, restore cursor/style | Direct terminal ownership outside `pty.rs` |

## Runtime flow

`pty::run` relays incoming bytes to the stock child before calling the observation classifier. The output loop is the only physical writer. Feed every child output chunk to the screen model in original order; protocol uncertainty disables overlay before it can corrupt the session. The compositor's initial profile is unconfirmed and hidden. The controlled fixture may opt in only through an explicit test gate bound to the fixture executable path. A fixture confirmation is never a Hermes confirmation.

The observed Codex CLI v0.158.0 profile requires its alternate-screen composer and footer at 120×40 or larger, plus empty default-style cells at the right edge. It owns a short divider under the cat and restores those footer cells before a modal or child redraw. Unknown layouts and narrow terminals hide the cat.

Before a planned resize, remove any owned cell overlay at its old coordinates; then resize both PTY and screen model. After an unsolicited reflow, do not erase stale coordinates blindly. Wait for fresh complete chrome and an empty safe rectangle before drawing again. Modal or approval content can leave a status row visible while the composer is unavailable, so bar presence alone is insufficient.

## Checks that exercise these seams

```sh
cargo +1.96.0 test --test transport --test lifecycle
cargo +1.96.0 test --test composer_fixture
cargo +1.96.0 test --test compositor_transport
cargo +1.96.0 clippy --all-targets -- -D warnings
```

`tests/fixtures/transport.rs` proves byte/env/exit parity without art. `tests/fixtures/composer.rs` provides status, composer, modal, resize, and alternate-screen transitions with synthetic input and a receipt. Real host animation needs xterm.js screenshots/recording and a truthful entry in `release/animation-gates.json`; unit tests or fixture images cannot establish that gate.
