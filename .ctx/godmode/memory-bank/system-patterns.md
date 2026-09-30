# System Patterns

## Architecture is data, not convention

The defining constraint of this codebase: **there is exactly one source of truth for workspace
structure**, and it is a table, not a set of conventions people are expected to remember.

`xtask/src/model.rs:59-197` holds a single `const CRATES` table recording each member's name,
manifest path, target kind, and allowed internal dependencies. Adding or renaming a crate means
editing that table. `tests/workspace_scaffold.rs:5-19` independently hard-codes the crate list as a
second check, so a drift between the table and reality fails the gate rather than being discovered
later.

## Dependency DAG

Acyclic, enforced by `xtask/src/architecture.rs:27-81`. A forbidden edge produces
`XtaskError::ForbiddenDependency`. The documented remedy is _add the edge to `model.rs`_, never to
edit the checker.

```
contracts ─┬─ diagnostics, syntax, vocabulary ─┬─ ir ─┬─ store
           │                                  │     └─ engine ─┬─ compiler ─┬─ scenarios, analysis
           └──────────────────────────────────┴────────────────┴─ scenarios ─┴─ emit ── rulery ── rulery-cli
```

`rulery-macros` (a `ProcMacro` crate) and `xtask` are isolated leaves.

Only **internal** dependencies are allowlisted; external crates are unconstrained, which is why the
no-network-capable-dep rule is enforced separately by reading the resolved `Cargo.lock` rather than
by the architecture model.

## The data flow

```
authored YAML
  → rulery-syntax          decode to a format-neutral source AST + source map
  → rulery-vocabulary      resolve types, names, roots; validate values
  → rulery-compiler        type check, normalize, lower to checked IR
  → rulery-ir              CompiledPackage (the single compiled form)
  → rulery-engine          evaluate against CaseFacts → Evaluation + DecisionTrace
  → rulery-analysis        partition cells, evaluate each, coverage/witnesses/diffs
  → rulery-emit            human, JSON, Markdown, decision table, SARIF
  → rulery-store           package discovery, imports, lockfile, integrity
  → rulery-cli             argument parsing, orchestration, one artifact, exit status
```

Every consumer after `rulery-ir` is a **projection** of the compiled form. Nothing re-implements the
rule to produce a different view, which is what makes traces, analyses, and renderings agree.

## Two composition layers on the facade

`src/lib.rs` is a pure facade — module declarations, docs, re-exports, no logic. It re-exports the
component crates as modules (`pub use rulery_engine as engine;`) and adds:

- **`PackageAssembler`** — recursively loads local packages, validates imports and lock state,
  remaps source spans, produces compiler input.
- **`WorkflowFacade<P>`** — composes caller-supplied `FacadePorts` for compile, evaluate, analyze,
  diff, and scenario workflows.

`WorkflowFacade` is the reason embedding works: an embedder supplies its own file system, clock,
and time-zone database through ports, and gets the same pipeline the CLI uses. Source: `README.md:49-53`.

## Conventions that are load-bearing

### Collections

`BTreeMap`/`BTreeSet` universally, **never** `HashMap`/`HashSet`. The reason is byte-deterministic
serialization, which is what makes content hashes and trace identities reproducible. Sort keys are
always explicitly chained with `.then_with(...)`.

### Error handling

`thiserror` derive is universal; two hand-written `Display`/`Error` impls exist in
`crates/compiler`. Cross-crate errors delegate with `#[error(transparent)]` + `#[from]`. Errors are
`Clone, Debug, Eq, Error, PartialEq`, never `Copy`.

There is **no `error!`/`bail!`/`abort!` macro** — construct errors directly or via a private
`new(field, message)` constructor. `todo!`/`unimplemented!` are forbidden outright, and
`xtask/src/conformance.rs:55-56` greps the tree for them.

Error type names state _what was rejected_ (`StableIdError`, `TimeValueError`, `PrecedenceError`),
never `ParseError`. Variants are nouns with structured named fields
(`ImportCycle { cycle: Vec<PackageId> }`).

### Ports, not mocks

Every I/O boundary is a trait with a `Default` unit-struct host adapter named for its role or
technology (`PackageStore`, `FacadePorts`, `HostFileSystem`, `YamlSourceParser`, `FixedClock`).
**No mocking crate is in the dependency tree.** Test doubles are `Cell`/`RefCell` fakes implementing
the port, plus `#[cfg(test)]` builder toggles on production types.

### Documentation as contract

`missing_docs` is on. Crate roots get a one-line title then a paragraph naming the invariant the
crate enforces, with intra-doc links. Every public field and enum variant is documented. A doc
comment that cannot state an invariant is a signal the design is unclear.

## Test placement

- **Unit tests inline** in `#[cfg(test)] mod tests` at the bottom of the source file, ending in
  private helper fns.
- **Integration tests** in `<crate>/tests/`.
- `tests/conformance.rs` is a 10-line aggregator folding suites in via `#[path = ...] mod ...`. Add a
  suite there, not a new top-level file.
- `crates/cli/tests/process_contract.rs` is the only suite that observes a finished process, via
  `env!("CARGO_BIN_EXE_rulery")`. Process-boundary regressions belong there.
- Tests run with `Command::env_clear()` so an ambient `CI` cannot leak into a result.
- Names are `<subject>_<behavior>` in snake*case: \*\*no `test*`prefix**, no`Type::method` nesting.

### The traceability constraint on test names

`xtask/src/conformance.rs:413-443` (`collect_test_names`) builds the set of verification tests by
scanning source for **literal `fn <name>(` lines**. The specification's traceability tables name
verification tests by source identifier, so this set is what `check_requirements` validates
against.

This is a hard architectural constraint: a test framework that generates test bodies via a proc
macro produces names the traceability tables cannot reference. It is the primary reason `rstest` was
declined, independent of style preference.

## Snapshot and property test conventions

- **Snapshots are reviewed, not auto-accepted.** `insta` writes `.snap.new`; the content is read and
  checked against what it replaces before being accepted. A snapshot nothing reviewed is worse than
  a literal, because it looks like a check.
- **Structural assertions accompany snapshots.** A snapshot pins bytes without saying what makes them
  correct, so byte shape (trailing newline count, required section headings) is asserted separately.
- **Exhaustive enumeration derives from one list.** `Truth::ALL` and `HashDomain::ALL` live on the
  types themselves, and strategies and index ranges read from them. Six hand-maintained copies of two
  enums' variants is what this replaced — with the copies, adding a variant left the build green and
  coverage at zero.
- **Property tests state what is false.** The truth laws record counterexamples for absorption and
  both distributivity directions, because those failures are the algebra, not defects.

## Fixture integrity

`examples/tool-library/` is normative and pinned. Editing its files or lock requires regenerating the
frozen integrity hash or the conformance gate fails. The content digests are pinned as constants in
`tests/conformance/tool_library.rs`; `IntegrityCalculator::bundle_frame_bytes` is public at
`crates/store/src/integrity.rs:23` for exactly this purpose.

Before adding analysis or diff assertions, note that a rule added to the normative fixture moves
**every** partition cell count, coverage ratio, and frozen hash. Author a separate fixture instead.

## Tooling that can silently change committed bytes

The pre-commit hook at `~/.config/git/hooks/pre-commit` runs `prettier --write` over staged YAML and
Markdown, then `git add`s the result. It exits 0 and prints a green check, so nothing signals that
byte-exact artifacts were rewritten. Paths that must not be reformatted go in a repo-local
`.prettierignore` — see the file for the two fixture directories currently listed and why.
