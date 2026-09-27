# Design: xtask Complex Repository Automation

## Goal

Add a non-publishable `xtask` crate that safely reconciles the approved Rulery workspace and
validates the normative specification through repeatable complex workflows.

## Approved Approach

Use the approved **Declarative Workspace Model** approach: one typed model drives bootstrap,
architecture validation, and specification conformance so generation and verification cannot
drift apart.

## Context Map

### Files to Modify

| File                            | Purpose                             | Change                                          |
| ------------------------------- | ----------------------------------- | ----------------------------------------------- |
| `Cargo.toml`                    | Root package and workspace manifest | Add workspace metadata and `xtask` member       |
| `.cargo/config.toml`            | Cargo aliases                       | Add `cargo xtask`                               |
| `xtask/Cargo.toml`              | Automation crate manifest           | Declare non-publishable dependencies            |
| `xtask/src/main.rs`             | Binary entry point                  | Parse CLI and call library dispatch             |
| `xtask/src/lib.rs`              | Workflow dispatch                   | Route typed commands                            |
| `xtask/src/model.rs`            | Declarative workspace authority     | Define crates, targets, and dependency edges    |
| `xtask/src/fs.rs`               | Filesystem boundary                 | Define safe read and atomic-write port          |
| `xtask/src/process.rs`          | Process boundary                    | Define command runner and xshell adapter        |
| `xtask/src/bootstrap/mod.rs`    | Workspace reconciliation            | Build and apply ordered changes                 |
| `xtask/src/conformance/mod.rs`  | Specification checks                | Run Markdown, data, registry, and hash checks   |
| `xtask/src/architecture/mod.rs` | Dependency checks                   | Validate members, edges, and cycles             |
| `xtask/src/verify/mod.rs`       | Aggregate workflow                  | Run all repository gates in fail-fast order     |
| `xtask/tests/bootstrap.rs`      | Bootstrap behavior                  | Verify modes, idempotence, and overwrite safety |
| `xtask/tests/conformance.rs`    | Specification behavior              | Verify malformed fixture detection              |
| `xtask/tests/architecture.rs`   | Dependency behavior                 | Verify missing members, edges, and cycles       |
| `xtask/tests/verify.rs`         | Aggregate behavior                  | Verify ordering and fail-fast behavior          |
| `tests/workspace_scaffold.rs`   | Root topology contract              | Require approved crates and `xtask`             |

### Dependencies

- No Rulery library or binary crate depends on `xtask`.
- `xtask` reads Cargo metadata, parses tracked documents, and starts local development commands.
- Generated scratch data is restricted to `.ctx/_WORKING_DIR/xtask-conformance/`.

### Reference Pattern

`/Users/joe/dev/minibox/xtask` demonstrates a thin entry point, xshell process execution, modular
commands, and architecture checks. Rulery uses a smaller declarative model rather than copying its
large command dispatcher.

### Risk

- [ ] Public Rulery API change: no.
- [ ] Persisted runtime format change: no.
- [x] Filesystem mutation: guarded by generated ownership markers and atomic writes.
- [x] Process execution: isolated behind `CommandRunner` and covered by recording fakes.
- [x] New development dependencies: listed under Tech Stack.

## Crate Ownership

- **Owner crate:** `xtask` - repository automation only.
- **Affected crates:** none import `xtask`; the workspace model describes them as data.
- **Publish policy:** `publish = false`.

## Command Surface

```text
cargo xtask bootstrap
cargo xtask bootstrap --check
cargo xtask bootstrap --dry-run
cargo xtask conformance
cargo xtask architecture
cargo xtask verify
```

- `bootstrap` computes and atomically applies workspace reconciliation.
- `bootstrap --check` computes the same changes, writes nothing, and fails when changes exist.
- `bootstrap --dry-run` computes the same changes, writes nothing, and prints them in order.
- `conformance` validates `docs/specification.md` and its embedded contracts.
- `architecture` validates workspace membership, internal dependency edges, and acyclicity.
- `verify` runs bootstrap check, conformance, architecture, formatting check, Clippy, nextest, and
  rustdoc, stopping at the first failure.

Simple single-command Cargo wrappers are intentionally excluded.

## Public API

### CLI

```rust
#[derive(Clone, Debug, Eq, PartialEq, clap::Parser)]
pub struct Cli {
    command: Command,
}

#[derive(Clone, Debug, Eq, PartialEq, clap::Subcommand)]
pub enum Command {
    Bootstrap(BootstrapArgs),
    Conformance,
    Architecture,
    Verify,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, clap::Args)]
pub struct BootstrapArgs {
    check: bool,
    dry_run: bool,
}
```

`check` and `dry_run` are mutually exclusive.

### Workspace Model

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceModel {
    members: Vec<CrateSpec>,
    dependency_rules: std::collections::BTreeMap<String, DependencyRule>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CrateSpec {
    directory: std::path::PathBuf,
    package_name: String,
    target: TargetKind,
    description: String,
    internal_dependencies: std::collections::BTreeSet<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetKind {
    Library,
    Binary,
    ProcMacro,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DependencyRule {
    crate_name: String,
    allowed_dependencies: std::collections::BTreeSet<String>,
}

pub fn rulery_workspace_model() -> WorkspaceModel;
```

The model includes the root `rulery` facade, every approved crate under `crates/`, and `xtask`.
`rulery-lsp` is excluded until its deferred phase is approved.

### Reconciliation

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReconcileMode {
    Apply,
    Check,
    DryRun,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangeSet {
    changes: Vec<Change>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Change {
    Create { path: std::path::PathBuf, contents: String },
    ReplaceGenerated { path: std::path::PathBuf, contents: String },
    EditRootManifest { contents: String },
}

pub fn plan_bootstrap<F>(
    root: &std::path::Path,
    model: &WorkspaceModel,
    fs: &F,
) -> Result<ChangeSet, BootstrapError>
where
    F: WorkspaceFileSystem;

pub fn reconcile<F>(
    root: &std::path::Path,
    changes: &ChangeSet,
    mode: ReconcileMode,
    fs: &F,
) -> Result<(), BootstrapError>
where
    F: WorkspaceFileSystem;
```

Changes are sorted by normalized relative path, then by change kind. Applying a change set twice
must produce an empty second plan.

### Filesystem And Process Ports

```rust
pub trait WorkspaceFileSystem {
    type Error: std::error::Error + Send + Sync + 'static;

    fn read(&self, path: &std::path::Path) -> Result<Option<Vec<u8>>, Self::Error>;
    fn write_atomic(&self, path: &std::path::Path, bytes: &[u8]) -> Result<(), Self::Error>;
    fn create_dir_all(&self, path: &std::path::Path) -> Result<(), Self::Error>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandSpec {
    program: String,
    arguments: Vec<String>,
}

pub trait CommandRunner {
    type Error: std::error::Error + Send + Sync + 'static;

    fn run(&self, root: &std::path::Path, command: &CommandSpec) -> Result<(), Self::Error>;
}
```

`HostFileSystem` and `XshellRunner` are the production adapters. Tests use a temporary filesystem
and recording process runner.

### Conformance

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConformanceReport {
    checks: Vec<CheckResult>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckResult {
    name: String,
    status: CheckStatus,
    detail: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckStatus {
    Passed,
    Failed,
}

pub fn check_specification<R>(
    root: &std::path::Path,
    runner: &R,
) -> Result<ConformanceReport, ConformanceError>
where
    R: CommandRunner;
```

The conformance workflow performs these checks in order:

1. Markdown structure, table of contents links, and forbidden incomplete markers.
2. YAML and JSON fenced-block parsing.
3. Rust fenced-block formatting through `rustfmt --check` in scratch files.
4. Exact diagnostic-code registry count and uniqueness.
5. Exact seven-envelope names and schema tags.
6. BLAKE3 framing vector recomputation.

All checks run so the report contains every failure. Process startup or unreadable input returns
`ConformanceError` immediately.

### Architecture And Verify

```rust
pub fn validate_architecture(
    metadata: &cargo_metadata::Metadata,
    model: &WorkspaceModel,
) -> Result<(), ArchitectureError>;

pub fn verify<R, F>(
    root: &std::path::Path,
    model: &WorkspaceModel,
    runner: &R,
    fs: &F,
) -> Result<(), VerifyError>
where
    R: CommandRunner,
    F: WorkspaceFileSystem;
```

`verify` executes this exact fail-fast sequence:

1. `bootstrap --check` in-process.
2. `conformance` in-process.
3. `architecture` from `cargo metadata --format-version 1`.
4. `cargo fmt --all --check`.
5. `cargo clippy --workspace -- -D warnings`.
6. `cargo nextest run --workspace`.
7. `cargo doc --workspace --no-deps`.

## Reconciliation Safety

- New crate manifests and entry points start with `# @generated by cargo xtask bootstrap`.
- Generated files may be replaced atomically when their model-derived bytes differ.
- Existing unmarked files are never replaced or merged; bootstrap returns `UnmanagedConflict`.
- Root `Cargo.toml` is edited structurally with `toml_edit`, preserving unrelated package content.
- Root `src/lib.rs` is created only when absent; an existing unmarked file is preserved.
- `Check` and `DryRun` never write.
- `DryRun` prints the ordered `ChangeSet`; `Check` is silent unless drift exists.
- Writes use a temporary sibling file, flush it, and atomically rename it.
- No bootstrap operation deletes files or directories.

## Data Flow

1. CLI input selects a typed command and reconciliation mode.
2. `rulery_workspace_model` supplies one authoritative crate and dependency graph.
3. Bootstrap compares model-derived files with filesystem state and emits a `ChangeSet`.
4. Reconciliation applies, checks, or prints that same change set.
5. Conformance parses the specification and recomputes embedded invariants.
6. Architecture compares Cargo metadata with the same workspace model.
7. Verify composes the three workflows with Cargo quality gates.

## Tech Stack

- Rust 2024.
- `clap` for typed command parsing.
- `xshell` for child-process execution.
- `cargo_metadata` for workspace graph inspection.
- `toml_edit` for structure-preserving root manifest edits.
- `pulldown-cmark` for Markdown fenced blocks and links.
- `serde_json` and `serde_yaml_ng` for embedded data validation.
- `blake3` and `hex` for conformance vectors.
- `thiserror` for typed errors.
- `tempfile` for test and atomic-write support.

## Testing

- Bootstrap tests cover missing files, generated replacement, unmanaged conflict, all three modes,
  deterministic ordering, atomic failure propagation, and idempotence.
- Conformance tests cover each check with one malformed fixture and one valid fixture.
- Architecture tests cover missing members, forbidden edges, allowed edges, and cycles.
- Verify tests assert exact process ordering and fail-fast behavior.
- Root scaffold tests require every approved package plus `xtask`.

## Integration Points

- `.cargo/config.toml` defines `xtask = "run --package xtask --"`.
- Root `Cargo.toml` includes `members = ["crates/*", "xtask"]`.
- `xtask` inherits workspace edition, Rust version, license, repository, and lint settings.
- No feature flag is required.

## Out Of Scope

- Release, publish, version-bump, deployment, coverage, or destructive cleanup automation.
- Domain-specific source generation or rulebook evaluation.
- Network access, credentials, remote registries, or CI-provider APIs.
- Taskit integration.
- Simple aliases that only wrap one Cargo command outside the aggregate `verify` workflow.

## Risk

- [ ] Breaking Rulery API changes: no.
- [x] New external dependencies: yes, confined to the non-publishable `xtask` crate.
- [ ] Feature flag required: no.
- [x] Filesystem writes: atomic, non-destructive, and restricted by ownership markers.
- [x] External processes: isolated behind `CommandRunner`.

## Approval Gate

No xtask implementation begins until this revised design is explicitly approved.
