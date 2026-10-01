# Progress

## Completed

- Implemented all 70 Godmode tasks for Rulery v0.1.
- Completed stable contracts, strict wire envelopes, source parsing, vocabulary validation, checked IR, deterministic compilation, local package storage, lock handling, four-valued evaluation, temporal semantics, traces, scenarios, finite analysis, semantic diffs, renderers, declarative/procedural macros, CLI orchestration, normative fixture, and cross-cutting conformance.
- Hardened xtask with a typed workspace model, safe bootstrap reconciliation, aggregate specification checks, pure architecture validation, cycle detection, and exact fail-fast verification order.
- Added canonical tool-library fixtures and deterministic end-to-end deny explanation.
- Created and configured the public GitHub repository and branch upstreams.
- **2026-09-30: closed the ten-gap programme** (33 commits, `c4facfa..7f29a71`, merged to `main` as a
  fast-forward). 125/125 graph tasks done. Fixed five latent defects found off-plan, three of which
  meant a documented capability was not actually working: text-partition completeness, the null-record
  decision void, diff-classification masking, record/text type-unsoundness, and unlinted test suites.
- **2026-09-30: satisfied all four specification `Required conformance gates` bullets** that were
  unmet. Property and doctest suites now exist and are gated (`Doctest` is the eighth verify gate),
  renderer snapshots are pinned as reviewed insta files, and both `examples/` fixtures are checked in
  and compared byte for byte. `xtask conformance` now reads the required-gate list, so deleting a
  bullet fails the gate instead of quietly removing the debt.
- **2026-09-30: added `LICENSE-MIT` and `LICENSE-APACHE`**, matching the declared
  `MIT OR Apache-2.0`, with a test that derives its expectation from `[workspace.package]`.
- **2026-09-30: removed two partition performance defects** found by a new criterion benchmark —
  89× on the pathological case, and the residual cost is `n log n` inherent to the output type.
- **2026-09-30: rejected a vocabulary root nested under another** at resolution time. The
  alternative "prefer the longest matching root" was then proved a no-op, because two roots can only
  both prefix a path if one prefixes the other — which the new check forbids.
- **2026-09-30: removed `git add -A` as a standing global mandate** from the notfiles-managed
  `CLAUDE.md`, and changed `daily-orchestration`'s fix-agent to commit only the paths it recorded
  rather than whatever a global `git status` reports. Both edits are live through symlinks but
  uncommitted, on a branch owned by another agent.
- **2026-09-30: recovered `model-ingress-redaction`** — a four-repo secret-redaction feature whose
  implementation existed only as untracked source in two worktrees. `personal-mcp` `00caa12` (merged
  onto `develop`, 260 tests) and `devloop` `994e96d` (536 tests) are now committed. Required fixing
  personal-mcp's `build.rs` stub, which claimed to keep the crate compiling but could not satisfy
  `baml.rs`, and an unpinned `bunx` that resolves a BAML CLI too new for the pinned generator target.

## Verification

- `cargo xtask verify`: passing, all eight gates (bootstrap check, conformance, architecture, fmt,
  clippy, nextest, doctest, rustdoc), fail-fast in that order.
- `cargo fmt --all`: passing.
- `cargo clippy --workspace --all-targets -- -D warnings`: passing. This was **failing** as of the
  previous entry and is the reason the gap programme widened it; five integration suites had never
  been linted.
- `cargo nextest run --workspace --all-features`: 184 passing.
- `cargo test --workspace --all-features --doc`: 4 passing.
- `cargo doc --workspace --no-deps`: passing, with Cargo's known duplicate `rulery` doc-output warning
  for the library and CLI binary.

## In progress

- Nothing in `rulery`. The checkout is on `fix/msrv-1.98` with one unpushed commit that is not mine.
- `model-ingress-redaction`: three of six tasks committed; `opencode-lifecycle` is active and
  `opencode-fail-closed` and `lifecycle-canary` are pending. All three are OpenCode plugin work.
- `devloop` has 70 uncommitted markdown files in its feature worktree — formatter output, no code.

## Not started

- Release impact confirmation and version bump.
- Changelog/release notes for the new implementation.
- GitHub Actions validation. There is still no `.github/` directory; `cargo xtask verify` is the gate.
- `cargo-semver-checks`. No value until there is a published version to diff against; revisit after
  the first crates.io release.
- Checking `personal-mcp`'s generated BAML client into version control, so its build has no
  missing-input branch at all.
