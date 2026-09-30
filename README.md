# Rulery

<!-- markdownlint-disable MD013 -->

Rulery is a local-first Rust compiler and evaluator for structured operational rules. It turns
YAML rule packages into deterministic intermediate representations, evaluates decisions with
explicit four-valued truth semantics, records reproducible traces, runs scenarios, performs static
analysis, and renders machine- and human-readable artifacts.

The project is deliberately not an LLM policy engine, workflow executor, remote package registry,
or substitute for accountable human judgment. Imports are local and the v0.1 design requires
offline operation.

## Status

Rulery is an experimental, pre-release `0.1.0` workspace under active development. The core
contracts, parser, compiler passes, evaluator, analysis, scenario, rendering, storage, facade, and
conformance fixtures are present. All nine `rulery` subcommands execute: the binary parses
arguments, runs the command through the root facade's application service, writes exactly one
artifact, and exits with the status the specification's exit-condition matrix names. The Rust APIs
and tests remain the implementation reference, and `crates/cli/tests/process_contract.rs` pins the
process-level contract by running the real binary.

The normative target is [the v0.1 core specification](docs/specification.md). A feature described
there should not be assumed complete unless it is also implemented in the current source.

## Architecture

The workspace enforces one responsibility per crate and an acyclic dependency direction:

| Package              | Responsibility                                                               |
| -------------------- | ---------------------------------------------------------------------------- |
| `rulery`             | Curated public facade, package assembly, embedding workflow, and macros      |
| `rulery-contracts`   | IDs, facts, values, outcomes, time, source maps, hashes, and lock contracts  |
| `rulery-diagnostics` | Stable diagnostic codes, reports, labels, evidence, and wire envelopes       |
| `rulery-syntax`      | YAML decoding, source maps, and the format-neutral source AST                |
| `rulery-vocabulary`  | Type schemas, terms, name resolution, and value validation                   |
| `rulery-ir`          | Checked expressions, rules, decisions, actions, and compiled packages        |
| `rulery-compiler`    | Validation, resolution, type checking, normalization, and lowering           |
| `rulery-engine`      | Four-valued evaluation, relevance, precedence, outcomes, and traces          |
| `rulery-analysis`    | Reachability, interactions, coverage, witnesses, budgets, and semantic diff  |
| `rulery-scenarios`   | Scenario compilation, execution, and exact result checking                   |
| `rulery-emit`        | Human, JSON, Markdown, decision-table, and SARIF rendering                   |
| `rulery-store`       | Local package discovery, imports, lockfiles, and integrity checks            |
| `rulery-cli`         | CLI arguments, orchestration contracts, output, and exit policy              |
| `rulery-macros`      | Optional procedural macros with compile-time literal validation              |
| `xtask`              | Repository bootstrap, architecture, conformance, and verification automation |

The root crate re-exports the component crates and provides two composition layers:

- `PackageAssembler` recursively loads local packages, validates imports and lock state, remaps
  source spans, and produces compiler input.
- `WorkflowFacade<P>` composes caller-supplied `FacadePorts` for compile, evaluate, analyze, diff,
  and scenario workflows.

## Build From Source

Rulery requires Rust 1.85 or newer and uses the Rust 2024 edition.

```sh
git clone https://github.com/89jobrien/rulery.git
cd rulery
cargo build --workspace
```

The packages are not documented as published crates. For an embedding project, use path
dependencies while developing locally:

```toml
[dependencies]
rulery = { path = "../rulery" }
```

The CLI builds and installs from its workspace package:

```sh
cargo build -p rulery-cli
cargo install --path crates/cli
cargo run -p rulery-cli -- --help
```

## Quickstart

The checked-in `tool_library_explain` helper is a fixture-integrity check and fixed conformance
artifact. It assembles the authored package under the frozen lock, compiles it, converts the
canonical case file into typed case facts, evaluates `checkout` with the production engine adapter
over a policy-local calendar pinned to the canonical instant, checks the authored scenario
expectation against the resulting trace, and renders JSON and human output from that trace. It is
bound to this fixture: the calendar holds policy-local date data for the canonical instant only, and
it is not a general evaluator path. For a general evaluator path, drive the CLI:

```sh
rulery explain examples/tool-library \
  --decision checkout \
  --facts examples/tool-library/cases/expired-training.yaml \
  --at 2026-09-16T16:00:00.000000000Z
```

```rust
use std::path::Path;

use rulery::contracts::UtcInstant;

fn main() -> Result<(), String> {
    let evaluated_at = UtcInstant::new(1_789_574_400_000_000_000)
        .map_err(|error| error.to_string())?;
    let result = rulery::tool_library_explain(
        Path::new("examples/tool-library"),
        evaluated_at,
    )?;

    println!("{}", result.human);
    println!("trace hash: {}", result.trace_hash);
    Ok(())
}
```

See [`examples/tool-library`](examples/tool-library) for the package manifest, vocabulary,
actions, rules, facts, scenarios, and lockfile used by the conformance tests.

## CLI Surface

All nine v0.1 commands execute. Each writes exactly one artifact to stdout, keeps human-mode
diagnostics on stderr, and exits with the first matching row of the specification's exit-condition
matrix. Use `rulery <COMMAND> --help` for required options and all declared flags.

| Command                | Operation                                                     | Formats                        |
| ---------------------- | ------------------------------------------------------------- | ------------------------------ |
| `init [PATH]`          | Scaffold the five files a package root requires               | human path line                |
| `fmt [PATH] [--check]` | Rewrite authored source into the canonical form               | canonical bytes, or human      |
| `check [PATH]`         | Validate a package, optionally with `--frozen`                | human, JSON, SARIF             |
| `analyze [PATH]`       | Run bounded static analysis                                   | human, JSON, SARIF             |
| `test [PATH]`          | Run root-package scenarios                                    | human, JSON                    |
| `explain [PATH]`       | Explain one decision for a facts file and optional fixed time | human, JSON                    |
| `diff BEFORE AFTER`    | Compare two packages semantically                             | human, JSON                    |
| `render [PATH]`        | Render Markdown, JSON, or a decision table                    | Markdown, JSON, decision table |
| `lock [PATH]`          | Resolve and atomically write a complete lockfile              | human path line                |

`--frozen` requires exact agreement with `rulery.lock`. `check`, `analyze`, `test`, `explain`,
`diff`, and `render` use update mode unless `--frozen` is supplied and never write a lock; only
`lock` writes one. Environment variables never change lock mode — CI must pass `--frozen`
explicitly.

### Exit status

| Status | Name                 | Meaning                                                      |
| ------ | -------------------- | ------------------------------------------------------------ |
| 0      | `Success`            | The command completed                                        |
| 1      | `DiagnosticsError`   | Error diagnostics, promoted warnings, or `fmt --check` drift |
| 2      | `InvalidInvocation`  | Bad syntax, bad argument value, unknown format or decision   |
| 3      | `IoFailure`          | A required read, write, or rename failed                     |
| 4      | `InternalFailure`    | An internal invariant failed                                 |
| 5      | `ScenarioFailure`    | `test` compiled and at least one scenario failed             |
| 6      | `AnalysisIncomplete` | Analysis or semantic diff was inconclusive                   |

Human and JSON formats map the same semantic result to the same code.

### Scaffolding and formatting

`init` derives the package identity from the destination directory name and writes a package that
`check` accepts with no diagnostic. It refuses a non-empty destination and never writes a lock.

```sh
rulery init ./community-policy   # -> community-policy/rulery.yaml
rulery check ./community-policy  # -> no findings
```

`fmt` renders the parsed document rather than the lowered AST, so it never inserts a field the
author did not author. It sorts mapping keys, fixes indentation to two spaces with block sequences
aligned to their owning key, normalizes scalar quoting, uses flow form for empty collections, and
removes comments — a change, not an error. The form is idempotent, so a canonical file is not
rewritten. Naming a single file reports its canonical bytes on stdout and leaves the file alone;
naming a package rewrites in place.

```sh
rulery fmt ./community-policy --check  # exit 1 while any file is non-canonical
```

## Configuration And Features

- The root crate has no default Cargo features.
- Enable `macros` to re-export `rulery-macros` and derive `RuleFacts`:

  ```toml
  rulery = { path = "../rulery", features = ["macros"] }
  ```

- Authored packages use `rulery.yaml` plus optional vocabulary, action, rule, case, and scenario
  files. The exact v0.1 layout and grammar are defined in the specification.
- Evaluation semantics include `true`, `false`, `unknown`, and `invalid`; outcomes include
  `approve`, `deny`, `escalate`, and `request_information`.
- Local import integrity is captured in `rulery.lock`. There is no network import mechanism.

## Development

Run the repository gates from the workspace root:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
cargo run -p xtask -- architecture
cargo run -p xtask -- conformance
cargo run -p xtask -- verify
```

`xtask bootstrap --check` checks the workspace against its declarative crate model;
`xtask bootstrap --dry-run` prints proposed scaffold changes without applying them.

Tests cover unit behavior inside each crate, workspace architecture, specification conformance,
wire contracts, safety properties, and the complete tool-library fixture. The workspace forbids
unsafe code and warns on missing public documentation.

## Documentation

- [Core specification](docs/specification.md) - normative v0.1 language and behavior
- [Language documentation](docs/language/README.md) - currently a placeholder for expanded guides
- [Semantic documentation](docs/semantics/README.md) - currently a placeholder for expanded guides
- [Conformance suite](tests/conformance/README.md) - conformance test organization
- [Architecture decisions](docs/adr/README.md) - ADR index
- [Design notes](docs/designs) - implementation and repository automation designs
- [Changelog](CHANGELOG.md) - release-oriented project history

## License

Dual licensed under MIT or Apache-2.0, at your option. The full text of each license is in
[`LICENSE-MIT`](LICENSE-MIT) and [`LICENSE-APACHE`](LICENSE-APACHE).
