# Close the ten v0.1 gap findings

## Goal

Close the ten gaps recorded as `gap1` through `gap10` in `.ctx/godmode/tasks.yaml`. One of them
(`gap1`) is a correctness defect in shipped output: `rulery analyze` evaluates every partition cell
at the Unix epoch, so a temporal condition resolves degenerately for any package carrying real dates.
The other nine are coverage, gate, and packaging gaps where behaviour is correct but nothing in the
workspace proves it.

Success is that each gap is closed by a test that fails before the change and passes after, and
`cargo xtask verify` is green at the end of every task. No task ends with a commit that leaves a
gate red.

## Architecture

- **Crates affected**: `xtask`, `rulery`, `rulery-cli`, `rulery-diagnostics`, `rulery-compiler`.
  No new workspace member, and no new crate edge — the dependency DAG in `xtask/src/model.rs:59-197`
  is already correct for every change below, so `xtask::architecture` needs no edit. A forbidden edge
  would be `XtaskError::ForbiddenDependency` and the fix is always to add the edge to `model.rs`,
  never to relax `architecture.rs`.
- **New traits/types**: none. Four additive public accessors
  (`Diagnostic::labels`, `DiagnosticProperties::confidence`, `DiagnosticReportV1::diagnostics`, and
  `xtask::gate_command`), one additive enum variant (`CheckedType::Record(StableId)`), one
  constructor (`ProductionApplication::with_time_zones`), and one gate-command change. The record
  variant is genuinely additive because `Value::Record(BTreeMap<StableId, Self>)` already exists at
  `crates/contracts/src/value.rs:65` and `TypeDeclaration::Record` already exists at
  `crates/vocabulary/src/model.rs:64-72` — only the compiler's checked type lagged.
- **Data flow**: unchanged for every gap except gap1 and gap5.
  - gap1 inserts one hop on the analysis path: CLI `--at` string → `UtcInstant::parse_rfc3339` →
    `ApplicationService::analyze(package, options, at)` → `PackageAnalyzer::new(&*self.time_zones, at)`
    → per-cell evaluations → witness identities. Omitting the flag still routes through
    `analysis_instant()`, so existing output is byte-identical.
  - gap5 changes the failure path only: `HostError` → `pre_artifact(command, status, message)` →
    a zero-diagnostic `DiagnosticReportEnvelope` in machine formats, stderr text in human formats.
  - gap4 removes three `serde_json` round trips from `crates/cli/src/findings.rs`, replacing
    serialization with direct accessor reads on data already resident in memory.
  - gap2, gap3, gap8 change only which commands run and which specification text is enforced; no
    production data flow moves.
- **Four seams, no task crosses a forbidden boundary**: the gate seam in `xtask`, the boundary seam
  from `rulery-diagnostics` to `rulery-cli`, the type-system seam from `rulery-contracts` to
  `rulery-compiler`, and the service seam from the `rulery` facade to `rulery-cli`.

## Tech Stack

- **Rust edition**: 2024, MSRV 1.85, 15-member workspace. Workspace lints set
  `missing_docs = "warn"` and `unsafe_code = "forbid"` with `clippy::all` and `clippy::pedantic` at
  `warn`, promoted to errors by `-D warnings`. Every manifest opts in via `[lints] workspace = true`.
- **New dependencies**: none. No `insta`, no `rstest`, no `proptest`, no benches. Testing is
  `cargo nextest` only. Error types use `thiserror`; wire structs use `serde` with
  `deny_unknown_fields`; collections are `BTreeMap`/`BTreeSet` for byte-deterministic output.
- **Conventions this plan must follow**: every public item and enum variant documented, every
  fallible public function carrying a `# Errors` section, `#[must_use]` on every pure getter,
  `let … else` over early return, inline format args, and test names in `<subject>_<behavior>`
  snake case with no `test` prefix and no `Type::method` nesting. The diagnostic registry stays at
  exactly forty codes.

## Tasks

### Task 1: Assert the analysis instant is configurable at the service boundary

**Crate**: `rulery`
**File(s)**: `src/application.rs`
**Run**: `cargo nextest run -p rulery analyze_honors_an_explicit_instant`

1. Add `analyze_honors_an_explicit_instant` to the `#[cfg(test)] mod tests` in `src/application.rs`.
   Reuse the fixture helpers already in that module, including `diff_package` at `:1070`. Build the
   package, call `ApplicationService::analyze` with a non-epoch `UtcInstant`, and assert the report
   records the supplied instant rather than the epoch.
2. Confirm FAIL: compile error, because `analyze` takes two arguments and no instant.
3. Implement nothing in this task; the compile error is the red state.
4. No commit — the tree is red by design and Task 2 makes it green.

### Task 2: Thread the instant through the analyze and diff service methods

**Crate**: `rulery`
**File(s)**: `src/application.rs`
**Run**: `cargo nextest run -p rulery`

1. Failing test: the test from Task 1.
2. Confirm FAIL: same compile error, arity mismatch on `analyze`.
3. Implement: add `at: UtcInstant` as the last parameter of `ApplicationService::analyze` at `:119`
   and `::diff` at `:131`. Change `ProductionApplication::analyzer` at `:186` to
   `fn analyzer(&self, at: UtcInstant) -> PackageAnalyzer<'_>`. Update the in-module tests at `:708`
   and `:871` to pass `analysis_instant()` so existing behaviour is unchanged, and update each doc
   comment to name the parameter while keeping its `# Errors` section intact.
4. Confirm GREEN: `cargo nextest run -p rulery` passes and
   `cargo clippy -p rulery --all-targets -- -D warnings` is clean.
5. `git commit -m "feat(rulery): thread an explicit instant through analyze and diff"`

### Task 3: Assert the CLI accepts an analysis instant

**Crate**: `rulery-cli`
**File(s)**: `crates/cli/src/args.rs`
**Run**: `cargo nextest run -p rulery-cli cli_parses_the_analysis_instant_flag`

1. Add `cli_parses_the_analysis_instant_flag` beside `cli_parses_v01_commands_and_defaults`. Assert
   that `Command::Analyze` and `Command::Diff` parse with `--at <rfc3339>` and that omission leaves
   the value `None`. Follow the construction style of the existing parser test.
2. Confirm FAIL: compile error, because neither variant has an `at` field.
3. Implement nothing; the compile error is the red state.
4. No commit — the tree is red by design and Task 4 makes it green.

### Task 4: Add the --at flag to analyze and diff

**Crate**: `rulery-cli`
**File(s)**: `crates/cli/src/args.rs`, `crates/cli/src/commands.rs`, `crates/cli/src/dispatch.rs`
**Run**: `cargo nextest run -p rulery-cli`

1. Failing test: the test from Task 3.
2. Confirm FAIL: same compile error on the missing `at` field.
3. Implement: add `#[arg(long)] at: Option<String>` to the `Analyze` and `Diff` variants, reusing
   the exact doc comment `Explain` already carries at `:100`. In `dispatch.rs`, resolve the string
   through the same `UtcInstant::parse_rfc3339` path the `Explain` command uses and pass it to the
   service. A parse failure must yield `HostError::Invocation` so it lands on the
   `InvalidInvocation` row of the exit matrix rather than panicking. Default to
   `analysis_instant()` so an omitted flag is byte-identical to today's output. Check
   `commands.rs` for an exhaustive match needing a new arm.
4. Confirm GREEN: `cargo nextest run -p rulery-cli` passes, including
   `cli_parses_v01_commands_and_defaults` and `exit_codes_match_the_specification_matrix`.
5. `git commit -m "feat(rulery-cli): add --at to analyze and diff"`

### Task 5: Assert the reported instant follows the flag

**Crate**: `rulery-cli`
**File(s)**: `crates/cli/tests/process_contract.rs`
**Run**: `cargo nextest run -p rulery-cli analyze_at_is_recorded_in_the_report`

1. Add `analyze_at_is_recorded_in_the_report`. Run `analyze <FIXTURE> --frozen --format json` with
   `--at 2026-09-16T16:00:00.000000000Z`, reusing the `INSTANT` constant at `:21`, and assert the
   witness instant in the envelope equals the supplied value. Use the module's `scratch`, `copy_tree`
   and `Invocation` helpers so the fixture is copied before anything writes.
2. Confirm FAIL: the emitted report records the epoch, so the assertion fails.
3. Implement: thread the parsed instant from the parsed command to the service call. If the instant
   does not surface in the analysis envelope payload, record that as a new task rather than
   weakening the assertion.
4. Confirm GREEN: `cargo nextest run -p rulery-cli` passes.
5. `git commit -m "fix(rulery-cli): honor the analysis instant in the emitted report"`

### Task 6: Allow a caller to inject the time-zone database

**Crate**: `rulery`
**File(s)**: `src/application.rs`
**Run**: `cargo nextest run -p rulery analyze_records_the_injected_database_identity`

1. Add `analyze_records_the_injected_database_identity`. Construct a `ProductionApplication` through
   a new `with_time_zones` taking a `Box<dyn TimeZoneDatabase>`, run `analyze`, and assert the
   report's `TimeZoneDatabaseIdentity` is the injected one rather than `jiff`. Reuse the
   `FixedTimeZoneDatabase` pattern from `crates/engine/src/policy_evaluator.rs:1409`.
2. Confirm FAIL: compile error, because no such constructor exists.
3. Implement: change the `time_zones` field at `:162` to `Arc<dyn TimeZoneDatabase>`, add
   `ProductionApplication::with_time_zones`, and have `Default` wrap `JiffTimeZoneDatabase::default()`.
   `Arc` rather than `Box` because `Default` derives `Clone` and `evaluate_at` at `:249` and
   `run_scenarios` at `:281` share the struct by reference. Update both to deref the trait object.
4. Confirm GREEN: `cargo nextest run -p rulery` passes and clippy is clean.
5. `git commit -m "feat(rulery): inject the analysis time-zone database"`

### Task 7: Run the full gate for gap1

**Crate**: `xtask`
**File(s)**: none
**Run**: `cargo xtask verify`

1. No failing test; this task is the gate check itself.
2. Confirm FAIL: not applicable.
3. Implement nothing.
4. Confirm GREEN: all seven gates pass and the test count grew by at least three.
5. No commit if already green. Do not create an empty commit.

### Task 8: Assert the clippy gate names every target

**Crate**: `xtask`
**File(s)**: `xtask/tests/verify.rs`
**Run**: `cargo nextest run -p xtask clippy_gate_lints_every_target`

1. Add `clippy_gate_lints_every_target`, calling a new public `xtask::gate_command(VerifyGate::Clippy)`
   and asserting the result contains `--all-targets`. Keep it separate from
   `verify_runs_exact_fail_fast_gate_order` at `:8`, which owns gate ordering and must not absorb a
   second concern.
2. Confirm FAIL: compile error, because `gate_command` does not exist.
3. Implement nothing; the compile error is the red state.
4. No commit — the tree is red by design and Task 9 makes it green.

### Task 9: Extract the gate commands and add the flag

**Crate**: `xtask`
**File(s)**: `xtask/src/verify.rs`, `xtask/src/lib.rs`
**Run**: `cargo nextest run -p xtask`

1. Failing test: the test from Task 8.
2. Confirm FAIL: compile error, as established.
3. Implement: add `pub fn gate_command(gate: VerifyGate) -> Option<&'static str>` returning the
   command for the four external gates and `None` for the three in-process gates, with
   `Clippy` returning `"cargo clippy --workspace --all-targets -- -D warnings"`. Rewrite
   `HostRunner::run` to invoke that string instead of the inlined `cmd!` calls, preserving the
   existing `map_err` into `XtaskError::Command`. Re-export from `xtask/src/lib.rs` and narrow the
   `xshell` `cmd` import if it becomes unused.
4. Confirm GREEN: `cargo nextest run -p xtask` passes and
   `cargo clippy --workspace --all-targets -- -D warnings` is clean, confirming the widened gate
   does not turn the workspace red.
5. `git commit -m "fix(xtask): lint every target in the verify clippy gate"`

### Task 10: Assert the diagnostics crate exposes the fields the CLI needs

**Crate**: `rulery-diagnostics`
**File(s)**: `crates/diagnostics/src/model.rs`, `crates/diagnostics/src/wire.rs`
**Run**: `cargo nextest run -p rulery-diagnostics diagnostics_expose_labels_confidence_and_report_contents`

1. Add `diagnostics_expose_labels_confidence_and_report_contents` to `crates/diagnostics/src/model.rs`.
   Build one diagnostic through `Diagnostic::builder`, attach a label, and assert `labels()` returns
   it and `properties().confidence()` returns the configured confidence. Also assert a
   `DiagnosticReportV1` round-trips and yields its contents through `diagnostics()`.
2. Confirm FAIL: compile error, because none of the three accessors exist.
3. Implement nothing; the compile error is the red state.
4. No commit — the tree is red by design and Task 11 makes it green.

### Task 11: Add the three accessors

**Crate**: `rulery-diagnostics`
**File(s)**: `crates/diagnostics/src/model.rs`, `crates/diagnostics/src/wire.rs`
**Run**: `cargo nextest run -p rulery-diagnostics`

1. Failing test: the test from Task 10.
2. Confirm FAIL: compile error, as established.
3. Implement: add `Diagnostic::labels(&self) -> &[DiagnosticLabel]`;
   `DiagnosticProperties::confidence(&self) -> FindingConfidence` beside `producer()` and
   `suppressibility()` at `:200-208`; and `DiagnosticReportV1::diagnostics(&self) -> &[Diagnostic]`
   in `wire.rs`. All three need `#[must_use]`; the two returning a `Copy` value may be `const fn`
   following surrounding style. One-line verb-first doc comment each. No field visibility change.
4. Confirm GREEN: `cargo nextest run -p rulery-diagnostics` passes and clippy is clean.
5. `git commit -m "feat(rulery-diagnostics): expose labels, confidence, and report diagnostics"`

### Task 12: Delete the three serialization round trips

**Crate**: `rulery-cli`
**File(s)**: `crates/cli/src/findings.rs`
**Run**: `cargo nextest run -p rulery-cli`

1. Failing test: none new. Run `machine_formats_emit_one_envelope_and_no_stderr` first and confirm
   it is green, so a later failure is attributable. This is a pure refactor behind an existing
   behavioural contract.
2. Confirm FAIL: not applicable; the baseline must be green before the refactor.
3. Implement: rewrite `confidence_of` at `:219` to
   `WarningConfidence::from(diagnostic.properties().confidence())` with no `Result`, and update its
   caller at `:67`. Rewrite `first_label_span` at `:269` to use `diagnostic.labels().first().map(DiagnosticLabel::span)`,
   keeping the best-effort absence behaviour. Rewrite `from_analysis` at `:94` to read
   `report.payload().diagnostics.diagnostics()` directly, dropping the round trip and the `HostError`
   it could produce, which may simplify the signature and its caller in `dispatch.rs`. Delete the
   stale doc comments at `:215-218` and `:266-268` and the acknowledgement in the module docs of
   `crates/cli/src/error.rs:7-12`. Remove any import that becomes unused.
4. Confirm GREEN: `cargo nextest run -p rulery-cli` passes with all 16 process tests still green, and
   clippy is clean with no new `allow`.
5. `git commit -m "refactor(rulery-cli): read diagnostic fields through accessors"`

### Task 13: Assert machine formats still receive one stdout artifact

**Crate**: `rulery-cli`
**File(s)**: `crates/cli/tests/process_contract.rs`
**Run**: `cargo nextest run -p rulery-cli machine_formats_emit_one_envelope_on_pre_artifact_failure`

1. Add `machine_formats_emit_one_envelope_on_pre_artifact_failure`. Run
   `rulery analyze <nonexistent-path> --format json` and assert stdout parses as a JSON value rather
   than being empty, while the exit status still matches the `IoFailure` row. Use the module's
   `Invocation` helpers and temp-scratch discipline.
2. Confirm FAIL: stdout is empty, so the JSON parse assertion fails.
3. Implement nothing; the empty stdout is the red state.
4. No commit — the tree is red by design and Task 14 makes it green.

### Task 14: Emit an empty diagnostic envelope in machine modes

**Crate**: `rulery-cli`
**File(s)**: `crates/cli/src/dispatch.rs`
**Run**: `cargo nextest run -p rulery-cli`

1. Failing test: the test from Task 13.
2. Confirm FAIL: empty stdout, as established.
3. Implement: change `pre_artifact` at `:167` to take the `Command` so it can read the format. In
   machine formats build `DiagnosticReport::new(Vec::new(), CLI_PRODUCER, LanguageVersion::V1)`,
   reusing the constant at `crates/cli/src/findings.rs:18`, and emit it through the existing
   `report::diagnostic_json` path already used at `:204`. Keep the human message on stderr. Human
   formats keep today's stderr-only behaviour. A zero-diagnostic envelope is honest — it asserts no
   diagnostics were constructible rather than inventing a registry code — and it satisfies the
   single-artifact cardinality the specification states. Update all three call sites in `failure` at `:157`.
4. Confirm GREEN: `cargo nextest run -p rulery-cli` passes. If
   `invocation_failures_report_on_stderr_with_an_empty_stdout` now fails, it is asserting the old
   contract; change it to assert the single-artifact rule for machine formats and note the contract
   change in the commit body.
5. `git commit -m "fix(rulery-cli): emit one machine artifact for pre-artifact failures"`

### Task 15: Scaffold the imported fixture package and assert the graph assembles

**Crate**: `rulery`
**File(s)**: `examples/import-graph/**`, `src/assembly.rs`
**Run**: `cargo nextest run -p rulery real_import_graph_assembles_from_disk`

1. Add `real_import_graph_assembles_from_disk` to the `#[cfg(test)] mod tests` in `src/assembly.rs`.
   Point `PackageAssembler` at `examples/import-graph` with the real `FilesystemPackageStore` and
   `YamlSourceParser`, the same construction `ProductionApplication::compile_package` uses at
   `src/application.rs:197`, and assert the assembled closure contains both packages.
2. Confirm FAIL: the fixture directory does not exist, so the store reports a missing root.
3. Implement: create `examples/import-graph/` with the exact authored layout the store requires —
   `rulery.yaml`, `vocabulary.yaml` and `actions.yaml` at the root plus non-empty `rules/` and
   `scenarios/` directories, per `REQUIRED_ROOT_FILES` and `REQUIRED_DIRS` at
   `crates/store/src/filesystem.rs:16-17`. Add a second package under `imports/shared/` with the
   same five-path layout, and give the root an `imports:` entry naming it by package id and relative
   path. Keep the imported decision minimal and free of temporal conditions so it does not interact
   with gap1.
4. Confirm GREEN: `cargo nextest run -p rulery` passes.
5. `git commit -m "test(rulery): add a real multi-package import fixture"`

### Task 16: Freeze the fixture lock and assert frozen agreement

**Crate**: `rulery`
**File(s)**: `examples/import-graph/rulery.lock`, `src/assembly.rs`
**Run**: `cargo nextest run -p rulery real_import_graph_freezes_and_reassembles_identically`

1. Add `real_import_graph_freezes_and_reassembles_identically`. Assemble in `LockMode::Update`,
   assert a proposed lock is returned, assemble again in `LockMode::Frozen` against the written lock,
   and assert the second closure hash equals the first.
2. Confirm FAIL: no lock exists, so `Frozen` fails.
3. Implement: produce `examples/import-graph/rulery.lock` with `rulery lock examples/import-graph`
   from a fresh build, then cross-check the generated `content_hash` against
   `IntegrityCalculator::bundle_frame_bytes`, which is public at `crates/store/src/integrity.rs:23`
   precisely so this can be verified. Record the hash as a literal in the test rather than
   recomputing it at runtime, so an unintended fixture edit fails instead of silently re-locking.
4. Confirm GREEN: `cargo nextest run -p rulery` and `cargo nextest run -p rulery-store` both pass.
5. `git commit -m "test(rulery): freeze the import-graph fixture lock"`

### Task 17: Assert the CLI resolves the closure end to end

**Crate**: `rulery-cli`
**File(s)**: `crates/cli/tests/process_contract.rs`
**Run**: `cargo nextest run -p rulery-cli check_frozen_accepts_the_import_graph_fixture`

1. Add `check_frozen_accepts_the_import_graph_fixture`. Run `rulery check examples/import-graph
--frozen` and assert exit zero with a valid machine artifact, then run `rulery test
examples/import-graph --frozen`. Define the fixture path as a second `const` beside `FIXTURE` at
   `:18` using the existing `env!("CARGO_MANIFEST_DIR")` pattern, and keep `Command::env_clear()` so
   no ambient `CI` value leaks in, as the module already does.
2. Confirm FAIL: before Tasks 15 and 16 the fixture does not exist, so the store reports a missing
   root and the exit status is not zero.
3. Implement nothing. This task exists to prove the real store, parser and assembler work together
   on disk, which no test previously did.
4. Confirm GREEN: `cargo nextest run -p rulery-cli` passes.
5. `git commit -m "test(rulery-cli): cover the import graph through the process boundary"`

### Task 18: Run the full gate for gap6

**Crate**: `xtask`
**File(s)**: none
**Run**: `cargo xtask verify`

1. No failing test; this task is the gate check itself.
2. Confirm FAIL: not applicable.
3. Implement nothing.
4. Confirm GREEN: all seven gates pass. Confirm the new fixture did not disturb
   `tests/workspace_scaffold.rs::workspace_contains_approved_crates`, which asserts a fixed
   directory set.
5. No commit if already green.

### Task 19: Derive an after-package in scratch and assert diff reports an outcome change

**Crate**: `rulery-cli`
**File(s)**: `crates/cli/tests/process_contract.rs`
**Run**: `cargo nextest run -p rulery-cli diff_reports_an_outcome_change_between_two_packages`

1. Add `diff_reports_an_outcome_change_between_two_packages`. Copy the tool-library fixture into a
   per-test scratch directory with the existing `copy_tree` helper, mutate one rule so a decision
   that previously denied now approves, run `rulery lock` on the mutated copy in `Update` mode to
   regenerate its lock, then run `rulery diff <original> <mutated> --frozen --format json` and assert
   the envelope contains a semantic diff entry classified as an outcome change. Use `DECISION` at
   `:24` for the decision filter.
2. Confirm FAIL: the assertion fails, since no existing test drives `diff` through a process.
3. Implement nothing; the failing assertion is the red state.
4. No commit — the tree is red by design and Task 21 makes it green.

### Task 20: Assert a structural-only change is classified as structural

**Crate**: `rulery-cli`
**File(s)**: `crates/cli/tests/process_contract.rs`
**Run**: `cargo nextest run -p rulery-cli diff_reports_a_structural_change_without_an_outcome_change`

1. Add `diff_reports_a_structural_change_without_an_outcome_change`, mutating only a rule title or a
   precedence setting so the evaluated outcome is unchanged. Assert the report carries the
   structural classification and no outcome change.
2. Confirm FAIL: the assertion fails, same red state as Task 19.
3. Implement nothing.
4. No commit — the tree is red by design and Task 21 makes it green.

### Task 21: Close the diff gap if either test exposed a defect

**Crate**: `rulery-cli`
**File(s)**: determined by Tasks 19 and 20
**Run**: `cargo nextest run -p rulery-cli diff`

1. Failing test: whichever of Task 19 or Task 20 failed for a real reason rather than for absence of
   coverage.
2. Confirm FAIL: the specific assertion that failed.
3. Implement: fix only what the failing assertion names. If both tests pass once written, this task
   completes with no code change — the value delivered here is the coverage, not a behaviour change.
4. Confirm GREEN: `cargo nextest run -p rulery-cli` passes.
5. `git commit -m "fix(rulery-cli): <describe the defect the diff tests exposed>"`, or no commit if
   no defect surfaced.

### Task 22: Run the full gate for gap7

**Crate**: `xtask`
**File(s)**: none
**Run**: `cargo xtask verify`

1. No failing test; this task is the gate check itself.
2. Confirm FAIL: not applicable.
3. Implement nothing.
4. Confirm GREEN: all seven gates pass and `crates/cli/tests/process_contract.rs` holds at least 19
   tests.
5. No commit if already green.

### Task 23: Assert the test gate builds with every feature

**Crate**: `xtask`
**File(s)**: `xtask/tests/verify.rs`
**Run**: `cargo nextest run -p xtask nextest_gate_builds_every_feature`

1. Add `nextest_gate_builds_every_feature`, asserting `gate_command(VerifyGate::Nextest)` contains
   `--all-features`. This reuses the accessor built in Task 9.
2. Confirm FAIL: the assertion fails, because the gate is currently `cargo nextest run --workspace`.
3. Implement nothing; the assertion failure is the red state.
4. No commit — the tree is red by design and Task 24 makes it green.

### Task 24: Add --all-features to the test gate

**Crate**: `xtask`
**File(s)**: `xtask/src/verify.rs`
**Run**: `cargo nextest run -p xtask`

1. Failing test: the test from Task 23.
2. Confirm FAIL: the assertion fails, as established.
3. Implement: change the `Nextest` arm of `gate_command` to
   `cargo nextest run --workspace --all-features`. Run the command once before committing to
   confirm the root `macros` feature at `Cargo.toml:45` actually compiles.
4. Confirm GREEN: `cargo nextest run -p xtask` passes and
   `cargo nextest run --workspace --all-features` compiles.
5. `git commit -m "fix(xtask): build the test suite with every feature"`

### Task 25: Assert the derive produces a working conversion

**Crate**: `rulery`
**File(s)**: `tests/workspace_scaffold.rs`
**Run**: `cargo nextest run -p rulery --features macros rule_facts_derive_converts_a_record_into_case_facts`

1. Add a test guarded by `#[cfg(feature = "macros")]` named
   `rule_facts_derive_converts_a_record_into_case_facts`. Declare a `#[derive(RuleFacts)]` struct
   with `#[rulery(root = "member")]` and explicit `#[rulery(path = "...")]` fields, convert it with
   `TryFrom`, and assert the resulting `CaseFacts` carries the authored nested paths. Follow the
   supported field types documented at `crates/macros/src/lib.rs:18-20`.
2. Confirm FAIL: the test does not exist, and no test in the workspace currently compiles generated
   `RuleFacts` output at all — `crates/macros/src/lib.rs:33` asserts only that `expand()` returns
   `Ok` or `Err` on a token stream.
3. Implement: write the test. Because the gate now passes `--all-features`, the derive is compiled
   and run as part of the normal suite. If the derive has a real defect this task exposes it, and the
   fix belongs in `crates/macros` as a separate commit.
4. Confirm GREEN: `cargo nextest run --workspace --all-features` passes.
5. `git commit -m "test(rulery): exercise the RuleFacts derive end to end"`

### Task 26: Add the renamed-dependency hygiene fixture crate

**Crate**: `rulery-macros`
**File(s)**: `examples/macro-hygiene/Cargo.toml`, `examples/macro-hygiene/src/lib.rs`
**Run**: `cargo check --manifest-path examples/macro-hygiene/Cargo.toml`

1. No in-workspace test can cover this, because the property is that a downstream crate may rename
   its `rulery` dependency. The red state is absence: `crates/macros/src/lib.rs:22-23` claims
   `proc-macro-crate` provides that hygiene and nothing in the workspace proves it.
2. Confirm FAIL: `cargo check --manifest-path examples/macro-hygiene/Cargo.toml` reports no such
   manifest.
3. Implement: create `examples/macro-hygiene/` with a `Cargo.toml` depending on `rulery` under a
   different local name, for example `rlry = { path = "../..", features = ["macros"] }`, and a
   `src/lib.rs` using `#[derive(rlry::macros::RuleFacts)]`. The manifest must carry an empty
   `[workspace]` table so cargo excludes it from the parent workspace — otherwise the `crates/*`
   glob is unaffected but the parent workspace may still claim it, and `xtask::architecture` will
   reject an unmodeled member. Add the path to the directory list asserted in
   `tests/workspace_scaffold.rs:69-80` if that list should track it.
4. Confirm GREEN: the check succeeds and `cargo xtask verify` still passes, proving the exclusion
   worked.
5. `git commit -m "test(rulery-macros): prove renamed-dependency hygiene in a downstream crate"`

### Task 27: Assert a record literal typechecks in a condition

**Crate**: `rulery-compiler`
**File(s)**: `crates/compiler/src/typecheck.rs`
**Run**: `cargo nextest run -p rulery-compiler record_literals_typecheck_against_record_typed_operands`

1. Add `record_literals_typecheck_against_record_typed_operands` to the `#[cfg(test)] mod tests` in
   `crates/compiler/src/typecheck.rs`. Build two `OperandSpec::Literal` values typed as the proposed
   record variant, assert `typecheck_predicate(Operator::Equals, ..)` succeeds, and assert an
   ordering operator against the same type is rejected. The second half pins that adding the variant
   does not silently widen the ordering arm, which currently admits only `Integer` and `Decimal` at `:120`.
2. Confirm FAIL: compile error, because `CheckedType::Record` does not exist.
3. Implement nothing; the compile error is the red state.
4. No commit — the tree is red by design and Task 28 makes it green.

### Task 28: Add the variant and the literal decode

**Crate**: `rulery-compiler`
**File(s)**: `crates/compiler/src/typecheck.rs`, `crates/compiler/src/lib.rs`
**Run**: `cargo nextest run -p rulery-compiler`

1. Failing test: the test from Task 27.
2. Confirm FAIL: compile error, as established.
3. Implement: add `Record(StableId)` to `CheckedType` with a doc comment. Equality already flows
   through the `same_type_ok` comparison at `:93-95` and `PartialEq` is already derived, so no
   operator arm changes. Then teach the `literal` decoder at `crates/compiler/src/lib.rs:872-912` to
   handle a `SourceValue::Mapping` against `CheckedType::Record`, producing
   `Value::Record(BTreeMap<StableId, Value>)` which already exists at
   `crates/contracts/src/value.rs:65`. Resolve each nested entry against its declared field type,
   mirroring the scenario bridge at `src/application.rs:480` rather than writing a second
   implementation. Delete the now-stale comment at `:869-871`. Honor the declared `closed` policy on
   `TypeDeclaration::Record` at `crates/vocabulary/src/model.rs:70-72` when checking extra fields.
4. Confirm GREEN: `cargo nextest run -p rulery-compiler` passes and the existing
   `typechecker_enforces_operator_matrix` is unaffected.
5. `git commit -m "feat(rulery-compiler): accept record literals in rule conditions"`

### Task 29: Assert a real package compiles a record-valued condition

**Crate**: `rulery`
**File(s)**: `examples/tool-library/rules/checkout.yaml`
**Run**: `cargo nextest run -p rulery tool_library_explain_is_reproducible_end_to_end`

1. No new test. Confirm
   `tests/conformance/tool_library_explain.rs::tool_library_explain_is_reproducible_end_to_end` is
   green first, so a later failure is attributable to the fixture edit rather than a stale baseline.
2. Confirm FAIL: not applicable; the baseline must be green before the fixture changes.
3. Implement: add a rule to the fixture whose condition compares a record-typed fact against a record
   literal, choosing one that is false for the checked-in case so the canonical explanation text does
   not change. Per `AGENTS.md` this invalidates the frozen hash, so regenerate with
   `rulery lock examples/tool-library` and cross-check against
   `IntegrityCalculator::bundle_frame_bytes`. Update the canonical-text assertion in
   `crates/emit/src/human.rs` only if the trace actually changed.
4. Confirm GREEN: `cargo xtask verify` passes, including the four frozen BLAKE3 vectors, and the
   canonical human explanation is unchanged.
5. `git commit -m "test(rulery): cover a record-valued condition in the tool-library fixture"`

### Task 30: Assert the license files exist and name the declared licence

**Crate**: `rulery`
**File(s)**: `tests/workspace_scaffold.rs`
**Run**: `cargo nextest run -p rulery license_files_back_the_declared_dual_licence`

1. Add `license_files_back_the_declared_dual_licence` to `tests/workspace_scaffold.rs`, which
   already owns workspace-wide file-presence assertions. Assert `LICENSE-MIT` and `LICENSE-APACHE`
   both exist at the repository root, that `LICENSE-MIT` contains the standard MIT permission
   notice, and that `LICENSE-APACHE` contains the Apache License version marker.
2. Confirm FAIL: neither file exists, so the assertion fails.
3. Implement nothing; the missing files are the red state.
4. No commit — the tree is red by design and Task 31 makes it green.

### Task 31: Write both license files

**Crate**: `rulery`
**File(s)**: `LICENSE-MIT`, `LICENSE-APACHE`, `README.md`
**Run**: `cargo nextest run -p rulery license_files_back_the_declared_dual_licence`

1. Failing test: the test from Task 30.
2. Confirm FAIL: missing files, as established.
3. Implement: write `LICENSE-MIT` with the standard MIT text and `LICENSE-APACHE` with the full
   Apache License 2.0 text, written directly rather than fetched because fetching truncates. Use
   Joseph O'Brien as copyright holder, matching the identity this workspace publishes under. Replace
   the "This repository does not currently include the corresponding license text files" line in
   `README.md`, which becomes false.
4. Confirm GREEN: `cargo nextest run -p rulery` passes and
   `cargo rail config validate --strict` still succeeds.
5. `git commit -m "docs: add the MIT and Apache-2.0 license texts"`

### Task 32: Assert the gate rejects a specification missing a required bullet

**Crate**: `xtask`
**File(s)**: `xtask/src/conformance.rs`
**Run**: `cargo nextest run -p xtask conformance_gate_rejects_a_missing_required_bullet`

1. Add `conformance_gate_rejects_a_missing_required_bullet` to the `#[cfg(test)] mod tests` in
   `xtask/src/conformance.rs`. Call a new `check_required_gates` with a specification whose
   "Required conformance gates" section is missing one bullet and assert the error names it. Add the
   matching accept assertion against the real `docs/specification.md`, following the accept-and-reject
   pairing already used by `requirements_accept_a_fully_satisfied_and_verified_specification`.
2. Confirm FAIL: compile error, because `check_required_gates` does not exist.
3. Implement nothing; the compile error is the red state.
4. No commit — the tree is red by design and Task 33 makes it green.

### Task 33: Implement the bullet check

**Crate**: `xtask`
**File(s)**: `xtask/src/conformance.rs`
**Run**: `cargo nextest run -p xtask`

1. Failing test: the test from Task 32.
2. Confirm FAIL: compile error, as established.
3. Implement: add a `REQUIRED_GATE_BULLETS` constant listing the fifteen bullets verbatim from
   `docs/specification.md:3330-3346` and a `check_required_gates(specification: &str)` that fails
   when the section is absent or any bullet is missing. Reuse the heading-split idiom from
   `requirement_rows` at `:291` and `check_registry` at `:363` so parsing style matches. Keep the
   normalizer tolerant of bullet text drifting while still failing on a removed bullet, which is
   what the accept-and-reject pair pins.
4. Confirm GREEN: `cargo nextest run -p xtask` passes.
5. `git commit -m "feat(xtask): check the required conformance gates in the specification"`

### Task 34: Wire the check into the conformance gate

**Crate**: `xtask`
**File(s)**: `xtask/src/conformance.rs`
**Run**: `cargo nextest run -p xtask conformance_accepts_normative_specification`

1. Failing test: the existing `conformance_accepts_normative_specification`, which exercises the
   real specification.
2. Confirm FAIL: once Task 33's check is wired in, this goes red if any bullet is reported missing.
   That is the intended red state and is the gate doing its job.
3. Implement: call `check_required_gates` from `check` and push failures into the `failures` vector
   under the check name `gates`, matching existing labels at `:43-47`.
4. Confirm GREEN: green only when all fifteen bullets are present. If it stays red, the
   specification genuinely lost a bullet — restore it rather than relaxing the list.
5. `git commit -m "feat(xtask): enforce the required conformance gates"`

### Task 35: Run the full gate for the plan

**Crate**: `xtask`
**File(s)**: none
**Run**: `cargo xtask verify`

1. No failing test; this task is the terminal gate check for the whole plan.
2. Confirm FAIL: not applicable.
3. Implement nothing. Three bullets remain open by design and are recorded as follow-up tasks when
   this plan is ingested, not patched here: the doctest bullet, since the workspace contains zero
   doctests and no gate runs `cargo test --doc`; the renderer-snapshot bullet at
   `docs/specification.md:3343`, which needs a decision because `AGENTS.md` bans snapshot testing
   while the specification requires snapshots; and the two `examples/` fixtures required at
   `docs/specification.md:3344-3345`, which today exist only as temp-dir scratch in
   `crates/cli/tests/process_contract.rs`. Do not quietly satisfy them by adding `insta` or
   `proptest` — both are banned.
4. Confirm GREEN: all seven gates pass with the new bullet check enforcing.
5. No commit if already green.

## Verification ledger

Every task above carries an explicit red-green cycle. Workspace-level evidence that the plan landed:
`cargo xtask verify` green at the end of each gap section, and all seventeen tests below present and
passing.

| Test                                                         | Crate                | Gap   |
| ------------------------------------------------------------ | -------------------- | ----- |
| `analyze_honors_an_explicit_instant`                         | `rulery`             | gap1  |
| `cli_parses_the_analysis_instant_flag`                       | `rulery-cli`         | gap1  |
| `analyze_at_is_recorded_in_the_report`                       | `rulery-cli`         | gap1  |
| `analyze_records_the_injected_database_identity`             | `rulery`             | gap1  |
| `clippy_gate_lints_every_target`                             | `xtask`              | gap2  |
| `diagnostics_expose_labels_confidence_and_report_contents`   | `rulery-diagnostics` | gap4  |
| `machine_formats_emit_one_envelope_on_pre_artifact_failure`  | `rulery-cli`         | gap5  |
| `real_import_graph_assembles_from_disk`                      | `rulery`             | gap6  |
| `real_import_graph_freezes_and_reassembles_identically`      | `rulery`             | gap6  |
| `check_frozen_accepts_the_import_graph_fixture`              | `rulery-cli`         | gap6  |
| `diff_reports_an_outcome_change_between_two_packages`        | `rulery-cli`         | gap7  |
| `diff_reports_a_structural_change_without_an_outcome_change` | `rulery-cli`         | gap7  |
| `nextest_gate_builds_every_feature`                          | `xtask`              | gap8  |
| `rule_facts_derive_converts_a_record_into_case_facts`        | `rulery`             | gap8  |
| `record_literals_typecheck_against_record_typed_operands`    | `rulery-compiler`    | gap9  |
| `license_files_back_the_declared_dual_licence`               | `rulery`             | gap10 |
| `conformance_gate_rejects_a_missing_required_bullet`         | `xtask`              | gap3  |

## Notes for the executing agent

- **Branch first.** `git branch --show-current` is `main` on this checkout. Every commit here is a
  mutation, so create a feature branch before Task 1 and never commit on `main`.
- **Never use `--no-verify`.** The pre-commit hook runs the gates; if it fails, fix the gate.
- **Editing a fixture changes its hash.** Any edit under `examples/` invalidates the `content_hash`
  in the sibling `rulery.lock`. Regenerate with `rulery lock <path>` and cross-check against
  `IntegrityCalculator::bundle_frame_bytes` at `crates/store/src/integrity.rs:23`.
- **Do not add a `RUL` code to report any of this.** The registry is fixed at exactly forty codes
  and `xtask::conformance::check_registry` rejects a forty-first. A finding a new test surfaces that
  is not expressible in an existing code belongs in a design note.
- **`gap3` runs last by dependency, not preference.** It is numbered third but executes after
  `gap10`, because it enforces fifteen specification bullets of which four are unmet; two are closed
  by `gap2` and `gap8` and two are out of scope. Running it in numeric position would turn the
  conformance gate red and leave it red.
