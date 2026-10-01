# Tech Context

## Stack

| Property | Value                                                | Source                              |
| -------- | ---------------------------------------------------- | ----------------------------------- |
| Language | Rust, **edition 2024**                               | `Cargo.toml:7`                      |
| MSRV     | **1.98**, the toolchain the gate runs on             | `Cargo.toml:11`                     |
| Resolver | 3                                                    | `Cargo.toml:3`                      |
| Version  | `0.1.0`, lockstep across all publishable crates      | `Cargo.toml:6`, `.config/rail.toml` |
| License  | `MIT OR Apache-2.0`                                  | `Cargo.toml:12`                     |
| Remote   | `github` → `https://github.com/89jobrien/rulery.git` | `git remote -v`                     |

## Workspace members

15 total: the root `rulery` facade, 13 crates under `crates/`, and `xtask`. Declared as
`members = ["crates/*", "xtask"]` (`Cargo.toml:2`).

| Crate                | Responsibility                                                      | Source         |
| -------------------- | ------------------------------------------------------------------- | -------------- |
| `rulery`             | Curated public facade, package assembly, embedding workflow, macros | `CHANGELOG.md` |
| `rulery-contracts`   | IDs, facts, values, outcomes, time, source maps, hashes, lock       | `CHANGELOG.md` |
| `rulery-diagnostics` | Stable codes, reports, labels, evidence, wire envelopes             | `CHANGELOG.md` |
| `rulery-syntax`      | YAML decoding, source maps, source AST                              | `CHANGELOG.md` |
| `rulery-vocabulary`  | Type schemas, terms, name resolution, value validation              | `CHANGELOG.md` |
| `rulery-ir`          | Checked expressions, rules, decisions, actions, packages            | `CHANGELOG.md` |
| `rulery-compiler`    | Validation, resolution, type checking, lowering                     | `CHANGELOG.md` |
| `rulery-engine`      | Four-valued evaluation, relevance, precedence, outcomes, traces     | `CHANGELOG.md` |
| `rulery-analysis`    | Reachability, interactions, coverage, witnesses, budgets, diffs     | `CHANGELOG.md` |
| `rulery-scenarios`   | Scenario compilation, execution, exact result checking              | `CHANGELOG.md` |
| `rulery-emit`        | Human, JSON, Markdown, decision-table, SARIF rendering              | `CHANGELOG.md` |
| `rulery-store`       | Local discovery, imports, lockfiles, integrity                      | `CHANGELOG.md` |
| `rulery-cli`         | CLI arguments, orchestration contracts, output, exit policy         | `CHANGELOG.md` |
| `rulery-macros`      | Optional procedural macros with compile-time validation             | `CHANGELOG.md` |
| `xtask`              | Bootstrap, architecture, conformance, verification automation       | `CHANGELOG.md` |

## Build and gate commands

All from the workspace root. **Run everything from the root.**

```sh
cargo xtask verify                                    # THE authoritative gate, 8 gates fail-fast
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace --all-features
cargo test --workspace --all-features --doc
cargo doc --workspace --no-deps

cargo xtask architecture      # membership + dependency boundaries
cargo xtask conformance       # specification contract
cargo xtask bootstrap --check # reconcile vs declarative model, no writes
cargo xtask bootstrap --dry-run
```

Single tests — use `-p`, a bare `cargo nextest run` is slow across 15 members:

```sh
cargo nextest run -p rulery-engine truth_tables_match_all_36_cells
cargo nextest run -p rulery-compiler predicate
cargo nextest run -E 'test(precedence) and package(rulery-engine)'
```

## Lint baseline

`Cargo.toml:26-31` sets `missing_docs = "warn"`, `unsafe_code = "forbid"`, and `clippy::all` +
`clippy::pedantic` at `warn`. Every manifest opts in with `[lints] workspace = true`, and
`-D warnings` promotes warnings to errors. Source: `AGENTS.md`.

This is the strictest part of the project and dictates most code style. Consequences that have
actually bitten:

- Every public item, field, and enum variant needs a doc comment.
- Every fallible public fn needs a `# Errors` section. **`# Panics` is never used** — so a
  production `expect` trips `clippy::missing_panics_doc` and cannot be fixed by documenting it.
  Use `if let (Some(a), Some(b)) = (x.pop(), y.first())` instead.
- `#[allow(...)]` is reserved for size/shape lints only: `too_many_lines`, `too_many_arguments`,
  `struct_field_names`, `struct_excessive_bools`, `naive_bytecount`, `wildcard_imports`. Never
  allow dead code, unused, or `clippy::all`. No `#[expect(...)]`.
- Proc-macro test helpers expand undocumented functions, so attribute macros that generate test
  bodies (`criterion_group!`, and by extension `rstest`) need a hand-written entry point instead.

## External dependencies

Deliberately few, and **no network-capable crate** — asserted by
`tests/workspace_scaffold.rs::resolved_dependency_closure_has_no_network_capable_crate`.

| Crate                          | Used for                                                           |
| ------------------------------ | ------------------------------------------------------------------ |
| `serde`, `serde_json`          | wire envelopes; `serde_json::Value` round-trips for canonical JSON |
| `serde_yaml` / `serde_yaml_ng` | authored YAML decoding                                             |
| `thiserror`                    | derive on every error type                                         |
| `jiff`                         | time-zone database and policy-local dates                          |
| `rust_decimal`                 | `DecimalValue` behind a newtype                                    |
| `blake3`, `hex`                | canonical content hashing                                          |
| `xshell`                       | process execution inside `xtask`                                   |

Dev-only: `proptest` (contracts, engine), `insta` (emit), `criterion` (analysis), `tempfile` (cli).

## Optional features

The root facade has exactly one: `macros = ["dep:rulery-macros"]` (`Cargo.toml:43-45`), default off.

This matters more than it looks. `xtask conformance` compiles the renamed-dependency hygiene fixture
**twice** — with default features and with `--features macros` — because the specification requires
both. A macro hidden behind a feature that is never exercised is a macro that is quietly broken.

## Checked-in fixtures

`examples/` holds normative artifacts, each with tests that assert rather than merely document:

| Path                                | Asserted by                                                      |
| ----------------------------------- | ---------------------------------------------------------------- |
| `examples/tool-library/`            | `tests/conformance/tool_library.rs`, frozen lock digests         |
| `examples/import-graph/`            | multi-package import closure, pinned content digests             |
| `examples/canonical-authored-form/` | `fmt --check` must report already-canonical                      |
| `examples/scaffolded-package/`      | byte-for-byte equal to what `init` writes                        |
| `examples/macro-hygiene/`           | compiled under two feature configurations by `xtask conformance` |

`examples/macro-hygiene/` is a **separate workspace** with its own `Cargo.lock`, deliberately named
`rlry` in its dependency on `rulery`, because `rulery-macros` resolves the facade by name.

## Constraints worth remembering

- `cargo fmt --all`, clippy, and nextest are the pre-commit gates; `cargo xtask verify` is the full one.
- There is no `.github/`, no `rustfmt.toml`, no `clippy.toml`, no `deny.toml`. Adding any of those
  requires a design note.
- `target/debug/rulery` can be stale relative to the working tree. Confirm freshness before
  trusting a hand-run of the binary as evidence.
- The shell is **Nushell**. See `mistakes.md` — this is the single largest source of wasted turns.
