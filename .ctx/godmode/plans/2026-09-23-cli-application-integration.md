# Plan: CLI Application Integration

## Goal

Make every declared Rulery CLI command executable through a reusable application boundary while keeping policy semantics out of the CLI transport.

## Context Map

### Files to Modify

| File                                    | Purpose                        | Change                                                                         |
| --------------------------------------- | ------------------------------ | ------------------------------------------------------------------------------ |
| `crates/syntax/src/ast.rs`              | Complete authored source model | Preserve vocabulary, actions, rules, cases, and scenarios                      |
| `crates/syntax/src/yaml/mod.rs`         | YAML bundle parser             | Parse every declared package document                                          |
| `crates/compiler/src/lib.rs`            | Compiler entry point           | Compile assembled syntax into executable IR                                    |
| `crates/compiler/src/lower.rs`          | Typed lowering                 | Retain rule effects, reasons, priority, and actions                            |
| `crates/ir/src/package.rs`              | Executable package model       | Add validated executable rule fields and accessors                             |
| `crates/ir/src/wire.rs`                 | Compiled package wire contract | Round-trip executable rule data                                                |
| `crates/engine/src/lib.rs`              | Evaluation service export      | Export package-level evaluator                                                 |
| `crates/engine/src/evaluator.rs`        | Decision evaluation            | Evaluate compiled decisions at an explicit instant                             |
| `crates/analysis/src/wire.rs`           | Static analysis service        | Implement production package analysis and diff                                 |
| `crates/scenarios/src/compile.rs`       | Scenario compilation           | Compile complete authored scenario sources                                     |
| `crates/scenarios/src/run.rs`           | Scenario execution             | Delegate to package evaluator with fixed time                                  |
| `crates/diagnostics/src/wire.rs`        | Diagnostic inspection          | Expose deterministic diagnostic slices                                         |
| `crates/emit/src/markdown.rs`           | Markdown output                | Render compiled package documentation                                          |
| `crates/emit/src/decision_table.rs`     | Decision table output          | Project compiled decisions without semantic loss                               |
| `src/application.rs`                    | Shared application service     | Compose production store, parser, compiler, evaluator, analysis, and scenarios |
| `src/lib.rs`                            | Root facade exports            | Export application API                                                         |
| `src/facade.rs`                         | Existing facade                | Preserve compatibility and delegate where practical                            |
| `Cargo.toml`                            | Root dependencies              | Promote YAML decoding needed at runtime                                        |
| `crates/cli/src/application.rs`         | CLI application adapter        | Map all commands to application operations                                     |
| `crates/cli/src/main.rs`                | Process entry point            | Dispatch, write streams once, and return stable exit code                      |
| `crates/cli/src/lib.rs`                 | CLI exports                    | Export testable dispatch boundary                                              |
| `crates/cli/src/commands.rs`            | Write command policy           | Align init/fmt/check/lock behavior with specification                          |
| `crates/cli/src/artifact.rs`            | Artifact command policy        | Delegate typed operations and rendering                                        |
| `crates/cli/src/output.rs`              | Stream boundary                | Write stdout and stderr exactly once                                           |
| `crates/cli/tests/cli.rs`               | Process conformance            | Test every command and output mode                                             |
| `tests/conformance/tool_library_cli.rs` | End-to-end fixture             | Replace predetermined helper with executable CLI path                          |
| `README.md`                             | User documentation             | Document executable commands and examples                                      |

### Dependencies

`syntax -> vocabulary -> ir -> compiler/engine -> analysis/scenarios -> emit -> rulery -> rulery-cli`

The root application must not depend on `rulery-cli`; CLI-owned adapters implement transport traits and call downward into `rulery`.

### Existing Coverage

- `crates/cli/src/args.rs`: command parsing and defaults.
- `crates/cli/src/commands.rs`: write policy using fake ports.
- `crates/cli/src/artifact.rs`: output and exit policy using fake ports.
- `src/assembly.rs`: import traversal, source remapping, locks, and limits.
- `crates/store/src/filesystem.rs`: filesystem containment and atomic writes.
- `tests/conformance/wire.rs`: strict versioned envelopes.
- `tests/conformance/tool_library_cli.rs`: deterministic fixture, currently not the binary.

### Risks

- Compiled package wire data changes before the first public release.
- CLI output is an external contract and requires subprocess tests before dispatch changes.
- Cross-crate implementation order must preserve the architecture allowlist.
- `FacadePorts` is stateful by shape; retain it during migration rather than forcing the production adapter through hidden mutable state.

## Architecture

- Crates affected: syntax, compiler, IR, engine, analysis, scenarios, diagnostics, emit, root `rulery`, and CLI.
- New traits/types: `PackageCompiler`, `PolicyEvaluator`, `ApplicationService`, `ProductionApplication`, `ApplicationError`, `CommandApplication`, and `CommandExecution` in the locations fixed by the approved design.
- Data flow: authored YAML bundle -> validated source AST -> executable compiled package -> evaluation/analysis/scenarios -> typed artifact -> CLI renderer and streams.
- Design authority: `docs/designs/2026-09-23-cli-application-integration-design.md`.

## Tech Stack

- Rust 2024, MSRV 1.85.
- Existing serde, serde_json, serde_yaml, thiserror, clap, and workspace crates.
- No new production dependency is planned.

## Tasks

### Task 1: Preserve complete authored YAML bundle semantics

**Crate**: `rulery-syntax`
**File(s)**: `crates/syntax/src/ast.rs`, `crates/syntax/src/yaml/mod.rs`
**Run**: `cargo nextest run -p rulery-syntax`

1. Add failing parser tests using a package containing vocabulary, actions, rules, cases, and scenarios in separate declared files.
2. Assert `ParsedPackage` retains each authored construct with deterministic source spans and ordering.
3. Extend the source AST and YAML conversion so `YamlSourceParser::parse_bundle` parses and merges every declared document instead of only indexing bytes in the source map.
4. Verify malformed and duplicate declarations produce deterministic parse diagnostics.
5. Run `cargo fmt --all`, `cargo clippy -p rulery-syntax --all-targets -- -D warnings`, and the task test command.
6. Run `git branch --show-current`; stop if it is `main`.
7. Commit with `feat(syntax): parse complete authored package bundles`.

### Task 2: Retain executable rule semantics in IR

**Crate**: `rulery-ir`
**File(s)**: `crates/ir/src/package.rs`, `crates/ir/src/wire.rs`, `crates/ir/src/lib.rs`, `tests/conformance/wire.rs`
**Run**: `cargo nextest run -p rulery-ir`

1. Add failing construction and wire-round-trip tests for rule effect, reasons, priority, actions, specificity, and source span.
2. Introduce the validated `CompiledEffect` representation required by the v0.1 specification.
3. Extend `CompiledRule` constructors and accessors, including `specificity()` and `effect()` from the approved design.
4. Extend `CompiledPackageV1` serialization without adding an unversioned alternate format.
5. Update all existing construction sites to provide complete executable rule data.
6. Run `cargo fmt --all`, `cargo clippy -p rulery-ir --all-targets -- -D warnings`, and the task test command.
7. Run `git branch --show-current`; stop if it is `main`.
8. Commit with `feat(ir): preserve executable rule semantics`.

### Task 3: Implement the end-to-end package compiler

**Crate**: `rulery-compiler`
**File(s)**: `crates/compiler/src/lib.rs`, `crates/compiler/src/lower.rs`, `crates/compiler/src/resolve.rs`, `crates/compiler/src/typecheck.rs`, `crates/compiler/src/normalize.rs`
**Run**: `cargo nextest run -p rulery-compiler`

1. Add a failing test that compiles the complete tool-library parsed package and asserts non-empty typed decisions, rules, effects, actions, vocabulary, integrity data, and source map.
2. Add the `PackageCompiler` trait and `PackageCompilerInput<'_>`/`CompilationOutput` types defined by the approved design.
3. Connect resolution, validation, type checking, normalization, specificity calculation, and lowering in deterministic order.
4. Replace the empty-package behavior in `PolicyCompiler` with the complete pipeline.
5. Return structured diagnostics for all authored errors and never emit a partial package on failure.
6. Run `cargo fmt --all`, `cargo clippy -p rulery-compiler --all-targets -- -D warnings`, and the task test command.
7. Run `git branch --show-current`; stop if it is `main`.
8. Commit with `feat(compiler): compile authored packages into executable IR`.

### Task 4: Implement package-level decision evaluation

**Crate**: `rulery-engine`
**File(s)**: `crates/engine/src/evaluator.rs`, `crates/engine/src/lib.rs`, `crates/engine/src/trace.rs`
**Run**: `cargo nextest run -p rulery-engine`

1. Add failing tests for complete package evaluation across approve, deny, escalate, request-information, runtime conflict, unknown, invalid, precedence, and explicit-time cases.
2. Add the `PolicyEvaluator` trait and typed `EvaluationError` boundary from the approved design.
3. Implement deterministic decision lookup, predicate evaluation, candidate creation, relevance, precedence, outcome selection, actions, reasons, and complete trace generation.
4. Require `UtcInstant` as input; do not read the system clock inside evaluation.
5. Run `cargo fmt --all`, `cargo clippy -p rulery-engine --all-targets -- -D warnings`, and the task test command.
6. Run `git branch --show-current`; stop if it is `main`.
7. Commit with `feat(engine): evaluate compiled package decisions`.

### Task 5: Complete analysis and semantic diff services

**Crate**: `rulery-analysis`
**File(s)**: `crates/analysis/src/wire.rs`, `crates/analysis/src/reachability.rs`, `crates/analysis/src/interaction.rs`, `crates/analysis/src/coverage.rs`, `crates/analysis/src/diff.rs`, `crates/analysis/src/budget.rs`
**Run**: `cargo nextest run -p rulery-analysis`

1. Add failing service-level tests for analysis aggregation, decision filtering, budget exhaustion, confidence, and semantic diff classifications.
2. Implement `PolicyAnalyzer` over compiled packages by composing existing reachability, interaction, coverage, witness, budget, and diff modules.
3. Ensure incomplete analysis is represented explicitly and preserves partial typed findings.
4. Expose only inspection APIs needed by renderers and CLI warning policy.
5. Run `cargo fmt --all`, `cargo clippy -p rulery-analysis --all-targets -- -D warnings`, and the task test command.
6. Run `git branch --show-current`; stop if it is `main`.
7. Commit with `feat(analysis): compose package analysis and semantic diff`.

### Task 6: Compile and run authored scenarios through the evaluator

**Crate**: `rulery-scenarios`
**File(s)**: `crates/scenarios/src/compile.rs`, `crates/scenarios/src/run.rs`, `crates/scenarios/src/lib.rs`
**Run**: `cargo nextest run -p rulery-scenarios`

1. Add failing tests proving authored scenarios retain fixed time, facts, decision references, expectations, and filters.
2. Align syntax scenario data with `ScenarioSource` without lossy conversion.
3. Compile scenarios against the complete package and execute them through `PolicyEvaluator`.
4. Preserve exact mismatch reporting and deterministic result ordering.
5. Run `cargo fmt --all`, `cargo clippy -p rulery-scenarios --all-targets -- -D warnings`, and the task test command.
6. Run `git branch --show-current`; stop if it is `main`.
7. Commit with `feat(scenarios): execute authored scenarios through policy evaluation`.

### Task 7: Expose typed diagnostics and complete package renderers

**Crate**: `rulery-emit`
**File(s)**: `crates/diagnostics/src/wire.rs`, `crates/emit/src/markdown.rs`, `crates/emit/src/decision_table.rs`, `crates/emit/src/json.rs`, `crates/emit/src/sarif.rs`
**Run**: `cargo nextest run -p rulery-diagnostics -p rulery-emit`

1. Add failing tests for diagnostic inspection, Markdown package rendering, decision-table projection, projection loss, JSON envelopes, and SARIF diagnostics.
2. Add `DiagnosticReport::diagnostics()` while preserving deterministic ordering and envelope shape.
3. Implement compiled-package-to-Markdown and compiled-package-to-decision-table conversion using typed IR only.
4. Keep projection loss explicit; do not silently omit unsupported semantics.
5. Run `cargo fmt --all`, `cargo clippy -p rulery-diagnostics -p rulery-emit --all-targets -- -D warnings`, and the task test command.
6. Run `git branch --show-current`; stop if it is `main`.
7. Commit with `feat(emit): render complete typed policy artifacts`.

### Task 8: Compose the shared production application

**Crate**: `rulery`
**File(s)**: `src/application.rs`, `src/lib.rs`, `src/facade.rs`, `Cargo.toml`
**Run**: `cargo nextest run -p rulery`

1. Add failing application tests for compile, evaluate-at, analyze, diff, scenario execution, lock proposal, frozen lock enforcement, and lock writing against temporary packages.
2. Add `ApplicationService`, `ProductionApplication`, and `ApplicationError` exactly as approved.
3. Compose `FilesystemPackageStore`, `YamlSourceParser`, package assembly, `PackageCompiler`, `PolicyEvaluator`, analysis, and scenario services without storing staged mutable workflow state.
4. Preserve `RuleryFacade` and `FacadePorts` compatibility; deprecate only if documentation clearly directs new callers to `ApplicationService`.
5. Promote `serde_yaml` from dev-only use only if facts decoding requires it at this boundary.
6. Run `cargo fmt --all`, `cargo clippy -p rulery --all-targets -- -D warnings`, and the task test command.
7. Run `git branch --show-current`; stop if it is `main`.
8. Commit with `feat(rulery): compose production application services`.

### Task 9: Implement unified CLI command dispatch

**Crate**: `rulery-cli`
**File(s)**: `crates/cli/src/application.rs`, `crates/cli/src/lib.rs`, `crates/cli/src/commands.rs`, `crates/cli/src/artifact.rs`, `crates/cli/src/output.rs`, `crates/cli/src/exit.rs`, `crates/cli/src/main.rs`, `crates/cli/Cargo.toml`
**Run**: `cargo nextest run -p rulery-cli`

1. Add failing dispatch tests for `init`, `fmt`, `check`, `analyze`, `test`, `explain`, `diff`, `render`, and `lock` using a fake `CommandApplication`.
2. Add `CommandApplication`, `CommandExecution`, `run_command`, and `write_command_output` exactly as approved.
3. Implement the CLI-owned application adapter that delegates semantic operations to `ProductionApplication` and owns only init, formatting, facts decoding, rendering selection, and stream policy.
4. Remove double stdout ownership and write each command output exactly once.
5. Align all status codes and stdout/stderr behavior with `docs/specification.md`, including non-empty init, `fmt --check`, lock path output, machine-mode stderr, `--frozen`, and `--deny-warnings`.
6. Parse CLI timestamps as RFC3339 and pass an explicit `UtcInstant` to evaluation.
7. Run `cargo fmt --all`, `cargo clippy -p rulery-cli --all-targets -- -D warnings`, and the task test command.
8. Run `git branch --show-current`; stop if it is `main`.
9. Commit with `feat(cli): dispatch all commands through application services`.

### Task 10: Lock process-level CLI contracts

**Crate**: `rulery-cli`
**File(s)**: `crates/cli/tests/cli.rs`, `tests/conformance/tool_library_cli.rs`, `examples/tool-library/README.md`
**Run**: `cargo nextest run -p rulery-cli --test cli; cargo nextest run --test conformance`

1. Add subprocess tests using the built `rulery` binary and temporary package copies.
2. Cover every command, supported format, stable schema discriminator, exit status, stdout/stderr split, invalid invocation, diagnostics, conflict, analysis exhaustion, I/O failure, and broken stdout.
3. Replace the predetermined fixture-helper path in `tool_library_cli.rs` with actual binary compilation and evaluation.
4. Prove check/analyze/test/explain/diff/render do not write source or lock files; prove only init, fmt without check, and lock perform intended writes.
5. Update the example README with commands that the subprocess tests execute verbatim.
6. Run `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, and the task test command.
7. Run `git branch --show-current`; stop if it is `main`.
8. Commit with `test(cli): lock executable command contracts`.

### Task 11: Update user documentation

**Crate**: `rulery`
**File(s)**: `README.md`, `docs/language/README.md`, `docs/semantics/README.md`, `tests/conformance/README.md`
**Run**: `cargo run -p xtask -- conformance`

1. Remove all claims that CLI dispatch is a stub.
2. Document installation, package validation, fixed-time explanation, analysis, scenarios, semantic diff, rendering, lock behavior, formats, and exit codes using tested commands.
3. Explain the Rust embedding boundary and stable versioned envelopes for future integrations.
4. Document how to add and run conformance cases.
5. Run Markdown/conformance validation and verify every command example against `--help`.
6. Run `git branch --show-current`; stop if it is `main`.
7. Commit with `docs(cli): document executable integration workflows`.

### Task 12: Run full repository verification

**Crate**: `rulery`
**File(s)**: `Cargo.toml`, `Cargo.lock`, `xtask/src/model.rs`
**Run**: `cargo fmt --all; cargo clippy --workspace -- -D warnings; cargo nextest run --workspace`

1. Run `cargo fmt --all`.
2. Run `cargo clippy --workspace -- -D warnings`.
3. Run `cargo nextest run --workspace`.
4. Run `cargo clippy --workspace --all-targets -- -D warnings`, `cargo run -p xtask -- architecture`, `cargo run -p xtask -- conformance`, and `cargo run -p xtask -- verify` as additional project gates.
5. Apply the three-attempt rule independently to each failing gate; after three failed attempts, stop and report the root cause without further patching.
6. Run `cargo fmt --all` again and re-stage all intended files.
7. Run `git branch --show-current`; stop immediately if it is `main`.
8. Commit only verified residual integration changes with a concrete conventional message derived from the diff.

## Completion Criteria

- Every command declared by `rulery --help` executes its specified operation.
- The CLI contains no policy semantics and delegates through `ApplicationService`.
- Arbitrary authored tool-library-equivalent packages compile and evaluate without predetermined traces.
- Machine outputs use strict versioned envelopes and stable exit codes.
- All required Cargo gates pass, with the working tree formatted and staged after the final format run.
