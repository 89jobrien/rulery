# Active Context

## Current focus

- Close out and commit the completed Rulery v0.1 implementation on `feat/rulery-v0-1`.
- Restore release readiness before version impact analysis: fix `cargo clippy --workspace --all-targets -- -D warnings` findings and resolve mixed-scope Cargo Rail documents.

## Current state

- Godmode graph: 70 done, 0 running, 0 pending, 0 blocked (`.ctx/godmode/tasks.yaml`).
- Standard gates pass: `cargo fmt --all`, `cargo clippy --workspace -- -D warnings`, `cargo nextest run --workspace` (65 tests), and `cargo doc --workspace --no-deps`.
- Public GitHub repo exists at `https://github.com/89jobrien/rulery`; `main` and `feat/rulery-v0-1` track the `github` remote.
- The full implementation is committed and pushed as `c25bb58` on `feat/rulery-v0-1`; the working tree is clean.

## Release blockers

- All-target clippy currently reports test-only findings in contracts hashes, vocabulary validation, emit JSON tests, IR wire tests, facade/assembly tests, and xtask verify tests.
- No release tags exist, so `v0.1.0` is the likely initial tag but still requires explicit confirmation.
- Cargo Rail plan/design files were included in `c25bb58`; confirm they belong in the release scope.

## Decisions

- GitHub commands must unset inherited `GH_TOKEN`/`GITHUB_TOKEN` so the authenticated keyring account is used.
- Release impact/version bump work must not begin until readiness passes.
- No source files should be reopened by evaluate/analyze/diff/scenario facade operations; they consume compiled packages.

## Open questions

- Include or split the Cargo Rail release-planning files?
- Confirm the initial release target as workspace version/tag `0.1.0`/`v0.1.0`.
