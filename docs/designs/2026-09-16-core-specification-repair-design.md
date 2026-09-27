# Design: Core Specification Repair

## Goal

Produce `docs/specification.md` as a coherent, compile-ready Rulery v0.1 blueprint whose semantics, public APIs, wire formats, examples, crate boundaries, and acceptance gates agree. Treat `local/spec.local.md` as descriptive source material rather than a versioned artifact.

## Approved Approach

Use the approved **Contract-First Rebuild** approach: define normative semantics and canonical contracts first, then derive crate ownership, Rust APIs, examples, conformance fixtures, and delivery phases from those contracts.

## Context Map

### Files to Modify

| File                                                          | Purpose                          | Changes Needed                                                                                                         |
| ------------------------------------------------------------- | -------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `docs/designs/2026-09-16-core-specification-repair-design.md` | Approved architecture contract   | Record the specification-repair design before planning                                                                 |
| `docs/specification.md`                                       | Tracked normative v0.1 blueprint | Rebuild the approved semantics, contracts, APIs, examples, and acceptance gates during the planned documentation phase |

### Reference-Only Files

| File                       | Relationship                                                                                        |
| -------------------------- | --------------------------------------------------------------------------------------------------- |
| `local/spec.local.md`      | Ignored descriptive source material; it is read for intent but not modified or treated as normative |
| `Cargo.toml`               | Current package is a minimal Rust 2024 binary with no dependencies                                  |
| `src/main.rs`              | Current entry point is only a `Hello, world!` placeholder; no architecture exists to preserve       |
| `.gitignore`               | Excludes `local/`, so the source specification is not a versioned project artifact                  |
| `/Users/joe/dev/CLAUDE.md` | Supplies workspace, Rust, testing, and scope conventions                                            |

### Existing Dependencies

There are no existing crate or module dependency edges. The current repository contains one package and one function.

### Existing Test Coverage

There are no tests. The future plan must add compile validation for documentation snippets, format fixtures, semantic truth tables, fixed hash vectors, and end-to-end conformance cases.

### Reference Patterns

No analogous implementation exists in this repository. The repaired specification is the authority for the initial workspace architecture, subject to the Rust API Guidelines and workspace conventions.

### Risk

- [x] Public API design: the specification defines the initial public API, so incorrect commitments would become semver constraints after v0.1.
- [x] Serialization design: authored YAML and generated JSON shapes may change because no released compatibility promise exists.
- [x] Cross-crate design: the blueprint defines all workspace boundaries and must prohibit cycles mechanically.
- [x] Documentation provenance: `local/spec.local.md` is ignored, so the repaired version must move to a tracked documentation path during implementation planning.
- [ ] Existing consumers: none exist in the current repository.

## Specification Authority

The rebuilt specification uses this authority order:

1. Normative semantics.
2. Canonical authored and generated data contracts.
3. Crate ownership and dependency rules.
4. Compile-ready Rust APIs.
5. Conformance fixtures and fixed expected outputs.
6. Delivery phases.
7. Explicitly non-normative deferred material.

When prose, Rust, schemas, and examples disagree, normative semantics control and the disagreement is a specification defect. RFC 2119-style `MUST`, `SHOULD`, and `MAY` language distinguishes requirements from guidance.

## Workspace Shape

```text
rulery/
|-- Cargo.toml
|-- src/                         # public `rulery` facade library
|-- crates/
|   |-- contracts/
|   |-- diagnostics/
|   |-- syntax/
|   |-- vocabulary/
|   |-- ir/
|   |-- compiler/
|   |-- engine/
|   |-- analysis/
|   |-- scenarios/
|   |-- emit/
|   |-- store/
|   |-- cli/
|   |-- macros/
|   `-- lsp/                    # deferred
|-- examples/
|   `-- tool-library/
|-- docs/
|   |-- adr/
|   |-- designs/
|   |-- language/
|   `-- semantics/
`-- tests/
    `-- conformance/
```

The root package becomes the public `rulery` library facade. The executable is owned exclusively by `rulery-cli`.

## Crate Ownership

| Crate                | Single responsibility                                                                                                                                   |
| -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `rulery`             | Curated public facade, embedding workflow, and hygienic declarative macros                                                                              |
| `rulery-contracts`   | Stable evaluator-neutral IDs, facts, values, outcomes, time inputs, raw source bundles, source maps, integrity metadata, and shared artifact identities |
| `rulery-diagnostics` | Diagnostic identities, registry, reports, labels, evidence, fixes, and versioned diagnostic envelopes                                                   |
| `rulery-syntax`      | Authored DTOs, source documents, YAML decoding, source-map construction, and spanned source ASTs                                                        |
| `rulery-vocabulary`  | Resolved schemas, operational terms, type relationships, and value validation                                                                           |
| `rulery-ir`          | Compiler-created resolved expressions, rules, decisions, actions, and package artifacts                                                                 |
| `rulery-compiler`    | Source validation, name resolution, type checking, normalization, lowering, compilation provenance, and canonical package hashing                       |
| `rulery-engine`      | Four-valued expression evaluation, uncertainty relevance, precedence, outcomes, and complete deterministic traces                                       |
| `rulery-analysis`    | Reachability, overlap, conflict, coverage, witness generation, and semantic diff analysis                                                               |
| `rulery-scenarios`   | Executable scenario models, scenario compilation, assertions, and results                                                                               |
| `rulery-emit`        | Human, JSON, Markdown, decision-table, and SARIF rendering                                                                                              |
| `rulery-store`       | Filesystem package discovery, raw source loading, import resolution, lockfiles, and integrity verification                                              |
| `rulery-cli`         | Command parsing, use-case orchestration, output selection, and exit policy                                                                              |
| `rulery-macros`      | Procedural macros and compile-time literal validation only                                                                                              |
| `rulery-lsp`         | Deferred editor adapter; excluded from v0.1                                                                                                             |

Normalization and lowering belong to `rulery-compiler`; `rulery-ir` owns data and checked construction, not compilation passes. Source decoding belongs to `rulery-syntax`; scenario execution belongs to `rulery-scenarios`.

## Dependency Direction

Allowed direct dependencies are:

```text
rulery-contracts
  <- rulery-diagnostics
  <- rulery-vocabulary
  <- rulery-syntax

rulery-contracts + rulery-vocabulary
  <- rulery-ir

rulery-contracts + rulery-diagnostics + rulery-syntax
+ rulery-vocabulary + rulery-ir
  <- rulery-compiler

rulery-contracts + rulery-ir
  <- rulery-engine

rulery-contracts + rulery-diagnostics + rulery-vocabulary
+ rulery-ir + rulery-engine
  <- rulery-analysis

rulery-contracts + rulery-diagnostics + rulery-syntax
+ rulery-ir + rulery-engine
  <- rulery-scenarios

rulery-contracts
  <- rulery-store

rulery-contracts + rulery-diagnostics + rulery-ir
+ rulery-engine + rulery-analysis + rulery-scenarios
  <- rulery-emit

stable library crates + optional rulery-macros
  <- rulery facade
  <- rulery-cli composition root
```

`rulery-store` returns raw source bundles and does not parse or compile them. The facade and CLI compose store, syntax, and compiler services. `rulery-engine` consumes a self-sufficient compiled package and therefore does not depend directly on vocabulary implementation details.

`rulery-macros` must not depend on the facade. Procedural expansions resolve the facade with `proc_macro_crate`; declarative expansions use `$crate` and a hidden facade module.

## Normative Semantic Model

### Four-Valued Truth

`Truth` remains `True`, `False`, `Unknown`, or `Invalid`. The repaired specification must provide complete tables for logical operators and every predicate category.

- `Unknown` means required evidence is absent or indeterminate.
- `Invalid` means evidence exists but violates its declared type or operator contract.
- Absence, explicit null, and malformed values are distinct states.
- Floating-point values are prohibited from policy comparison and hashing.
- Decimal values use one validated, exact, canonical representation.

### Uncertainty Relevance

An unresolved rule is decision-relevant only when its maximum possible semantic precedence could change the selected outcome. If no unresolved rule can alter the result, the decisive candidate remains valid and unresolved facts are retained only in the trace.

Missing and invalid facts use independent compiled strategies. Callers cannot override those strategies through runtime booleans.

```rust
pub enum MissingFactStrategy {
    PreserveUnknown,
    ClosedWorldFalse,
    RequestInformation,
    Escalate { destination: EscalationId },
}

pub enum InvalidFactStrategy {
    RejectEvaluation,
    PreserveInvalid,
    Escalate { destination: EscalationId },
}

pub struct DecisionSemantics {
    missing_facts: MissingFactStrategy,
    invalid_facts: InvalidFactStrategy,
    precedence: PrecedenceModel,
    time: TimeSemantics,
}
```

Outcome templates make required reasons, escalation destinations, and requested facts unrepresentable as empty or mismatched states.

```rust
pub struct Reasons {
    values: Vec<Reason>,
}

pub struct RequiredFacts {
    values: BTreeSet<FactPath>,
}

pub enum OutcomeTemplate {
    Approve {
        reasons: Reasons,
        actions: Vec<ActionInvocation>,
    },
    Deny {
        reasons: Reasons,
        actions: Vec<ActionInvocation>,
    },
    Escalate {
        destination: EscalationId,
        reasons: Reasons,
        actions: Vec<ActionInvocation>,
    },
    RequestInformation {
        required_facts: RequiredFacts,
        reasons: Reasons,
        actions: Vec<ActionInvocation>,
    },
}
```

### Precedence and Conflict

Semantic precedence excludes rule identity. All components compare in descending order.

```rust
pub enum PrecedenceModel {
    SafetyFirst,
    PriorityFirst,
    Explicit(ExplicitPrecedence),
}

pub struct ExplicitPrecedence {
    outcome_ranks: BTreeMap<OutcomeKind, OutcomeRank>,
    primary_dimension: PrecedenceDimension,
}

pub enum PrecedenceDimension {
    Outcome,
    Priority,
}

pub struct SafetyFirstKey {
    outcome_rank: OutcomeRank,
    priority: RulePriority,
    specificity: Specificity,
    override_rank: OverrideRank,
}

pub struct PriorityFirstKey {
    priority: RulePriority,
    outcome_rank: OutcomeRank,
    specificity: Specificity,
    override_rank: OverrideRank,
}
```

Equal semantic keys with incompatible outcomes produce `UnresolvedConflict`. `QualifiedRuleId` is used only to stabilize trace and diagnostic presentation.

### Time

Clocks provide UTC instants. Decision semantics provide a validated IANA timezone and explicit date-expiry policy. Policy-local dates are obtained by converting the UTC instant through the decision timezone, including daylight-saving transitions.

```rust
pub trait Clock: Send + Sync + 'static {
    fn now_utc(&self) -> UtcInstant;
}

pub trait TimeZoneDatabase: Send + Sync + 'static {
    fn identity(&self) -> TimeZoneDatabaseIdentity;
    fn local_date(
        &self,
        instant: UtcInstant,
        timezone: &PolicyTimeZone,
    ) -> Result<PolicyDate, TimeZoneError>;
}

pub struct UtcInstant {
    unix_nanoseconds: i128,
}

pub struct PolicyTimeZone {
    name: String,
}

pub struct TimeZoneDatabaseIdentity {
    implementation: String,
    version: String,
}

pub struct PolicyDate {
    year: u16,
    month: u8,
    day: u8,
}

pub struct TimeSemantics {
    timezone: PolicyTimeZone,
    expiry: DateExpiryPolicy,
}

pub enum DateExpiryPolicy {
    Inclusive,
    Exclusive,
}
```

## Canonical Contracts

### Source Identity and Spans

`Span` uses a compact copyable key. Human-authored source IDs and paths live in `SourceMap`.

```rust
pub struct SourceKey {
    value: u32,
}

pub struct Span {
    source: SourceKey,
    start: u32,
    end: u32,
}

pub struct SourceMap {
    entries: BTreeMap<SourceKey, SourceFile>,
}

pub struct SourceFile {
    id: SourceId,
    path: SourcePath,
    content: Arc<str>,
}
```

`SourceKey` and `Span` may implement `Copy`; `SourceId`, `SourcePath`, and `SourceFile` do not. Every source condition and effect has a span before semantic compilation.

### Authored Package

The authored package has one assembly contract:

- `rulery.yaml`: metadata, semantics, imports, and decision declarations.
- `vocabulary.yaml`: roots, named types, and operational terms.
- `actions.yaml`: action declarations.
- `rules/*.yaml`: rules grouped by decision.
- `scenarios/*.yaml`: executable examples.

Primitive Boolean, integer, decimal, text, date, date-time, and duration types are language built-ins. Enums, records, and lists are named vocabulary declarations.

The YAML-only DTO layer is private to `rulery-syntax`. The public source AST is format-neutral and replaces multi-option predicates with one operator enum.

```rust
pub struct SourcePredicate {
    fact: UnresolvedFactPath,
    operator: SourceOperator,
    span: Span,
}

pub enum SourceOperator {
    Equal(SourceOperand),
    NotEqual(SourceOperand),
    Before(SourceOperand),
    OnOrAfter(SourceOperand),
    GreaterThan(SourceOperand),
    LessThan(SourceOperand),
    IsAbsent,
    IsPresent,
}

pub struct ParsedPackage {
    package: SourcePackage,
    scenarios: Vec<SourceScenario>,
    source_map: SourceMap,
}

pub trait SourceParser: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    fn parse_bundle(&self, bundle: &SourceBundle) -> Result<ParsedPackage, Self::Error>;
}
```

### Package Assembly

The root facade owns recursive package assembly because it composes the store and parser ports. Imports are traversed depth-first in ascending `PackageId` order. Each import is resolved relative to its declaring package, validated against its declared ID and version requirement, and pinned by content hash in the lockfile.

Rulery v0.1 permits one resolved version per `PackageId` in a package graph. A cycle, duplicate ID with different content, unresolved version, or lock mismatch produces a diagnostic and prevents compilation. Lockfiles contain the complete transitive import closure.

Assembly always receives an explicit lock mode:

```rust
pub enum LockMode {
    Update,
    Frozen,
}
```

- `Update` accepts a missing or stale lockfile, resolves the complete local import graph deterministically, and returns a replacement `RulebookLock`. Assembly does not write it; only the `rulery lock` use case calls `PackageStore::write_lock`.
- `Frozen` requires a lockfile. The root package identity, language version, every transitive import identity, resolved version, source location, and content hash must match exactly. Missing, stale, additional, or absent entries are errors and no replacement is produced.
- A package with no imports still has a lockfile containing its root identity, language version, root content hash, and an empty import set. Therefore `Frozen` never has a special lockfile-absence exception.
- `check`, `test`, `explain`, `analyze`, `render`, and `diff` use `Update` unless `--frozen` is passed. They never infer behavior from a CI environment variable and never write a lockfile. CI examples and the recommended CI workflow always pass `--frozen` explicitly.
- `rulery lock` always uses `Update` and atomically writes the returned lock only after parsing, resolution, compilation, and integrity validation succeed.

Each parsed package begins with package-local source keys. Assembly assigns globally unique keys in package-ID and source-path order, rewrites every span, and emits one merged `SourceMap` before compilation.

```rust
pub struct PackageIntegritySet {
    root: SourceIntegrity,
    imports: BTreeMap<PackageId, SourceIntegrity>,
}

pub struct PackageAssembly {
    input: CompilationInput,
    source_scenarios: Vec<SourceScenario>,
    diagnostics: DiagnosticReport,
    proposed_lock: Option<RulebookLock>,
}

pub struct PackageAssembler<S, P> {
    store: S,
    parser: P,
}

pub trait PackageAssemblyService: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    fn assemble(
        &self,
        root: &PackagePath,
        lock_mode: LockMode,
    ) -> Result<PackageAssembly, Self::Error>;
}
```

`PackageAssembler<S, P>` implements `PackageAssemblyService` when `S: PackageStore` and `P: SourceParser`. Its returned `CompilationInput` contains the remapped parsed packages and a `PackageIntegritySet` covering the root and every transitive import.

### Compiled Package

The compiled artifact retains everything required by evaluation, analysis, and reproducibility.

```rust
pub struct CompiledPackage {
    metadata: PackageMetadata,
    vocabulary: ResolvedVocabulary,
    decisions: BTreeMap<DecisionId, CompiledDecision>,
    actions: BTreeMap<ActionId, Action>,
    source_map: SourceMap,
    integrity: PackageIntegritySet,
    compiler: CompilerIdentity,
    language_version: LanguageVersion,
    content_hash: ContentHash,
}

pub struct CompilationOutput {
    package: Option<CompiledPackage>,
    diagnostics: DiagnosticReport,
}

pub struct CompilationInput {
    root: ParsedPackage,
    imports: BTreeMap<PackageId, ParsedPackage>,
    integrity: PackageIntegritySet,
}

pub trait PolicyCompiler: Send + Sync {
    fn compile(&self, input: &CompilationInput) -> CompilationOutput;
}
```

The compiler hashes a canonical artifact body that excludes its own `content_hash` field.

### Canonical Hashing

Generic `of_json<T>` hashing is removed. Each artifact body is encoded as RFC 8785 JSON Canonicalization Scheme bytes, then hashed with BLAKE3-256 under an exact domain string.

```rust
pub enum HashDomain {
    CompiledPackageV1,
    CaseFactsV1,
    DecisionTraceV1,
    EvaluationV1,
}

pub fn hash_parts<'a>(
    domain: HashDomain,
    parts: impl IntoIterator<Item = &'a [u8]>,
) -> ContentHash;
```

The exact domain bytes are:

| Domain              | UTF-8 bytes                  |
| ------------------- | ---------------------------- |
| `CompiledPackageV1` | `rulery.compiled-package.v1` |
| `CaseFactsV1`       | `rulery.case-facts.v1`       |
| `DecisionTraceV1`   | `rulery.decision-trace.v1`   |
| `EvaluationV1`      | `rulery.evaluation.v1`       |

The framing is an unsigned 64-bit big-endian domain length, domain bytes, then an unsigned 64-bit big-endian length and bytes for each part. Every artifact specification includes fixed input and BLAKE3-256 output vectors. `serde_jcs` supplies canonical JSON encoding; ordinary `serde_json::to_vec` is not a hashing primitive.

Artifact inputs and part order are fixed:

| Hash                | Ordered parts                                                                                                                                              |
| ------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Compiled package    | JCS bytes of `CompiledPackageV1` with `content_hash` omitted                                                                                               |
| Case facts          | JCS bytes of `CaseFactsV1`                                                                                                                                 |
| Decision trace      | JCS bytes of `DecisionTraceV1` with `trace_hash` omitted                                                                                                   |
| Evaluation identity | raw 32-byte package hash, decision ID UTF-8 bytes, raw 32-byte facts hash, signed 128-bit big-endian UTC nanoseconds, timezone database identity JCS bytes |

Only versioned payloads are hashed; envelope tags are never included because the hash domain already identifies the artifact and version. `CompiledPackageV1` includes metadata, resolved vocabulary, decisions, actions, source catalog, compiler identity, language version, and import integrity in that order conceptually; JCS determines object-key order. `DecisionTraceV1` includes all trace fields except `trace_hash`. No display-only prose or renderer output enters a semantic hash.

Canonical scalar encodings are:

| Value                        | Canonical JSON representation                                                                                     |
| ---------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| Boolean and null             | JSON `true`, `false`, and `null`                                                                                  |
| Signed and unsigned integers | Base-10 strings with no leading plus sign or leading zeroes; zero is `"0"`                                        |
| Decimal                      | Canonical decimal string using the grammar after this table                                                       |
| Date                         | Zero-padded Gregorian `YYYY-MM-DD` string with year in `0001` through `9999`                                      |
| UTC instant                  | Signed base-10 Unix nanoseconds string                                                                            |
| Displayed date-time          | UTC RFC 3339 string with year `0001` through `9999`, exactly nine fractional digits, and `Z` suffix               |
| Duration                     | Signed base-10 nanoseconds string                                                                                 |
| UUID                         | Lowercase hyphenated string                                                                                       |
| Stable ID and enum symbol    | One to 128 lowercase ASCII letters, digits, `.`, `_`, or `-`; first and last characters must be letters or digits |
| Content hash                 | `blake3:` followed by exactly 64 lowercase hexadecimal digits                                                     |
| Timezone                     | Validated canonical IANA timezone name                                                                            |

Strings use RFC 8785 escaping. Lists retain semantic order. Maps have string keys and use JCS key ordering. Sets serialize as arrays sorted lexicographically by each element's complete canonical byte encoding. Optional absent fields are omitted; explicit null is serialized only when null is semantically distinct. These rules apply recursively to every hashed payload.

The canonical decimal grammar is:

```text
-?(0|[1-9][0-9]*)(\.[0-9]*[1-9])?
```

Negative zero normalizes to `0`; a zero fractional component is removed entirely. The equivalent stable-ID grammar is `[a-z0-9](?:[a-z0-9._-]{0,126}[a-z0-9])?`.

`ContentHash` stores exactly 32 bytes internally and exposes those bytes only to hashing APIs. Its parser rejects unknown algorithms, uppercase hexadecimal, and lengths other than 64 hexadecimal digits.

### Versioned Envelopes

Persisted and interchange formats use artifact-specific tagged envelopes. Language version and wire schema version remain independent.

```rust
#[serde(tag = "schema", content = "payload")]
pub enum DecisionTraceEnvelope {
    #[serde(rename = "rulery.decision-trace/v1")]
    V1(DecisionTraceV1),
}

#[serde(tag = "schema", content = "payload")]
pub enum DiagnosticReportEnvelope {
    #[serde(rename = "rulery.diagnostic-report/v1")]
    V1(DiagnosticReportV1),
}

#[serde(tag = "schema", content = "payload")]
pub enum CompiledPackageEnvelope {
    #[serde(rename = "rulery.compiled-package/v1")]
    V1(CompiledPackageV1),
}

#[serde(tag = "schema", content = "payload")]
pub enum RulebookLockEnvelope {
    #[serde(rename = "rulery.rulebook-lock/v1")]
    V1(RulebookLockV1),
}

#[serde(tag = "schema", content = "payload")]
pub enum ScenarioResultEnvelope {
    #[serde(rename = "rulery.scenario-result/v1")]
    V1(ScenarioResultV1),
}

#[serde(tag = "schema", content = "payload")]
pub enum AnalysisReportEnvelope {
    #[serde(rename = "rulery.analysis-report/v1")]
    V1(AnalysisReportV1),
}

#[serde(tag = "schema", content = "payload")]
pub enum DecisionTableEnvelope {
    #[serde(rename = "rulery.decision-table/v1")]
    V1(DecisionTableV1),
}
```

These seven envelopes are the complete v0.1 persisted and interchange set.

## Diagnostic Contract

The supplied diagnostic model is adopted as supplemental context with the following corrections:

- `DiagnosticCode` remains an owned validated newtype, but custom `Deserialize` must call `DiagnosticCode::new`.
- Known code constants and their meanings live in a registry; arbitrary well-formed codes remain representable for forward compatibility.
- Codes `RUL900` through `RUL999` are internal. Unassigned ranges are `Reserved`, not `Internal`.
- `Diagnostic` and invariant-bearing members use private fields and validated constructors or builders.
- `DiagnosticReportEnvelope` is the stable interchange contract; in-memory model evolution is not tied to JSON compatibility.
- Structured evidence uses typed IDs where contracts exist instead of raw strings.

```rust
pub struct DiagnosticCode {
    value: String,
}

impl DiagnosticCode {
    pub fn new(value: impl Into<String>) -> Result<Self, DiagnosticCodeError>;
    pub fn as_str(&self) -> &str;
    pub fn family(&self) -> DiagnosticFamily;
}

pub enum DiagnosticFamily {
    Syntax,
    Vocabulary,
    Clarity,
    Semantics,
    RuleInteraction,
    Coverage,
    Scenario,
    Change,
    Export,
    Evidence,
    Reserved,
    Internal,
}

pub struct DiagnosticDefinition {
    code: DiagnosticCode,
    default_severity: Severity,
    title: &'static str,
    producer: DiagnosticProducer,
    suppressibility: Suppressibility,
}

pub fn diagnostic_definition(
    code: &DiagnosticCode,
) -> Option<&'static DiagnosticDefinition>;
```

The v0.1 registry retains the supplied names and codes from `SYNTAX_INVALID` (`RUL001`) through `INTERNAL_INVARIANT` (`RUL900`). The rebuilt specification adds one table mapping every known code to title, family, default severity, producer, suppressibility, and required evidence.

The in-memory diagnostic model retains these final type names:

```rust
pub struct Diagnostic {
    code: DiagnosticCode,
    severity: Severity,
    title: String,
    message: String,
    impact: Option<String>,
    labels: Vec<DiagnosticLabel>,
    notes: Vec<DiagnosticNote>,
    help: Vec<HelpItem>,
    evidence: Vec<DiagnosticEvidence>,
    fixes: Vec<SuggestedFix>,
    properties: DiagnosticProperties,
}

pub enum DiagnosticEvidence {
    Witness(WitnessEvidence),
    Trace(TraceEvidence),
    BehaviorChange(BehaviorChangeEvidence),
    Provenance(ProvenanceEvidence),
    AnalysisLimit(AnalysisLimitEvidence),
    Properties(BTreeMap<String, serde_json::Value>),
}

pub enum FindingConfidence {
    Proven,
    Witnessed,
    Heuristic,
    Inconclusive,
}
```

`SuggestedFix` validation rejects overlapping edits. `MachineApplicable` means syntax-safe, not semantics-preserving, unless the diagnostic definition explicitly guarantees semantic preservation.

## Evaluation, Analysis, and Scenario APIs

Evaluation receives the package artifact rather than a detached decision, making package identity, compiler identity, vocabulary validation, and decision lookup available to the evaluator.

```rust
pub trait DecisionEvaluator: Send + Sync {
    fn evaluate(
        &self,
        package: &CompiledPackage,
        decision: &DecisionId,
        facts: &CaseFacts,
        context: &EvaluationContext,
    ) -> Result<DecisionTrace, EvaluationError>;
}

pub struct EvaluationContext {
    clock: Arc<dyn Clock>,
    time_zones: Arc<dyn TimeZoneDatabase>,
    trace_detail: TraceDetail,
}

pub trait PolicyAnalyzer: Send + Sync {
    fn analyze(
        &self,
        package: &CompiledPackage,
        options: &AnalysisOptions,
    ) -> AnalysisReport;
}

pub trait ScenarioRunner: Send + Sync {
    fn run(
        &self,
        package: &CompiledPackage,
        scenario: &CompiledScenario,
        context: &EvaluationContext,
    ) -> ScenarioResult;
}

pub struct ScenarioCompilationOutput {
    scenarios: Vec<CompiledScenario>,
    diagnostics: DiagnosticReport,
}

pub trait ScenarioCompiler: Send + Sync {
    fn compile(
        &self,
        package: &CompiledPackage,
        scenarios: &[SourceScenario],
    ) -> ScenarioCompilationOutput;
}
```

Scenario comparison checks outcome, determining rules, required facts, and reason codes. Collection comparison semantics are set-based and exact unless a scenario field explicitly selects containment matching.

## Store and Rendering Ports

Filesystem behavior remains outside language and domain crates.

```rust
pub trait PackageStore: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    fn load_source(&self, root: &PackagePath) -> Result<LoadedSourceBundle, Self::Error>;
    fn resolve_import(
        &self,
        importer: &PackagePath,
        import: &ImportRef,
        lock: Option<&RulebookLock>,
    ) -> Result<LoadedSourceBundle, Self::Error>;
    fn load_lock(&self, root: &PackagePath) -> Result<Option<RulebookLock>, Self::Error>;
    fn write_lock(
        &self,
        root: &PackagePath,
        lock: &RulebookLock,
    ) -> Result<(), Self::Error>;
}

pub struct LoadedSourceBundle {
    bundle: SourceBundle,
    integrity: SourceIntegrity,
}

pub trait ArtifactRenderer<T>: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    fn render(&self, artifact: &T) -> Result<Vec<u8>, Self::Error>;
}
```

The CLI owns writing rendered bytes to stdout or files. Renderers never own command parsing or exit-code policy.

## Macro Boundary

The root facade exports declarative macros `stable_id!`, typed ID literal macros, `facts!`, `scenario!`, `assert_decision!`, and `diagnostic!`. Expansions use `$crate::__private` and call validated public constructors rather than constructing private fields.

`rulery-macros` owns procedural derives and compile-time literal validation. Downstream conformance tests rename the `rulery` dependency to prove expansion hygiene.

```rust
#[doc(hidden)]
pub mod __private {
    pub use rulery_contracts::*;
    pub use rulery_diagnostics::*;
    pub use rulery_scenarios::*;
}
```

## Data Flow

1. `rulery-store` loads the root `LoadedSourceBundle` and requested lock state; `rulery-syntax` parses the bundle into `ParsedPackage`, exposing declared imports and its `SourceMap`.
2. `PackageAssembler` applies the selected `LockMode`, recursively resolves the transitive import closure, rejects cycles and duplicate package identities, remaps source keys, and builds `CompilationInput` with complete integrity metadata and any proposed replacement lock.
3. `rulery-compiler` validates, resolves vocabulary and names, type-checks, normalizes, lowers, and returns `CompilationOutput`.
4. `ScenarioCompiler` resolves the parsed root scenarios against `CompiledPackage` and returns `ScenarioCompilationOutput`.
5. `rulery-engine` evaluates a decision in `CompiledPackage` against `CaseFacts` and an explicit UTC clock, producing a complete `DecisionTrace`.
6. `rulery-scenarios` compares traces against compiled expectations and produces typed results.
7. `rulery-analysis` explores the compiled package and emits findings with concrete witnesses or explicit inconclusive evidence.
8. `rulery-emit` renders typed artifacts; `rulery-cli` selects formats, writes output, and maps results to documented exit codes.

## Hexagonal Boundaries

- **Port:** `PackageStore` in `rulery-store`; **adapter:** `FilesystemPackageStore` in `rulery-store::filesystem`.
- **Port:** `Clock` in `rulery-contracts`; **adapters:** `SystemClock` and `FixedClock` in `rulery-engine::clock`.
- **Port:** `TimeZoneDatabase` in `rulery-contracts`; **adapter:** `JiffTimeZoneDatabase` in `rulery-engine::time`.
- **Port:** `SourceParser` in `rulery-syntax`; **adapter:** `YamlSourceParser` in `rulery-syntax::yaml`.
- **Port:** `ArtifactRenderer<T>` in `rulery-emit`; **adapters:** human, JSON, Markdown, decision-table, and SARIF renderers in format modules.
- No network port exists in v0.1; imports resolve from local package sources only.

## Specification Repair Acceptance

The documentation-only repair is complete when:

1. `docs/specification.md` contains every normative contract and has no unresolved feasibility qualifiers.
2. Rust blocks extracted into `.ctx/_WORKING_DIR/spec-check/` pass formatting and `cargo check` as a temporary validation harness; no harness code is committed as production implementation.
3. YAML and JSON examples pass syntax and embedded-schema validation from the same temporary workspace.
4. Canonical serialization and hashing include fixed vectors independently recomputed in the temporary workspace.
5. Truth, precedence, uncertainty, temporal, and scenario semantics have complete tables and expected results.
6. Every diagnostic code has a registry row, and every CLI command has an exit-condition matrix.
7. All internal links, type references, crate names, and dependency edges resolve consistently.

The repaired blueprint also defines, but does not execute, future implementation conformance gates: workspace formatting, Clippy with warnings denied, nextest, doctests, architecture checks, format round trips, fixed hash vectors, and the tool-library end-to-end fixture.

## Blueprint Dependency Phases

These phases constrain the specification's dependency narrative and acceptance criteria; they are not an implementation task plan.

1. Contracts, diagnostics, and vocabulary.
2. Syntax, IR, compiler, and store.
3. Engine and scenarios.
4. Analysis and emit.
5. Facade, macros, CLI, conformance, and compatibility review.

The specification assigns compile, serialization, semantic, and conformance exit criteria to each phase. This design does not implement any phase.

## Integration Points

- `Cargo.toml` becomes a workspace manifest and root facade package during implementation, not during this design phase.
- `src/main.rs` is replaced by a facade library entry point; the executable moves to `crates/cli` during implementation.
- The ignored `local/spec.local.md` remains unchanged descriptive input. The repaired normative specification is written to `docs/specification.md`.
- The diagnostic model supplied during design is incorporated as corrected API context, not copied as implementation code.
- No compatibility feature flag is required because no public v0.1 release or consumer exists.
- The optional procedural macro dependency is exposed through a facade feature named `macros`, disabled by default.

## Out of Scope

- Production implementation, committed test scaffolding, or Cargo dependency changes during the specification repair. Temporary validation artifacts are restricted to `.ctx/_WORKING_DIR/`.
- LSP implementation.
- Native `.rulery` syntax.
- WASM, Rego, Cedar, SQL, database, remote registry, and network import adapters.
- Signed releases, AI authoring, natural-language extraction, runtime action execution, and workflow orchestration.
- Full theorem proving over unbounded domains.
- Backward compatibility with the current unreleased YAML and JSON examples.

## Design Risks

- [x] **Breaking API changes:** yes - the existing descriptive contracts are intentionally replaced before release.
- [x] **Serialization changes:** yes - source and generated formats become explicit, validated, canonical, and versioned.
- [x] **New external dependencies:** yes - `jiff` provides IANA timezone conversion behind `TimeZoneDatabase`; `rust_decimal` provides private exact decimal representation; `yaml-rust2` powers `YamlSourceParser`; `serde_jcs` provides RFC 8785 encoding; and `syn`, `quote`, `proc-macro2`, and `proc-macro-crate` support `rulery-macros`.
- [x] **Feature flag:** yes - procedural macros are exposed through the facade's `macros` feature.
- [x] **Cross-crate breadth:** high - planning must split work by the approved delivery phases and preserve dependency order.
- [x] **Specification size:** high - the rewrite must use compile fixtures and schemas to prevent prose/code drift.

## Design Approval Gate

No implementation plan or production code may be written until this design is explicitly approved. After approval, the next command is `/gm-plan`.
