---
version: 2
last_updated: 2026-09-30
next_review: 2026-10-07
---

# Recurring Mistakes Ledger

## Process Errors

### POSIX shell syntax used in Nushell command context

- **Occurrences**: 22 (3 on 2026-09-21, ~19 on 2026-09-30)
- **Dates**: 2026-09-21, 2026-09-30
- **Affected**: Every `Bash` tool call in this repo; report-directory creation; scratch-file
  comparisons; loop-based snapshot acceptance
- **Prevention**: Write the Nushell-correct form before sending, since these fail at parse time and
  cost a whole turn each. The reliable wrapper is `do { cmd } | complete`, then `get exit_code` or
  `get stderr`. Confirmed-failing forms: `2>&1`, `2>/dev/null`, `&&`, `||`, `mkdir -p`, `cp -R`,
  `ls -1`, `ls -la` with trailing args, `for x in a b; do`, `cat - file > out`, and
  `date +%F` used as a second positional.
- **Notes**: Every occurrence is a parse error with no side effect, so retrying is always safe. The
  frequency is the problem: this was relearned at least four separate times in one session after
  already having learned it. Treat the first `2>&1` of a session as a signal to re-read this entry.

### Heredoc inside command substitution corrupts commit messages

- **Occurrences**: 1
- **Dates**: 2026-09-30
- **Affected**: `git commit -m "$(cat <<'EOF' ... EOF )"`
- **Prevention**: Never pass a multi-line commit message inline. Write it to
  `.ctx/_WORKING_DIR/commit-<slug>.txt` and use `git commit -F <path>`.
- **Notes**: Severity is higher than the other process errors because the damage lands in git
  history. The heredoc did not expand: the literal `$(cat <<'EOF'` became part of the subject line
  and the whole message collapsed onto one line. Recovering required `git commit --amend`, which was
  only safe because the commit was unpushed and created moments earlier. If this ever happens to a
  pushed commit, the recovery is a rebase instead. The `-F` form has been used for every commit since
  and has not failed once.

### Tests pass standalone but fail under the full workspace run

- **Occurrences**: 1 (2 tests affected)
- **Dates**: 2026-09-30
- **Affected**: `crates/cli/tests/fixtures.rs` — `both_fixtures_are_already_in_canonical_authored_form`
  and `scaffolded_fixture_is_exactly_what_init_writes`
- **Prevention**: When a test passes under `-p <crate>` but fails under `--workspace`, suspect
  something that runs between the two invocations, not the test. Check for a formatter or hook that
  rewrites files on commit.
- **Notes**: Both fixtures passed `cargo nextest run -p rulery-cli --test fixtures`, then failed
  `cargo xtask verify` minutes later with no code change. The pre-commit hook had rewritten them in
  between (see the hook entry below). The signature — green locally, red in the full gate — is
  inherently invisible from inside a single command, so treat the discrepancy itself as the signal.

## Hook Conflicts

### Pre-commit `prettier --write` rewrites staged YAML and re-adds it

- **Occurrences**: 1 (2 fixtures corrupted)
- **Dates**: 2026-09-30
- **Affected**: Any byte-exact YAML artifact under version control — currently
  `examples/scaffolded-package/` and `examples/canonical-authored-form/`
- **Prevention**: Add the path to a repo-local `.prettierignore`. Prettier reads that file from the
  working directory, so no change to the shared hook at `~/.config/git/hooks/pre-commit` is needed.
  Read that hook before committing any generated or golden file.
- **Notes**: This is not a false positive — prettier is correctly formatting the file per its own
  rules. The conflict is that `rulery fmt` defines canonical authored form and emits block sequences
  at their parent's indentation, while prettier indents them. The hook already excludes
  `*HANDOFF*.yaml` for exactly this reason ("indent style fights hj's serde_yaml output"), so the
  hazard was known and I failed to connect it. The failure mode is silent: the hook exits 0, prints
  a green check, and reports success, so nothing signals that the bytes changed.

## Clippy Lints

### `missing_panics_doc` on production `expect`

- **Occurrences**: 1
- **Dates**: 2026-09-30
- **Affected**: `crates/analysis/src/partition.rs` (`build_partition`)
- **Prevention**: Write `if let (Some(a), Some(b)) = (x.pop(), y.first())` instead of
  `x.pop().expect("checked length")`. This repo has no `# Panics` sections, so the lint cannot be
  satisfied by documenting the panic.
- **Notes**: The `expect` was asserting an invariant the surrounding condition had already proven
  true. `AGENTS.md` prefers no production `expect` at all, and the lint is the mechanism that
  enforces it — a rare case where the linter and the written convention agree.

### Proc-macro test helpers expand undocumented functions

- **Occurrences**: 2 (`criterion_group!`, implicitly `rstest` in the same reasoning)
- **Dates**: 2026-09-30
- **Affected**: `crates/analysis/benches/partition_scaling.rs`
- **Prevention**: Prefer hand-written entry points over attribute macros that generate functions.
  `criterion_group!` costs four lines to replace with a `main` that calls each group; `criterion_main!`
  likewise. Any `#[allow]` needed to accommodate them would fall outside the set `AGENTS.md` permits.
- **Notes**: `missing_docs` and `clippy::missing_semicolon` both fired on the macro expansion. This
  is a general hazard for any test framework that generates test bodies in this workspace, and it is
  a second independent reason to decline `rstest` alongside the traceability-constraint one.

### Lints in freshly written test code caught by the widened gate

- **Occurrences**: 3 commits affected (10 lints total)
- **Dates**: 2026-09-30
- **Affected**: `crates/analysis/benches/partition_scaling.rs` (5), `crates/engine/src/truth.rs` (3),
  `crates/contracts/tests/hashes.rs` (2)
- **Prevention**: None needed. Run `cargo xtask verify` before committing rather than only
  `cargo nextest run`, so clippy sees benches and test targets.
- **Notes**: Listed for the trend rather than as a defect. These are expected lints in new code, found
  by the gate doing its job. The `--all-targets` widening landed earlier in the programme and kept
  paying off on code written after it.

## Analytical Errors

### Hand-derived counterexamples are less reliable than asking the property test

- **Occurrences**: 2
- **Dates**: 2026-09-30
- **Affected**: `crates/engine/src/truth.rs` distributivity laws
- **Prevention**: When a claim can be checked by a property test, write the property and let it
  answer, rather than deriving a witness by inspection and asserting on it.
- **Notes**: I asserted `Unknown or (True and Invalid) != (Unknown or True) and (Unknown or Invalid)`
  and the two sides were both `Invalid`. proptest then found the real counterexample
  (`value=Unknown, left=False, right=Invalid`), confirming _both_ distributivity directions fail
  while my hand-picked witness for the second one agreed. Three attempts were spent on that pair,
  the last two producing confidently wrong witnesses. The same pattern earlier in the session cost a
  turn on the benchmark: I hypothesised that `specs.clone()` contaminated the measurement, and the
  control benchmark disproved it. Both cases share one shape — a plausible mechanism asserted without
  a control.

## Test Failures

### Transient `LEAK` in subprocess-spawning suites

- **Occurrences**: 3 (2 distinct tests)
- **Dates**: 2026-09-30
- **Affected**: `crates/cli/tests/process_contract.rs`, `xtask::verify`
- **Prevention**: None applied. Reported and left alone deliberately.
- **Notes**: Never reproduced in isolation, at package scope, or across four-plus full runs. Correlated
  with concurrent cargo file-lock contention from another agent working in the same checkout. Does not
  fail the gate. Recorded so a future occurrence is recognised as known rather than re-investigated.

## Reverts

None this session.
