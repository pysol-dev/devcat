# devcat

`bongo` is a local Rust PTY launcher for a small Bongo Cat beside an agent TUI's composer. It uses artwork derived from the original [VS Code BongoCat extension](https://marketplace.visualstudio.com/items?itemName=pixl-garden.BongoCat), with its source and license recorded in [THIRD_PARTY.md](THIRD_PARTY.md). The stock agent executable keeps its own installation, configuration, authentication, arguments, working directory, and exit status.

## Build and run

```sh
cargo +1.96.0 build --release
./target/release/bongo --renderer cells -- hermes --tui
```

The confirmed stock host is Hermes Agent v0.20.0 at 120×40 cells. `--renderer auto` (the default) asks the terminal whether it supports Kitty graphics and uses cell art if it does not answer; `--renderer kitty` makes the same bounded attempt. Neither option assumes support from `TERM`. `--no-cat` forwards the PTY without an overlay. Use `bongo doctor --host hermes` to inspect executable resolution and terminal dimensions; doctor does not perform graphics negotiation.

The launcher hides the cat when it cannot prove an unobstructed composer bar. For this reason, a missing cat on an unvalidated host is expected. Stock Codex CLI v0.158.0 has an observed 120×40 cell profile; its full release gate is still pending. Claude Code, native Kitty rendering, and shared platform coverage remain gated by live tests. The current status and evidence requirements are in [PLAN.md](PLAN.md) and [release/animation-gates.json](release/animation-gates.json). Hyperpower effects are reserved until the animation gates pass.

## Reproduce the controlled terminal test

```sh
cargo +1.96.0 build --examples
BONGO_TEST_PROFILE=fixture ./target/debug/bongo --renderer cells -- ./target/debug/examples/composer-fixture
```

Type `abcdef`, press Backspace, and press Ctrl+D to exit the fixture. `BONGO_TEST_PROFILE` only enables the bundled fixture executable. The fixture and the PTY tests use sanitized text; no prompt capture or logging is part of the launcher. Run `cargo +1.96.0 test --workspace`, `cargo +1.96.0 clippy --all-targets -- -D warnings`, and `cargo +1.96.0 fmt --check` for automated checks. Terminal capture tooling and its separate dependencies are documented in [script/qa/README.md](script/qa/README.md).
