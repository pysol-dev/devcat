# Animation release gates

From the repository root, run:

```sh
jq -e -f release/validate-animation-gates.jq release/animation-gates.json
```

Exit 0 with `true` means the manifest is structurally valid. It does not certify
that a host passed. All gates start pending and utilities remain locked.

Gates pass in the order listed in `animation-gates.json`. Utilities may unlock
only after the foundation and all four host/compatibility gates pass. A blocked
gate needs a nonempty `blocker`; pending and blocked gates have empty evidence.

A passing gate needs at least one evidence record with these fields:

- `wrapper_commit`: full lowercase Git commit SHA.
- `artwork_sha256`: full lowercase artwork SHA-256.
- `child_executable`, `child_version`, `os`, `terminal`, `terminal_version`,
  `renderer`: the exact tested installation and environment.
- `exact_test`: scenario and binary pass condition from `PLAN.md`.
- `commands`: nonempty array of exact invocations.
- `screenshot`, `recording`, `cleanup`: paths to the actual screenshot, short
  unedited recording, and teardown receipt.
- `result`: `pass`.

Use multiple records to cover distinct environments. Reviewers must inspect the
referenced artifacts and confirm full scenario coverage against `PLAN.md` before
accepting a pass. The validator checks record structure, not artifact existence,
authenticity, terminal behavior, or compatibility coverage. Run it as a release
check; it does not yet scan source paths or install a CI workflow.
