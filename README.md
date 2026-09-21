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
conformance fixtures are present. The `rulery` binary currently exposes the planned command-line
syntax but only parses arguments; command dispatch is not yet connected in `main`. Use the Rust
APIs and tests as the executable implementation reference for now.

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

The CLI can be built or installed from its workspace package, but its runtime dispatch is still a
stub:

```sh
cargo build -p rulery-cli
cargo install --path crates/cli
cargo run -p rulery-cli -- --help
```

## Quickstart

The checked-in tool-library fixture provides a currently executable explanation path. It checks
source integrity against the frozen lock, reads case and scenario fixtures, constructs a
deterministic decision trace for the canonical case, and renders JSON and human output. It is a
focused conformance helper rather than the not-yet-wired general CLI workflow.

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

The parser defines the following v0.1 commands. They are useful for inspecting the intended
interface, but the binary does not yet execute them.

| Command                | Intended operation                                            |
| ---------------------- | ------------------------------------------------------------- |
| `init [PATH]`          | Initialize an empty package destination                       |
| `fmt [PATH] [--check]` | Format source documents or check formatting                   |
| `check [PATH]`         | Validate a package, optionally with `--frozen`                |
| `analyze [PATH]`       | Run bounded static analysis                                   |
| `test [PATH]`          | Run root-package scenarios                                    |
| `explain [PATH]`       | Explain one decision for a facts file and optional fixed time |
| `diff BEFORE AFTER`    | Compare two packages semantically                             |
| `render [PATH]`        | Render Markdown, JSON, or a decision table                    |
| `lock [PATH]`          | Resolve and atomically write a complete lockfile              |

Declared output formats include human text, JSON, SARIF, Markdown, and decision tables depending
on the command. `--frozen` requires exact agreement with `rulery.lock`; update mode proposes a
replacement lock, while only the lock workflow is intended to write it.

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

Workspace package metadata declares dual licensing under MIT or Apache-2.0.
