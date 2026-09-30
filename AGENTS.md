# AGENTS.md

Rulery is a local-first Rust **compiler and evaluator for structured operational rules**. It turns
YAML rule packages into deterministic IR, evaluates with explicit four-valued truth
(`true`/`false`/`unknown`/`invalid`), records reproducible traces, runs scenarios, performs static
analysis, and renders JSON/Markdown/SARIF/decision-table artifacts.

Cargo workspace, **Rust 2024 edition, MSRV 1.85**, 15 members (root `rulery` facade + 13
`crates/*` + `xtask`). No `.cursor/rules`, `.cursorrules`, or `.github/copilot-instructions.md`
exist — this file is the agent contract.

## Commands

Run everything from the workspace root.

```sh
cargo build --workspace                      # build all
cargo fmt --all                             # format (stock rustfmt, no rustfmt.toml)
cargo fmt --all --check                     # gate: formatting
cargo clippy --workspace --all-targets -- -D warnings   # gate: lints (warnings are errors)
cargo nextest run --workspace               # gate: tests
cargo doc --workspace --no-deps             # gate: docs
```

The authoritative gate is `cargo xtask verify` (`.cargo` alias for `run -p xtask --`). It runs
fail-fast in this exact order (`xtask/src/verify.rs:44-52`): bootstrap check, conformance,
architecture, fmt, clippy, nextest, rustdoc. Individual gates:

```sh
cargo xtask architecture    # workspace membership + dependency boundaries
cargo xtask conformance     # docs/specification.md contract
cargo xtask bootstrap --check      # reconcile vs declarative model, no writes
cargo xtask bootstrap --dry-run    # print proposed scaffold changes
```

### Running a single test

```sh
cargo nextest run -p rulery-engine truth_tables_match_all_36_cells   # one test, one crate
cargo nextest run -p rulery-engine predicate                        # all tests whose name contains
cargo nextest run all_36_cells                                      # substring match across workspace
cargo nextest run -E 'test(precedence) and package(rulery-engine)' # filter expression
cargo test -p rulery-contracts --test ids stable_ids_enforce_wire_grammar  # one integration test
```

Use `-p <package>` liberally; a bare `cargo nextest run` is slow in a 15-member workspace.

### Release (cargo-rail)

`mise.toml` and `.config/rail.toml` define a lockstep release — all publishable crates move as one
`version_groups` entry. `mise run release:gate` then `release:check`; mutations require `main`.

## Architecture is data, not convention

`xtask/src/model.rs:59-197` is a single `const CRATES` table — the **only** source of truth for
member name, manifest path, target kind, and allowed internal dependencies. Adding or renaming a
crate means editing that table; `xtask bootstrap` can then materialize manifests.
`tests/workspace_scaffold.rs:5-19` hard-codes the crate list as an independent second check.

Dependency DAG — acyclic, enforced by `xtask/src/architecture.rs:27-81`:

```
contracts ─┬─ diagnostics, syntax, vocabulary ─┬─ ir ─┬─ store
           │                                  │     └─ engine ─┬─ compiler ─┬─ scenarios, analysis
           └──────────────────────────────────┴────────────────┴─ scenarios ─┴─ emit ── rulery ── rulery-cli
```

- `rulery-macros` (`ProcMacro`) and `xtask` are isolated leaves.
- Only **internal** deps are allowlisted; external crates are unconstrained.
- A forbidden edge is `XtaskError::ForbiddenDependency`. Do not "fix" it by editing
  `architecture.rs` — add the edge to `model.rs`.
- `rulery-contracts` and `rulery-ir` re-export vocabulary's _schema_ surface
  (`ResolvedVocabulary`, `FieldDeclaration`, `crates/ir/src/lib.rs:20-23`) so downstream crates can
  read a compiled vocabulary without depending on `rulery-vocabulary`. That re-export is
  deliberately limited to types — vocabulary's _validation behavior_ (`FactLookupState`,
  `validate_case_facts`) is not re-exported, so crates that validate case facts
  (`rulery-engine`, `rulery-compiler`, `rulery-analysis`) take a direct, modeled edge instead.

## Lint baseline — this dictates the style

`Cargo.toml:26-31` sets `missing_docs = "warn"`, `unsafe_code = "forbid"`, and `clippy::all` +
`clippy::pedantic` at `warn`. Every manifest opts in with `[lints] workspace = true`, and
`-D warnings` promotes warnings to errors. Consequences:

- Every file starts with `//!` docs. Crate roots get a one-line title then a paragraph naming the
  invariant the crate enforces, with `[`intra-doc links`]`. Non-root modules get a bare verb-first
  one-liner (`//! Validated stable identifiers.`).
- **Every public item, field, and enum variant is documented.** `missing_docs` covers fields.
- **Every fallible public fn needs a `# Errors` section** naming the returned error. `# Panics` is
  never used.
  ```rust
  /// Creates an identifier after validating its canonical wire grammar.
  ///
  /// # Errors
  ///
  /// Returns [`StableIdError`] when the value is empty, too long, non-ASCII, or contains an
  /// invalid character or boundary.
  pub fn new(value: impl Into<String>) -> Result<Self, StableIdError> {
  ```
- **`#[must_use]` on every pure getter, infallible constructor, and `push_*`/`with_*` builder
  setter** — after the doc comment, before `pub`. Trivial accessors are often
  `#[must_use] pub const fn`.
- `#![forbid(unsafe_code)]` is repeated in every `lib.rs` on top of the workspace lint.
- `#[allow(...)]` is reserved for size/shape lints only: `too_many_lines`, `too_many_arguments`,
  `struct_field_names`, `struct_excessive_bools`, `naive_bytecount`, `wildcard_imports`
  (`crates/syntax/src/yaml/mod.rs:2` is the only file-level allow). **Never** allow dead code,
  unused, or `clippy::all`. No `#[expect(...)]`.

## Code style

**Files & modules.** snake_case filenames. Flat `src/*.rs` per crate — do not create nested module
trees (only `crates/syntax/src/yaml/` exists). Leaf modules are declared `mod foo;` (private) and
re-exported from `lib.rs` in an alphabetized braced `pub use` block, types first then lowercase fns.
`lib.rs` is a pure facade: module decls, docs, re-exports, no logic. The root crate re-exports whole
crates as modules (`src/lib.rs:39-53`).

**Imports.** Three blank-line-separated groups: `std`/`core` first; then external crates
alphabetized with first-party `rulery_*` sorted among them; then `use crate::{...}` last, always
braced. Inside braces, types uppercase first then lowercase fns. No prelude module, no nested
`mod imports` blocks. Rename on import rather than at use sites
(`CompilationInput as PackageCompilationInput`, `crates/compiler/src/lib.rs:25-31`). In
`#[cfg(test)] mod tests`, end the import list with `use super::*;` alone.

**Types.** Newtype at every boundary — `define_typed_id!` generates `RuleId`, `PackageId`, etc.
with validating constructors, `Display`, `FromStr`, `AsRef<str>`, and hand-written `Deserialize`
that routes through the validator. No bare domain primitives in public signatures: instants are
`i128` nanoseconds behind `UtcInstant`, spans are `u32` **half-open** intervals, decimals are
`rust_decimal::Decimal` behind `DecimalValue`. Wire contracts use validated-wrapper + `…V1` payload

- `…Envelope`, always `#[serde(tag = "schema", content = "payload", deny_unknown_fields)]` with
  `rename_all = "snake_case"`. `#[serde(deny_unknown_fields)]` on every wire struct,
  `#[serde(transparent)]` on single-field newtypes, `skip_serializing_if = "Option::is_none"` on
  optionals. No `#[non_exhaustive]` — cross-crate enums are matched with an explicit `_ =>` arm.

Derive order: `Clone, Debug, Eq, PartialEq, Serialize, Deserialize`. Add `Copy` for small
enums/values, `Hash`/`Ord`/`PartialOrd` for anything used as a `BTreeMap` key. Errors always
`Clone, Debug, Eq, Error, PartialEq` — never `Copy`. Derive `Default` for POD configs; write
`impl Default` when defaults are non-zero, or use `#[default]` on an enum variant.

**Collections.** `BTreeMap`/`BTreeSet` universally — never `HashMap`/`HashSet` — so serialization
is byte-deterministic. Sort keys are always explicitly chained with `.then_with(...)`.

**Naming.** Functions verb-first, snake*case, no abbreviations in public API
(`validate_architecture`, `parse_bundle`, `minimize_witness`); short names reserved for accessors
(`as_str`, `code`, `kind`). Error types are `<Subject>Error` and state \_what was rejected*
(`StableIdError`, `TimeValueError`, `PrecedenceError`) — never `ParseError`. Error variants are
PascalCase nouns with structured named fields (`ImportCycle { cycle: Vec<PackageId> }`); unit
variants only when the message alone is diagnostic. Consts are `SCREAMING_SNAKE` and their value is
a stable wire string (`SYNTAX_INVALID = "RUL001"`). Ports are named for the role, not the trait
(`PackageStore`, `FacadePorts`, `VerifyRunner`); adapters are `Host*` or technology-named
(`HostFileSystem`, `YamlSourceParser`, `FixedClock`).

**Error handling.** `thiserror` derive is universal (two hand-written `Display`/`Error` impls
exist in `crates/compiler`). Delegate cross-crate with `#[error(transparent)]` + `#[from]` (use a
fully-qualified path in the attribute when the type isn't imported). Flatten to a string with
`.map_err(|error| error.to_string())`. **There is no `error!`/`bail!`/`abort!` macro** — construct
errors directly or via a private `new(field, message)` constructor. `expect`/`unwrap`/`panic!` are
test-only; the handful of production `expect`s assert a documented internal invariant with a
message saying _why_ it cannot fire (`crates/syntax/src/ast.rs:318`). `todo!`/`unimplemented!`
are forbidden outright — `xtask/src/conformance.rs:55-56` greps for them.

**Iterators & control flow.** Iterator chains dominate collection building. Use `for` when
mutating borrowed state or when order matters. Prefer `let … else` over early `return` for
irrefutable pattern rejection. `const fn` for trivial accessors and pure constructors. `format!`
for owned construction, `write!`/`formatter.write_str` inside `Display`, `push_str` in renderers
where newlines are a wire contract. Inline format args always (`{error}`, never `{}` positional).
Digit separators on literals (`1_024`, `32 * 1024 * 1024`).

**Ports, not mocks.** Every I/O boundary is a trait with a `Default` unit-struct host adapter and
no mocking crate in the dependency tree.

## Tests

- **Unit tests are inline `#[cfg(test)] mod tests`** at the bottom of the source file, ending in
  private helper fns. **Integration tests live in `<crate>/tests/`.**
- `tests/conformance.rs` is a 10-line aggregator folding four suites into one binary via
  `#[path = "conformance/<name>.rs"] mod <name>;`. Add a suite there, not a new top-level file.
- `crates/cli/tests/process_contract.rs` is the workspace's first crate-level integration suite. It
  spawns the real binary, so it is the only place a process-boundary regression can be caught. It
  locates the fixture with `concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/tool-library")`
  and copies it into a per-test scratch directory under `std::env::temp_dir()` before anything that
  writes. Run commands with `Command::env_clear()` so an ambient `CI` cannot leak into the result.
- Test names are `<subject>_<behavior>` in snake*case — \*\*no `test*`prefix**, no`Type::method` nesting
(`stable_ids_enforce_wire_grammar`, `spans_are_half_open_utf8_intervals`).
- Table-driven loops over literal arrays instead of parametrised tests:
  `for invalid in [String::new(), "a".repeat(129)] { assert!(...is_err(), "accepted {invalid:?}"); }`
- Wire contracts use the `assert_strict` helper (`tests/conformance/wire.rs:95-119`): round-trip,
  then reject unknown field, wrong schema tag, and missing payload.
- Fixtures are checked in under `examples/tool-library/`, located with
  `env!("CARGO_MANIFEST_DIR")`. **Editing those files or their lock requires regenerating the
  frozen integrity hash** or the conformance gate fails.
- Test-only state via `Cell`/`RefCell` fakes implementing port traits, plus `#[cfg(test)]` builder
  toggles on production types (`with_fail_before_rename`, `crates/store/src/filesystem.rs:100-105`).
- Test `expect` messages are 1–3 lowercase words, no punctuation (`.expect("valid code")`).

## Gotchas

- `docs/specification.md` is **machine-gated**. `xtask conformance` requires 9 named `##` headings,
  7 schema tags, 40 unique `RUL###` codes, 4 fixed BLAKE3 vectors, and rustfmt-clean ` ```rust `
  blocks. Changing the spec means changing the spec _and_ passing that gate.
- The `rulery` binary's command dispatch **is wired** (`crates/cli/src/main.rs` calls
  `rulery_cli::run`). All nine v0.1 subcommands execute and resolve their exit status through the
  matrix in `crates/cli/src/dispatch.rs`. `crates/cli/tests/process_contract.rs` is the only suite
  that observes a finished process, via `env!("CARGO_BIN_EXE_rulery")`; it is where a change to
  argument parsing, stream routing, or process exit belongs.
- `init` and `fmt` were unspecified until the `Scaffolded package` and `Canonical authored form`
  sections were added under `## CLI`. Their templates live in `crates/cli/src/scaffold.rs` and are
  rendered _through_ the emitter in `crates/cli/src/canonical.rs`, so a fresh scaffold is canonical
  by construction. Do not hand-order a scaffold; do not add a 41st `RUL` code to report a comment
  removal.
- `canonical.rs` is defined over the **parsed document**, not the lowered `SourcePackage`. The typed
  AST has already discarded field presence, so a formatter that walked it would fill in defaulted
  fields the author never wrote.
- There is **no in-repo CI** (no `.github/`) — `cargo xtask verify` _is_ the gate. Likewise no
  `rustfmt.toml`, `clippy.toml`, or `deny.toml`; do not add one without a design note.
- `target/debug/rulery` can be **stale** relative to the working tree. Before trusting a
  hand-run of the binary as evidence, confirm it is fresh (`cargo clean -p rulery-cli` if in
  doubt). A stale binary once reported a spec violation that did not exist.
- `Command::lock_mode` (`crates/cli/src/args.rs`) is the **single source of truth** for assembly lock
  mode; `dispatch.rs::lock_of` delegates to it. Do not re-inline the `frozen` decision in dispatch —
  a second implementation is what let the public accessor and the shipped behavior drift apart
  unobserved.
- Conventional commits: `feat(scope):`, `fix(scope):`, `docs:`, `build:`, `chore:`.
