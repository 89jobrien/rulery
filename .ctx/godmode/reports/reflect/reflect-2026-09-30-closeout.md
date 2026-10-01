# Session Reflection — 2026-09-30 16:00

## Shipped

Two distinct bodies of work, in four repositories.

**`rulery` — gap programme closed and pushed (33 commits, `c4facfa..7f29a71`).** All ten
prioritised gaps plus the four specification `Required conformance gates` bullets that were unmet at
session start. Latent defects found off-plan: text-partition domains permanently blocked
completeness, a null record assignment voided whole decisions, `StructuralChange::ImportIntegrity`
masked every semantic-diff classification, record-typed vocabulary fields collapsed to
`CheckedType::Text`, and `xtask` had never linted five integration suites. Added a `Doctest` verify
gate, proptest laws for the four-valued truth algebra and canonical hashing, `insta` snapshots, a
`criterion` benchmark, the two required `examples/` fixtures, and a spec gate-bullet check.

**Agent tooling.** Removed `git add -A` as a standing global mandate from the notfiles-managed
`CLAUDE.md`, and fixed `daily-orchestration`'s fix-agent to trigger auto-commit on its own recorded
path list rather than a global dirty check.

**Recovered `model-ingress-redaction` (3 of 6 tasks now committed).** `personal-mcp` `00caa12` on
`~/dev/personal-mcp`, `devloop` `994e96d`, both merged onto current `develop`.

## Unfinished

- Three tasks remain in `model-ingress-redaction`: `opencode-lifecycle` (active),
  `opencode-fail-closed`, `lifecycle-canary`. All OpenCode plugin work, which the design excludes
  from crates.io.
- `devloop` has 70 uncommitted markdown files — formatter output, no code. Left deliberately.
- `personal-mcp`'s `baml-log-lab.sh` resolves an unpinned `bunx`, while `generators.baml` pins
  `version "0.219.0"`. Latest CLI fails with `Client generation failed`; pinned works. Flagged, not
  fixed — it belongs to whoever owns the BAML work.
- `rulery` is on `fix/msrv-1.98` with one unpushed commit that is not mine.
- One unpairable analyzer cell still voids a whole decision. Deferred deliberately: no honest failing
  test exists, and fixing it needs a fixture that provokes an unpairable cell by another route.

## Patterns & Surprises

### Took longer than expected

- **personal-mcp's build was broken three ways, none of them mine.** `build.rs` claimed its stub
  "allow[s] compilation to succeed" while generating `paths: {}`, which cannot produce the
  `analyze_log_event` method `baml.rs` calls. The generator also failed outright. Fixing it took
  finding that unpinned `bunx` resolves 0.226.2 against a 0.219.0 generator target.
- **Four merge conflicts in personal-mcp**, all additive. Mechanical once read, but the merge had to
  be driven twice because the first attempt left the index in a conflicted state I did not check.
- **Nushell, again.** Roughly a dozen parse failures this session despite the mistake ledger already
  documenting them. Literal parentheses inside `$"..."` are read as interpolation; `2>&1` and `&&`
  remain invalid; `>` is passed to `cargo` as a literal filename argument.

### Went smoothly

- The failing-test-first loop held throughout `rulery`. Every RED was for the right reason with no
  incidental compile noise.
- `verify` and the pre-commit hooks caught real defects in my own new code on three separate
  occasions: eight lints in the criterion bench, four in the truth and hash tests, and an
  indentation regression in a hand-resolved conflict.
- The `setup_clone` control group immediately disproved my own hypothesis that `specs.clone()`
  contaminated the benchmark — cheaper than being wrong for another hour.
- Both redaction repos compiled and passed on the first gate run once `personal-mcp`'s build
  prerequisites were fixed. `devloop` needed nothing at all.

### Discovered mid-session

- **The four-valued algebra is not a lattice.** Absorption fails and _both_ distributivity laws fail,
  because conjunction is `min` under `False < Invalid < Unknown < True` while disjunction is `max`
  under `False < Unknown < Invalid < True`. The orders disagree on exactly one pair. Recorded as law
  with proptest-produced counterexamples rather than "fixed", since the spec's tables define the
  evaluation semantics.
- **Two partition performance defects**, both matching their measured milliseconds to within a few
  percent: a `BTreeMap` clone chain worth 524,800 element clones at 1024 paths, and `sort_by_key`
  recomputing a key that serializes the whole cell on every comparison.
- **`collect_test_names` reads literal `fn <name>(` source lines**, because the spec's traceability
  tables name tests by source identifier. That is the architectural reason `rstest` cannot be used,
  independent of the table-driven-loop style rule.
- **The pre-commit hook was the cause of a mystery test failure.** Both new `examples/` fixtures
  passed standalone, then failed under `--workspace` — prettier had rewritten them between runs,
  exiting 0 and printing a green check. Fixed repo-locally with `.prettierignore`.
- **`godcommit add -- <pathspec>` reads the working tree, not the index.** I used it precisely
  because I had just identified that hazard, and it swept another agent's uncommitted 1Password work
  into my commit. Explicit-path staging and explicit-path _committing_ are not the same operation.

### Next session speedups

- **Before recommending any deletion or prune, look for siblings.** A worktree named for a feature is
  evidence the feature exists. I judged `model-ingress-redaction` disposable from one tree's contents
  when it was one of four repos holding the only copy of unfinished security work.
- **Assert no cause before stating one.** I named a BAML path mismatch twice, confidently, before
  running the generator. Both times the real cause was version skew. Read the config, then run the
  thing.
- **Check `git diff --cached --stat` before committing, not just after `git add`.** Ten files were
  already in the index from another agent; my `git add` added to a queue that was not mine.
- `whatidid` cannot see this session — it harvests `~/.claude/projects/**/*.jsonl` and this ran in
  opencode. Verify that before invoking it, not after.
- `rust-script`-style scratch scripts in Nushell are themselves a Nushell hazard: `$"($x | length)"`
  and literal parens both misparse. Precompute values into `let` bindings before interpolating.

## Task graph snapshot

`godmode task list`: 125 tasks, all `done`. 0 blocked, 0 pending, 0 running. See
`.ctx/godmode/tasks.yaml`.

The separate `model-ingress-redaction` graph lives at
`/Users/joe/dev/.ctx/_WORKING_DIR/model-ingress-redaction/tasks.yaml`: 3 done, 1 active, 2 pending.

## Open questions

- The three remaining OpenCode-plugin tasks need a decision on whether secret redaction belongs in
  the OpenCode plugin at all, given the design excludes it from crates.io and names plugin removal as
  a standing bypass.
- `personal-mcp`'s BAML build should check in the generated client rather than depend on a
  gitignored artifact plus a stub that cannot compile. Scanned 1,268 published `build.rs` files:
  zero use a stub-fallback; 145 ship their codegen input in the package.
- `rulery` is currently on another agent's `fix/msrv-1.98` with an unpushed commit. Decide whether
  that branch and any session-closeout branch merge first or independently.
