# Design: CLI Application Integration

## Goal

Make every declared Rulery CLI command executable through one reusable application boundary while preserving identical semantics for future MCP and HTTP adapters.

## Approved Approach

Use a shared production application adapter in the root `rulery` crate and keep `rulery-cli` as a thin argument, rendering, stream, and exit-code transport.

## Delivery Boundary

The CLI cannot be wired correctly before the authored-package pipeline is complete: bundle parsing currently omits non-manifest semantics, compilation does not produce executable decisions, and no package-level evaluator exists. Delivery therefore proceeds in three ordered slices:

1. Complete the source-to-compiled-package and package-to-trace services in their owning crates.
2. Compose those services behind the root application boundary.
3. Delegate every CLI command to that boundary and add subprocess conformance tests.

## Crate Ownership

- **Core owners**: `rulery-syntax`, `rulery-compiler`, `rulery-ir`, `rulery-engine`, `rulery-analysis`, and `rulery-scenarios` retain ownership of parsing, compilation, executable rule data, evaluation, analysis, and scenarios.
- **Application owner**: `rulery` owns use-case composition and production adapters.
- **Transport owner**: `rulery-cli` owns command mapping, filesystem presentation operations, rendering selection, streams, and exit policy.
- **Affected support crates**: `rulery-emit` and `rulery-diagnostics` expose typed rendering and diagnostic inspection required by the application and transport layers.

No new workspace crate is introduced.

## Public API

### Application Trait

```rust
pub trait ApplicationService: Send + Sync {
    fn compile_package(
        &self,
        root: &PackagePath,
        lock_mode: LockMode,
    ) -> Result<CompileWorkflowOutput, ApplicationError>;

    fn evaluate_at(
        &self,
        package: &CompiledPackage,
        decision: &DecisionId,
        facts: &CaseFacts,
        at: UtcInstant,
    ) -> Result<DecisionTrace, ApplicationError>;

    fn analyze(
        &self,
        package: &CompiledPackage,
        options: &AnalysisOptions,
    ) -> Result<AnalysisReport, ApplicationError>;

    fn diff(
        &self,
        before: &CompiledPackage,
        after: &CompiledPackage,
        decision: Option<&DecisionId>,
        options: &AnalysisOptions,
    ) -> Result<AnalysisReport, ApplicationError>;

    fn run_scenarios(
        &self,
        package: &CompiledPackage,
        scenarios: &[CompiledScenario],
    ) -> Result<Vec<ScenarioResult>, ApplicationError>;

    fn write_lock(
        &self,
        root: &PackagePath,
        lock: &RulebookLock,
    ) -> Result<(), ApplicationError>;
}
```

### Production Application

```rust
#[derive(Clone, Debug, Default)]
pub struct ProductionApplication;

impl ProductionApplication {
    pub fn new() -> Self;
}
```

### Application Error

```rust
#[derive(Debug, thiserror::Error)]
pub enum ApplicationError {
    InvalidInvocation(String),
    Diagnostics(DiagnosticReport),
    AnalysisIncomplete(AnalysisReport),
    RuntimeConflict { evidence: Vec<String> },
    Io(std::io::Error),
    Internal(String),
}
```

### Compiler Port

```rust
pub trait PackageCompiler: Send + Sync {
    fn compile(
        &self,
        input: PackageCompilerInput<'_>,
    ) -> Result<CompilationOutput, CompilerError>;
}
```

### Evaluator Port

```rust
pub trait PolicyEvaluator: Send + Sync {
    fn evaluate(
        &self,
        package: &CompiledPackage,
        decision: &DecisionId,
        facts: &CaseFacts,
        at: UtcInstant,
    ) -> Result<DecisionTrace, EvaluationError>;
}
```

### CLI Boundary

```rust
pub trait CommandApplication {
    fn execute(&self, command: &Command) -> Result<CommandExecution, CommandError>;
}

pub fn run_command<A: CommandApplication>(
    command: &Command,
    application: &A,
) -> CommandOutput;

pub fn write_command_output<W, E>(
    output: &CommandOutput,
    stdout: &mut W,
    stderr: &mut E,
) -> Result<(), CommandError>
where
    W: std::io::Write,
    E: std::io::Write;
```

### Required Inspection APIs

```rust
impl DiagnosticReport {
    pub fn diagnostics(&self) -> &[Diagnostic];
}

impl CompiledRule {
    pub const fn specificity(&self) -> u32;
    pub fn effect(&self) -> &CompiledEffect;
}
```

## Command Mapping

| Command   | Application operation                               | Primary output                      |
| --------- | --------------------------------------------------- | ----------------------------------- |
| `init`    | CLI-owned atomic package scaffold                   | Created path                        |
| `fmt`     | CLI-owned canonical source formatting               | Human paths or empty check output   |
| `check`   | `compile_package`                                   | Diagnostics report                  |
| `analyze` | `compile_package`, then `analyze`                   | Analysis report or SARIF            |
| `test`    | `compile_package`, then `run_scenarios`             | Scenario result envelopes           |
| `explain` | `compile_package`, facts decode, then `evaluate_at` | Decision trace or human explanation |
| `diff`    | Compile both roots, then `diff`                     | Analysis report                     |
| `render`  | Compile, then typed renderer                        | JSON, Markdown, or decision table   |
| `lock`    | Compile in update mode, then `write_lock`           | Written lock path                   |

## Data Flow

1. The CLI converts parsed arguments into typed command inputs without interpreting rule semantics.
2. `ProductionApplication` loads and assembles local package sources through `FilesystemPackageStore` and `YamlSourceParser`.
3. `PackageCompiler` resolves, type-checks, normalizes, and lowers the complete source bundle into `CompiledPackage` and compiled scenarios.
4. `PolicyEvaluator`, analysis services, and scenario services consume only compiled typed values.
5. `rulery-emit` converts typed artifacts into human text, JSON envelopes, Markdown, decision tables, or SARIF.
6. The CLI writes each output exactly once and maps typed errors to stable exit codes.

## Hexagonal Boundaries

- **Ports**: `SourceParser`, `PackageStore`, `PackageCompiler`, `PolicyEvaluator`, `ApplicationService`, and `CommandApplication`.
- **Core adapters**: YAML parsing, filesystem package storage, compiler pipeline, evaluator, analyzer, and scenario runner implementations in their owning crates.
- **Application adapter**: `ProductionApplication` in `rulery` composes core adapters.
- **Transport adapter**: the `rulery-cli` application adapter maps commands and typed results without owning policy semantics.

## Compatibility

- Existing versioned wire envelopes remain the integration contract.
- Existing command names, arguments, output formats, and exit-code values remain stable.
- `FacadePorts` may be deprecated after `ApplicationService` is proven; it is not removed in this change.
- Public additions are additive for the pre-release `0.1.0` API.

## Testing

- Unit tests cover each new compiler and evaluator boundary with in-memory inputs.
- Existing conformance fixtures exercise the real authored-package pipeline rather than the predetermined helper.
- CLI subprocess tests cover every command, supported output format, exit status, stdout/stderr separation, `--frozen`, `--deny-warnings`, fixed evaluation time, and non-writing checks.
- Architecture tests continue to reject dependency cycles.

## Out of Scope

- MCP, HTTP, gRPC, WASI, and language-specific SDK adapters.
- Remote package registries or network imports.
- Background services, caching, or daemon lifecycle.
- New rule-language semantics beyond the v0.1 specification.

## Risk

- [ ] Breaking API changes: no; retain existing facade and wire contracts during migration.
- [ ] Serialization changes: yes; executable rule effects must be represented in the existing compiled-package v1 payload before release, with conformance fixtures updated atomically.
- [ ] New external dependency: no required production dependency beyond promoting an existing YAML dependency where needed.
- [ ] Feature flag required: no.
- [ ] Cross-crate risk: high; implementation must follow dependency order and pass architecture checks after each slice.
- [ ] CLI compatibility risk: high; subprocess tests must lock output and exit behavior before replacing the stub entry point.
