# Controlled composer fixture

This synthetic PTY host exercises the fixture compositor profile. It is not evidence of compatibility with Hermes, Claude, or Codex.

```sh
cargo +1.96.0 build --example composer-fixture
target/debug/examples/composer-fixture --exit-code 37
```

Run in a real terminal. Type a draft; Backspace deletes its last Unicode scalar. Bracketed paste appends text as one draft. Ctrl+O toggles a modal occupying the safe area. Ctrl+T toggles alternate screen. Ctrl+D or Ctrl+C exits with the configured status (default 37). The final `FIXTURE_RECEIPT:` JSON contains only the synthetic bytes, draft, terminal size, and requested exit status.

At 80x24 the status bar is row 18, the composer starts on row 20, and the bottom rule occupies row 23. Columns 63–78 on rows 13–17 are blank with default style. At 120x40 those rows move to 34, 36, 39 and the safe area becomes columns 103–118 on rows 29–33. Dimensions come from the PTY; resize redraw is polled every 50 ms. The draft viewport is two rows and preserves the complete draft in the exit receipt.

Build the example before testing, because Cargo integration tests do not rebuild example executables automatically:

```sh
cargo +1.96.0 build --example composer-fixture
BONGO_EVIDENCE_DIR=/absolute/current-attempt/composer-fixture cargo +1.96.0 test --test composer_fixture -- --nocapture
cargo +1.96.0 clippy --example composer-fixture --test composer_fixture -- -D warnings
```

The test drives an actual PTY and saves `terminal.raw`, an escaped transcript, parsed screen captures for each transition, `receipt.json`, and `cleanup.txt`. Test input is synthetic. These artifacts validate the fixture; screenshot and video validation of the compositor or stock hosts is a separate acceptance step.
