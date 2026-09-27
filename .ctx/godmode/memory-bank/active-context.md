# Active Context

## Current focus

- cli18 is the only blocked item: `init` and `fmt` are the two commands the specification does not
  determine. Everything else in the graph is runnable (cli10 is next).

## Current state

- Godmode graph: 84 done, 0 running, 1 blocked, 3 pending (`.ctx/godmode/tasks.yaml`).
- `cargo xtask verify` passes all seven gates. `cargo nextest run --workspace` is 98 tests,
  98 passing. `cargo clippy --workspace --all-targets -- -D warnings` is clean.
- The `rulery` binary executes. `check`, `analyze`, `test`, `explain`, `diff`, `render`, and `lock`
  are implemented against spec lines 2964-3053 with the full nine-row exit matrix.
- The v0.1 requirement gate (`xtask::check_requirements`) exits 0: all twelve requirements and all
  nine static checks are `satisfied`, each against a named test that exists in the workspace.
- HEAD is `42dc7e0`; the working tree is dirty and uncommitted.

## Completed recently

- cli9: the binary now runs. `runtime.rs` holds the production `ArtifactPorts`/`CommandPorts`;
  `dispatch.rs`, `report.rs`, `findings.rs`, `facts.rs`, `error.rs` implement the commands;
  `main.rs` converts `ExitStatus` to a process exit code.
- cli17: authored literals keep their structure. New `SourceValue` in `crates/syntax/src/ast.rs`
  replaces the flattened `String` in `SourceOperand::Literal`; the parser builds it from YAML; a
  recursive vocabulary-guided `decode` in `src/application.rs` handles records and lists; and
  `resolve` now synthesizes the eleven primitive built-ins, which are named by identity and never
  declared. `rulery test examples/tool-library --frozen` passes.
- cli5: the analysis input-extraction layer, which closed the requirement gate.
- cli15: `tool_library_explain` drives a real pipeline and the conformance test actually evaluates.
- Fixed: the scenario bridge read the authored `at` with the signed-nanosecond wire grammar
  (`UtcInstant::parse_rfc3339` now exists), and `validate_value` marked every correctly-typed
  scalar and every enum as `Malformed`.

## Known deviations, deliberately not hidden

- `ProductionApplication` hard-codes `JiffTimeZoneDatabase`, so a scenario trace records the host
  identity `jiff/system-tzdb` even when the instant is pinned. That weakens the cross-machine
  reproducibility the canonical success criterion requires. The fix is to inject the database.
- When a failure precedes artifact production and no `Diagnostic` is constructible, the CLI emits
  no stdout artifact and reports on stderr. Affects `RUL400`, `RUL401`, `RUL900`.
- `Diagnostic` confidence and labels are private with no accessors, so the CLI reads them through
  `serde_json`. Every value is a real input, but this should be an accessor.
- A `datetime` literal in authored source is read with the signed-nanosecond grammar. The spec says
  only that date-time literals must be quoted, so the authored form is ambiguous and was left alone.

## Blockers

- **cli18 (`init` and `fmt`)**: `init` pins no template bytes and no required-file list. `fmt` has
  no canonical authored-source form; the only "canonical" in the specification is JCS JSON for
  hashed payloads. Both are surfaced as explicit typed-unavailable errors rather than fabricated.
  Needs a specification amendment or an explicit deferral from v0.1.
- Release: Cargo Rail file scope, and the initial `0.1.0` / `v0.1.0` tag need confirmation.

## Environment

The `cargo-fmt-on-edit` and `memory-vault-sync` hooks in `~/.config/crs/plugins.d/godmode.toml`
pointed at `/Users/joe/dotfiles/.claude/hooks/`, which no longer exists after the migration to
notfiles, so both were dead. They now resolve through the managed `~/.claude/hooks/` symlinks and
were confirmed firing via `crs log`.

## Decisions

- `analyze` and `diff` fail with a typed error rather than returning a report that was never
  derived. Reporting an un-derived analysis result would be worse than reporting nothing.
- The `ir` re-export of the vocabulary surface stays limited to schema types. The scenario bridge
  needs vocabulary's validation behavior, so the engine takes a direct modelled edge instead of
  widening that re-export.
- CLI policy layers (`run_write_command`, `run_artifact_command`) are reused rather than replaced;
  the production work is adapters behind `CommandPorts` and `ArtifactPorts`, plus
  `run_command`/`write_command_output`/`main.rs`.

## Open questions

- Include or split the Cargo Rail release-planning files?
- Confirm the initial release target as workspace version/tag `0.1.0`/`v0.1.0`.
- Should the canonical authored-source form for `fmt` be added to the specification, or should
  `fmt` be deferred from v0.1?
