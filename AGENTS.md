# devcat project knowledge base

Generated: 2026-10-02. Baseline commit: `3582e56` on `main`. Update this file when the runtime or gate process changes.

## Overview

`devcat` builds the `bongo` Rust CLI: a same-terminal PTY launcher for unchanged Hermes, Claude Code, and Codex executables. The product contract and release order are in `PLAN.md`; the original VS Code cat font and attribution are in `assets/` and `THIRD_PARTY.md`.

## Structure

```text
src/                 CLI, PTY transport, terminal state, input, screen, profile, animation, compositor
tests/               Real PTY transport/lifecycle and controlled composer fixtures
assets/source/       Pinned upstream BongoCat WOFF, glyph map, MIT license
assets/generated/    Reproducible transparent PNG and cell frames
tools/               Pinned Python asset generator and checks
script/qa/           Development-only real PTY + xterm.js screenshot harness
release/             Gate manifest and structural validator
PLAN.md              Source-derived product scope and acceptance order
```

## Where to look

| Task | Location | Evidence limit |
| --- | --- | --- |
| Command parsing and doctor | `src/cli.rs`, `src/doctor.rs` | Doctor reports observed capability, not a host gate |
| PTY bytes, resize, signals | `src/pty.rs`, `src/terminal.rs` | `tests/transport.rs`, `tests/lifecycle.rs` exercise real PTYs |
| Key observation and paw timing | `src/input.rs`, `src/animation.rs` | Events must not retain typed characters |
| Screen safety and overlay | `src/screen.rs`, `src/profile.rs`, `src/compositor.rs` | Real terminal screenshots and outside-cat parity decide visual claims |
| Controlled host | `tests/fixtures/composer.rs`, `tests/composer_fixture.rs` | A fixture pass never counts as Hermes, Claude, or Codex |
| Art generation | `tools/generate_assets.py`, `assets/README.md` | WOFF hash and generated manifest must match |
| Gate status | `release/animation-gates.json`, `release/README.md` | Every pass needs actual host/version, screenshot, recording, cleanup |

## Code map

The Rust entry path is `main.rs` → CLI dispatch → `pty::run` → one parent-terminal output owner. The terminal guard restores input mode. The compositor observes a copy of input, models child output, and draws only at a confirmed safe anchor. `Profile::Unconfirmed` must hide. `Animation` alternates `Idle`, `Left`, and `Right`. The source currently has roughly 43 Rust function items across ten `src/` files by ast-grep; LSP symbol/reference requests timed out during initialization, so reference centrality is unmeasured. Check module declarations and re-run build/tests before assuming a newly added module is wired into the binary.

## Project-specific rules

- Primary order: controlled foundation, stock Hermes, authenticated stock Claude, stock Codex, shared compatibility. Do not mark a real-host gate passed from a fixture, parser dump, static screenshot, or login screen.
- Do not add Hyperpower, HUDs, notifications, history, or other utility code while `release/animation-gates.json` has a pending or blocked animation gate. `utilities_unlocked` stays false until the four host/compatibility gates pass.
- The child executable, cwd, environment, argument bytes, authentication, configuration, update path, and exit status stay stock. Do not patch agent binaries, intercept global keyboard devices, force `TERM`, or require terminal-specific plugins or a tmux pane.
- Forward original input bytes exactly once before classifying an observation copy. Do not save typed characters, prompts, credentials, or raw host screens by default. Evidence sessions use synthetic text and are sanitized before sharing.
- Only one writer may address the parent terminal. On uncertain layout, occupied cat rectangle, modal, approval, secret prompt, nested editor, unsupported sequence, or renderer failure, hide the cat while keeping the child usable. Recompute anchor after redraw/resize; preserve all non-cat cells and cursor.
- The shipped cat comes from pinned `assets/source/bongocat.woff` (SHA-256 in `THIRD_PARTY.md`), with glyph pairs `bc`, `dc`, `ba`. Cell mode is a lower-fidelity compatibility rendering; claim raster support only after a specific terminal/protocol combination passes live QA.
- `.omo/evidence/` and `evidence/` are local test artifacts, ignored by Git. Keep test inputs synthetic; record a teardown receipt for every spawned PTY, process group, browser, server, and port.

## Commands

```sh
cargo +1.96.0 fmt --check
cargo +1.96.0 clippy --all-targets -- -D warnings
cargo +1.96.0 test
cargo +1.96.0 build --example composer-fixture
uv run tools/generate_assets.py
uv run tools/test_assets.py
pnpm --dir script/qa install --frozen-lockfile
pnpm --dir script/qa run postinstall
node script/qa/web-terminal-visual-qa.mjs --help
jq -e -f release/validate-animation-gates.jq release/animation-gates.json
```

The installed default Rust is 1.83, which cannot parse this edition-2024 package; toolchain 1.96.0 is installed and was used for verification. The Node package under `script/qa/` is only for development evidence, never a runtime dependency of `bongo`. The release validator checks manifest structure and gate order; it cannot authenticate screenshots or prove compatibility by itself.
