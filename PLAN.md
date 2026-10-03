# devcat: Bongo Cat terminal launcher plan

Source of truth: ChatGPT Library `BONGO_TERMINAL_ARCHITECTURE.md` (`libfile_7acb9ede3a848191a2c707d42c509437`), read 2026-10-02. This plan follows its decision record over the older Library `PLAN.md` and `BONGO_WRAPPER_SPEC.md` where they differ. Implementation evidence belongs under `evidence/` locally and in `release/animation-gates.json`; a reported prototype is not a passing gate.

## Product contract

`bongo hermes --tui`, `bongo claude`, and `bongo codex` launch each stock executable inside the same terminal window. The wrapper owns a PTY, forwards original input bytes once and in order, reconstructs the child's visible screen, and draws a small cat at a safe composer bar. The cat uses the original pixl-garden VS Code extension artwork: idle `bc`, left strike `dc`, right strike `ba` in its font. Composing keys alternate paws; Backspace strikes; paste gives one bounded strike; navigation, mouse, focus, and terminal replies do not. The cat hides when placement is uncertain or an approval, password field, nested editor, or modal could be covered.

The child remains the user's existing installation, authentication, configuration, updates, cwd, environment, arguments, and exit status. The launcher needs no global keyboard hook, patched agent binary, terminal extension, required tmux pane, browser terminal, background daemon, or prompt logging. A Unicode/ANSI cell version is the baseline. Raster output is claimed only for an exact graphics protocol and terminal combination that passes a live test. Hyperpower, HUDs, streaks, chat, history, app pets, and all other utilities remain out of scope until the four animation gates pass.

## Release gates and order

| Gate | Version | Required observable evidence | Initial state |
| --- | --- | --- | --- |
| Controlled foundation | 0.0.1 | Original asset audit and deterministic frames; PTY fixture input/output parity; safe redraw, mode and lifecycle cleanup | pending |
| Stock Hermes | 0.1.0 | `hermes --tui` with six slow composing keys and Backspace visibly alternating paws on its information bar; preserved draft, paste, streaming, modal, resize, idle and exit | pending |
| Stock Claude Code | 0.2.0 | Same real, authenticated host contract on the observed composer border; login screen cannot pass | pending |
| Stock Codex CLI | 0.3.0 | Same real host contract in modern and narrow layouts; native Codex pet cannot substitute | pending |
| Shared compatibility | 0.4.0 | Tested macOS arm64 and Linux x86-64, one cell-only and one graphics-capable terminal, dark/light, 80x24, 120x40, narrow height, paste, Unicode, mouse, dialogs, SSH, signals, child exit, and renderer failure | pending |

The first implementation milestone is 0.0.1 followed by Hermes 0.1.0. Do not claim or version later gates early. After Hermes passes, test Claude and then Codex. `release/animation-gates.json` starts with every gate pending and `utilities_unlocked: false`; a pass records wrapper commit, artwork hash, child executable/version, OS, terminal/version, renderer, exact commands, screenshot, short unedited recording, result, and cleanup. Missing credentials or hardware make a gate blocked, never passed. CI must reject utility feature paths while any gate is pending or blocked.

After all four animation and compatibility gates pass, evaluate the requested Hyperpower screen and typing animation as an optional effect. It needs its own real-terminal motion capture, input-latency measurement, reduced-motion behavior, and regression run across the three stock hosts. It must not delay or replace paw strikes. No Hyperpower implementation belongs in the foundation milestones.

## Build sequence

### 1. Audit and render the cat

Pin `kitgore/BongoCat` at `20e59c96cd25e4dcc098a5ab908bb27cdd4caabc`. Retain `bongocat.woff` and the full upstream MIT notice in `assets/source/` and `THIRD_PARTY.md`; verify WOFF SHA-256 `5b9bc74991b8abdb04785e4ef16db86bef867d5b19fa60444dbca11230224225`. Inspect `bongocat-theme.json` and `src/extension.ts` at that commit. Generate transparent RGBA idle, left, and right frames at one fixed canvas and baseline, plus deterministic truecolor half-block cell frames and a recognizable monochrome fallback. Record generator version, glyph mapping, palette, size, and output hashes in a manifest. Preview each pose on dark and light terminal backgrounds and inspect the paw contact point on the bar. Do not introduce replacement drum-cat art.

### 2. Establish transport and screen parity

Create only the Rust workspace parts the foundation needs: CLI, PTY session, input reducer, screen model, anchor detector, and cell renderer. Define `InputEvent { kind, monotonic_time }` without character payload, `Anchor { row, col, width, bar_row, safe_rect, confidence }`, `RenderCapability`, and `HideReason`. Inspect `twrap` and `baeru` as references; evaluate `portable-pty` and `vt100` with fixture sequences before pinning their versions. Do not copy the Python probe's byte counting, forced `TERM`, prompt glyph detection, style-flattening repaint, or destructive child cleanup.

The PTY has one output owner. Forward raw bytes promptly; classify an observation copy incrementally across UTF-8 and escape-sequence chunk boundaries. Preserve paste delimiters, control keys, mouse/focus reports, terminal query replies, keyboard enhancement, original `TERM`/`COLORTERM`, and child exit status. Propagate resize (PTY and parser together), signals, EOF, and suspend/resume. Bound queues; drop/coalesce animation before delaying input. On error, hide the cat or pass through safely and restore raw mode, cursor, alternate screen, palette, and Bongo-owned images.

The fixture exercises primary/alternate screen, styles, cursor motion, scrolling, combining/wide graphemes, multiline composer, modal, nested editor, paste, mouse/focus, keyboard queries, rapid redraw, and repeated resize. Compare output against the unwrapped child for every cell outside cat bounds, and verify exact input bytes and exit status. A parser text dump alone does not prove parity.

### 3. Implement conservative host profiles

Profiles recognize the visible installed TUI using geometry, border/status style, cursor position, and nearby rows, never prompt text. Recompute the anchor after material redraw and resize. Draw only in safe cells, erase prior bounds, preserve child colors/text outside the cat, and restore the cursor. Unknown versions, missing composer, modal, approval, password prompt, nested editor, or overlap return `Hidden(reason)` while the child continues. Test Hermes at 80x24, 120x40, and narrow height first, then apply the same contract to Claude and Codex.

### 4. Validate graphics, then distribution

Implement a bounded Kitty graphics query and one Kitty raster backend after cell mode works. Reuse Bongo image IDs and explicitly delete placements when the anchor moves or hides; fall back to cells on unsupported or failed graphics. Tmux and Zellij default to cells until a precise passthrough path is tested. iTerm2 and Sixel are later backends, each requiring their own terminal test. `bongo doctor --host <name>` reports executable, terminal stack, dimensions, selected renderer, and validation limits without credentials or screen text. `bongo --renderer cells` and `bongo --no-cat` are diagnostics. Publishing channels follow the real-host gate; a future npm wrapper may distribute a checksum-pinned binary but is never the renderer or a runtime dependency.

## Exact acceptance sessions

Run through a real PTY and screenshot/video the actual terminal, with sanitized test text only. For each stock host, type six slow characters before Enter, then Backspace and more text; both paws must alternate while the same draft reaches the host as in an unwrapped run. Rapid typing must return to idle without replay. Engineering targets are added key-to-visible-paw p95 under 50 ms and input transport p95 under 5 ms on a disclosed local setup; publish measurements, not assumptions. Repeat with paste, UTF-8 and combining characters, wide emoji, multiline input, navigation, terminal replies, mouse/focus, resize, scrolling, streaming, approval/modal, alternate screen, nested editor, Ctrl+C, EOF, suspend/resume, renderer failure, and child exit. The cat must stay aligned or hide, and child behavior must remain usable. Capture an unedited short video, screenshots, exact versions, hashes, and teardown receipt. Automated tests support these observations but cannot replace them.

## Repo guidance and completion

Run `omo:init-deep` after the actual source tree exists: a root `AGENTS.md` is required, and child files are added only when directory complexity and distinct conventions justify them. Keep instructions grounded in verified commands and code structure. Commit each verified increment atomically. Stop foundation work when the real Hermes gate passes; continue Claude, Codex, and shared compatibility in order only when the required host and environment are available. Do not unlock Hyperpower until all four gates are proved.
