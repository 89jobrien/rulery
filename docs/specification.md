# Rulery v0.1 Core Specification

<!-- markdownlint-disable MD013 -->

Status: Normative blueprint for Rulery language version 1 and v0.1 implementations.

This document is the authority for Rulery v0.1. An implementation conforms only when its
source contracts, compiled artifacts, evaluation results, diagnostics, and CLI behavior agree
with this specification.

## Table of contents

- [Conventions and authority](#conventions-and-authority)
- [Goals and non-goals](#goals-and-non-goals)
- [Workspace and dependency architecture](#workspace-and-dependency-architecture)
  - [Workspace shape](#workspace-shape)
  - [Crate ownership](#crate-ownership)
  - [Complete dependency direction](#complete-dependency-direction)
- [Core contracts](#core-contracts)
  - [Validated newtypes and wire grammars](#validated-newtypes-and-wire-grammars)
  - [Source identity and spans](#source-identity-and-spans)
  - [Values and facts](#values-and-facts)
  - [Supporting domain declarations](#supporting-domain-declarations)
- [Authored language](#authored-language)
  - [Exact file layout](#exact-file-layout)
  - [Exhaustive authored grammar](#exhaustive-authored-grammar)
  - [Normative authored YAML example](#normative-authored-yaml-example)
- [Compilation and package assembly](#compilation-and-package-assembly)
  - [Assembly and imports](#assembly-and-imports)
  - [Compilation](#compilation)
- [Evaluation semantics](#evaluation-semantics)
  - [Four-valued truth](#four-valued-truth)
  - [Predicate behavior](#predicate-behavior)
  - [Missing and invalid strategies](#missing-and-invalid-strategies)
  - [Outcomes](#outcomes)
  - [Precedence and conflict](#precedence-and-conflict)
  - [Evaluation algorithm and trace](#evaluation-algorithm-and-trace)
- [Time semantics](#time-semantics)
- [Canonical serialization and hashing](#canonical-serialization-and-hashing)
  - [Canonical payloads](#canonical-payloads)
  - [Framing, integrity, and domains](#framing-integrity-and-domains)
  - [Fixed conformance vectors](#fixed-conformance-vectors)
- [Versioned envelopes](#versioned-envelopes)
- [Diagnostics](#diagnostics)
  - [Model and invariants](#model-and-invariants)
  - [Complete v0.1 known-code registry](#complete-v01-known-code-registry)
- [Analysis](#analysis)
  - [Finite partitions and options](#finite-partitions-and-options)
  - [Soundness, completeness, and budgets](#soundness-completeness-and-budgets)
  - [Semantic diff](#semantic-diff)
- [Scenarios](#scenarios)
- [Ports, facade, and macros](#ports-facade-and-macros)
  - [Complete port list](#complete-port-list)
  - [Facade workflows](#facade-workflows)
  - [Macro contracts and hygiene](#macro-contracts-and-hygiene)
- [CLI](#cli)
  - [Command surface and lock defaults](#command-surface-and-lock-defaults)
  - [Output contract](#output-contract)
  - [Exit-condition matrix](#exit-condition-matrix)
- [Tool-library conformance example](#tool-library-conformance-example)
- [Conformance and governance](#conformance-and-governance)
- [Blueprint phases](#blueprint-phases)
- [Deferred features](#deferred-features)
- [Canonical success criterion](#canonical-success-criterion)

## Conventions and authority

The key words MUST, MUST NOT, REQUIRED, SHALL, SHALL NOT, SHOULD, SHOULD NOT, RECOMMENDED,
NOT RECOMMENDED, MAY, and OPTIONAL are to be interpreted as described by
[RFC 2119](https://www.rfc-editor.org/rfc/rfc2119) and
[RFC 8174](https://www.rfc-editor.org/rfc/rfc8174) when, and only when, they appear in all
capitals.

Authority descends in this order:

1. Normative semantics in this document.
2. Canonical authored and generated data contracts.
3. Crate ownership and dependency rules.
4. Normative Rust signatures and data declarations.
5. Conformance fixtures and fixed expected behavior.
6. Blueprint phases.
7. Material explicitly labeled illustrative or deferred.

If two normative portions disagree, the implementation MUST stop and the disagreement MUST be
treated as a specification defect. Illustrative examples never override normative text. Rust
blocks labeled normative contain signatures and data declarations only; they are not function
implementations.

Unless a block is explicitly labeled illustrative, every fenced code, schema, grammar, fixture,
and output block in this document is normative. Illustrative blocks define no contract.

Language version `1` denotes the v0.1 source language. Wire schema versions are independent of
the language version.

## Goals and non-goals

### Functional goals

Rulery v0.1 MUST:

- load a local, structured YAML rulebook and its complete local import closure;
- resolve declared vocabulary, names, fact paths, enum symbols, actions, and decisions;
- type-check and lower source into deterministic, evaluator-ready IR;
- distinguish `true`, `false`, `unknown`, and `invalid` throughout evaluation;
- produce `approve`, `deny`, `escalate`, or `request_information` outcomes;
- preserve explicit missing-fact, invalid-fact, precedence, and time semantics;
- emit deterministic and reproducible decision traces;
- compile and run named scenarios as executable behavior contracts;
- detect syntax, vocabulary, semantic, interaction, coverage, and change problems;
- attach concrete witnesses to proven existential analysis findings;
- provide stable JSON envelopes and human, Markdown, decision-table, and SARIF rendering;
- operate offline with no network import path; and
- expose the same behavior through a Rust facade and the `rulery` CLI.

### Explicit non-goals

Rulery v0.1 is not:

- a legal interpretation system or a substitute for accountable human judgment;
- a natural-language policy extractor, AI authoring system, or LLM decision engine;
- a workflow scheduler or runtime action executor;
- a database, remote package registry, or cloud administration platform;
- an authorization-language replacement for Rego or Cedar;
- a system that silently defines ambiguous human terms;
- a theorem prover for unbounded domains; or
- a compatibility layer for unreleased pre-v0.1 YAML or JSON shapes.

## Workspace and dependency architecture

### Workspace shape

The implementation workspace MUST have this logical shape. The `lsp` directory MAY be absent in
v0.1 because it is deferred.

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
|   `-- lsp/                    # deferred; excluded from v0.1
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

The root package MUST be the `rulery` library facade. The executable MUST be owned exclusively
by `rulery-cli`.

### Crate ownership

| Crate                | Normative single responsibility                                                                                |
| -------------------- | -------------------------------------------------------------------------------------------------------------- |
| `rulery`             | Curated facade, embedding workflow, and hygienic declarative macros                                            |
| `rulery-contracts`   | Stable IDs, facts, values, outcomes, time inputs, source maps, raw bundles, integrity, and artifact identities |
| `rulery-diagnostics` | Diagnostic IDs, registry, reports, labels, evidence, fixes, and diagnostic envelope                            |
| `rulery-syntax`      | Private YAML DTOs, YAML decoding, source-map construction, and spanned format-neutral source AST               |
| `rulery-vocabulary`  | Resolved schemas, operational terms, type relationships, and value validation                                  |
| `rulery-ir`          | Checked resolved expressions, rules, decisions, actions, and compiled package data                             |
| `rulery-compiler`    | Validation, resolution, type checking, normalization, lowering, provenance, and package hashing                |
| `rulery-engine`      | Four-valued evaluation, relevance, precedence, outcomes, and deterministic traces                              |
| `rulery-analysis`    | Reachability, overlap, conflict, coverage, witnesses, and semantic diff                                        |
| `rulery-scenarios`   | Source scenario compilation, exact assertions, execution, and results                                          |
| `rulery-emit`        | Human, JSON, Markdown, decision-table, and SARIF renderers                                                     |
| `rulery-store`       | Local discovery, raw source loading, import resolution, lockfiles, and integrity checks                        |
| `rulery-cli`         | Commands, use-case orchestration, output selection, file writes, and exit policy                               |
| `rulery-macros`      | Procedural macros and compile-time literal validation only                                                     |
| `rulery-lsp`         | Deferred editor adapter; no v0.1 contract                                                                      |

Normalization and lowering belong to `rulery-compiler`. `rulery-ir` owns checked data
construction, not compiler passes. Source decoding belongs to `rulery-syntax`. Scenario
execution belongs to `rulery-scenarios`.

### Complete dependency direction

The following list is the complete set of permitted direct Rulery-crate dependencies. A crate
MUST NOT add a Rulery dependency not listed here.

| Dependent            | Permitted direct Rulery dependencies                                                                                                                                                                                            |
| -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `rulery-contracts`   | none                                                                                                                                                                                                                            |
| `rulery-diagnostics` | `rulery-contracts`                                                                                                                                                                                                              |
| `rulery-vocabulary`  | `rulery-contracts`                                                                                                                                                                                                              |
| `rulery-syntax`      | `rulery-contracts`                                                                                                                                                                                                              |
| `rulery-ir`          | `rulery-contracts`, `rulery-vocabulary`                                                                                                                                                                                         |
| `rulery-compiler`    | `rulery-contracts`, `rulery-diagnostics`, `rulery-syntax`, `rulery-vocabulary`, `rulery-ir`                                                                                                                                     |
| `rulery-engine`      | `rulery-contracts`, `rulery-ir`                                                                                                                                                                                                 |
| `rulery-analysis`    | `rulery-contracts`, `rulery-diagnostics`, `rulery-vocabulary`, `rulery-ir`, `rulery-engine`                                                                                                                                     |
| `rulery-scenarios`   | `rulery-contracts`, `rulery-diagnostics`, `rulery-syntax`, `rulery-ir`, `rulery-engine`                                                                                                                                         |
| `rulery-store`       | `rulery-contracts`                                                                                                                                                                                                              |
| `rulery-emit`        | `rulery-contracts`, `rulery-diagnostics`, `rulery-ir`, `rulery-engine`, `rulery-analysis`, `rulery-scenarios`                                                                                                                   |
| `rulery-macros`      | none; it has no Rulery dependency and never depends on `rulery`                                                                                                                                                                 |
| `rulery`             | `rulery-contracts`, `rulery-diagnostics`, `rulery-syntax`, `rulery-vocabulary`, `rulery-ir`, `rulery-compiler`, `rulery-engine`, `rulery-analysis`, `rulery-scenarios`, `rulery-emit`, `rulery-store`; optional `rulery-macros` |
| `rulery-cli`         | `rulery` and external `clap` only                                                                                                                                                                                               |

These edges MUST form an acyclic graph. Architecture tests MUST reject cycles and unlisted
edges. `rulery-store` returns raw bundles; it MUST NOT parse or compile. The facade and CLI
assemble store, parser, compiler, evaluator, analyzer, scenario, and renderer services.
`rulery-engine` consumes a self-sufficient `CompiledPackage` and MUST NOT depend directly on
syntax, diagnostics, vocabulary, store, emit, or CLI.

`rulery-contracts` owns `SourceKey`, `Span`, `SourceMap`, and related source identity types.
`rulery-syntax` constructs those types while decoding; ownership does not imply construction.
`rulery-syntax` returns typed `SourceParseError` values and does not construct diagnostics. The
facade or compiler converts parser errors into `rulery-diagnostics` values with source labels.
Likewise, `rulery-store` returns typed store errors; the facade converts them to diagnostics while
preserving `Store` as the registry producer.

## Core contracts

### Validated newtypes and wire grammars

All stable boundary types with private fields MUST expose checked constructors. Their custom
`Deserialize` implementations MUST deserialize an intermediate string or scalar and invoke the
same checked constructor. Deriving `Deserialize` directly over an invariant-bearing private
field is non-conforming.

The stable-ID grammar is:

```text
[a-z0-9](?:[a-z0-9._-]{0,126}[a-z0-9])?
```

It permits 1 through 128 ASCII characters. Typed IDs include `PackageId`, `DecisionId`,
`RuleId`, `TypeId`, `PredicateId`, `ActionId`, `ScenarioId`, `EscalationId`, `SourceId`,
`FactRootId`, and `ReasonCode`. A `QualifiedRuleId` is the ordered pair `(PackageId, RuleId)`.
Its text form is `PACKAGE_ID::RULE_ID`; `:` is not legal inside either component. Display labels
MUST NOT serve as stable references.

`FactSegment` uses `[a-z0-9](?:[a-z0-9_-]{0,126}[a-z0-9])?`. `FactPath` is one or more
`FactSegment` values joined by `.` and MUST contain no empty segment.

The exact decimal grammar is:

```text
-?(0|[1-9][0-9]*)(\.[0-9]*[1-9])?
```

Decimal construction MUST normalize negative zero to `0` and remove a zero fractional part.
Policy comparison and hashing MUST NOT use binary floating point.

Dates use zero-padded proleptic Gregorian `YYYY-MM-DD`, with years `0001` through `9999`.
Displayed date-times use UTC RFC 3339 with exactly nine fractional digits and a `Z` suffix.
UTC instants on the wire use signed base-10 Unix nanoseconds. Durations use signed base-10
nanoseconds. Signed integer strings have no plus sign or leading zeroes; zero is `0`. UUIDs are
lowercase hyphenated strings. A content hash matches `blake3:[0-9a-f]{64}` exactly.

Exact lexical grammars are:

```text
integer-or-nanoseconds = -?(0|[1-9][0-9]*)
date = [0-9]{4}-[0-9]{2}-[0-9]{2}
display-date-time = [0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]{9}Z
uuid = [0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}
content-hash = blake3:[0-9a-f]{64}
```

Lexical acceptance is followed by calendar, time, UUID variant/version where required, and
numeric-range validation. Leap seconds are rejected; UTC seconds are `00` through `59`.

**Normative Rust signatures and data declarations:**

```rust
pub struct StableId {
    value: String,
}

pub struct DecimalValue {
    canonical: String,
}

pub struct ContentHash {
    bytes: [u8; 32],
}

pub struct FactPath {
    segments: Vec<FactSegment>,
}

pub struct DiagnosticCode {
    value: String,
}

impl StableId {
    pub fn new(value: impl Into<String>) -> Result<Self, StableIdError>;
    pub fn as_str(&self) -> &str;
}

impl DecimalValue {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, DecimalValueError>;
    pub fn as_str(&self) -> &str;
}

impl ContentHash {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, ContentHashError>;
    pub fn as_bytes(&self) -> &[u8; 32];
}
```

`ContentHash::parse` MUST reject unknown algorithms, uppercase hexadecimal, and any digest not
exactly 32 bytes.

### Source identity and spans

`Span` is a half-open byte interval `[start, end)` into UTF-8 source content. `start <= end`
MUST hold, both offsets MUST be UTF-8 boundaries, and `end` MUST not exceed the source length.
Line and column positions are derived lazily. Package-local parsing may initially assign local
keys; assembly remaps them as specified below.

**Normative Rust data declarations:**

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

pub struct SourceDocument {
    path: SourcePath,
    bytes: Arc<[u8]>,
}

pub struct SourceBundle {
    root: PackagePath,
    documents: BTreeMap<SourcePath, SourceDocument>,
}

pub struct SourceIntegrity {
    package: PackageId,
    version: Version,
    location: NormalizedSourceLocation,
    bundle_hash: ContentHash,
    document_hashes: BTreeMap<SourcePath, ContentHash>,
}

pub struct LoadedSourceBundle {
    bundle: SourceBundle,
    integrity: SourceIntegrity,
}
```

`SourceKey` and `Span` MAY implement `Copy`. `SourceId`, `SourcePath`, and `SourceFile` MUST NOT
implement `Copy`. Every authored condition and effect MUST have a span before compilation.

`SourceBundle` contains exactly the selected authored files, with paths relative to its canonical
package root. Paths use `/`, contain no `.` or `..` segment, and sort by UTF-8 bytes.

### Values and facts

**Normative Rust data declarations:**

```rust
pub enum Value {
    Null,
    Boolean(bool),
    Integer(i64),
    Decimal(DecimalValue),
    Text(String),
    Date(PolicyDate),
    DateTime(UtcInstant),
    Duration(DurationValue),
    Enum(EnumValue),
    List(Vec<Value>),
    Record(BTreeMap<StableId, Value>),
}

pub struct EnumValue {
    type_id: TypeId,
    variant: StableId,
}

pub struct CaseFacts {
    roots: BTreeMap<FactRootId, Value>,
}

pub enum FactState<'a> {
    Absent,
    Null,
    Valid(&'a Value),
    Malformed(FactValidationError),
}
```

Absence means no value exists at a path. `Null` is a supplied value and is never absence.
Malformed means supplied evidence violates vocabulary or an operator contract. Structural path
traversal into a non-record supplied value is malformed, not absent. A required absent field is
both absent for predicate semantics and a validation diagnostic.

### Supporting domain declarations

The following declarations close the public blueprint vocabulary. Each ID wrapper uses the
stable-ID constructor and custom deserialization rule. String wrappers have private fields and
validated constructors. Error enums carry typed subjects and non-empty messages.

**Normative Rust data declarations:**

```rust
pub struct PackageId(StableId);
pub struct DecisionId(StableId);
pub struct RuleId(StableId);
pub struct TypeId(StableId);
pub struct PredicateId(StableId);
pub struct ActionId(StableId);
pub struct ScenarioId(StableId);
pub struct EscalationId(StableId);
pub struct SourceId(StableId);
pub struct FactRootId(StableId);
pub struct ReasonCode(StableId);
pub struct FactSegment(StableId);

pub struct QualifiedRuleId {
    package: PackageId,
    rule: RuleId,
}

pub struct SourcePath(String);
pub struct PackagePath(PathBuf);
pub struct NormalizedSourceLocation(String);
pub struct Version(String);
pub struct LanguageVersion(u16);
pub struct DurationValue(i128);
pub struct UnresolvedFactPath(String);

pub struct PackageMetadata {
    id: PackageId,
    display_name: String,
    version: Version,
    language_version: LanguageVersion,
    description: Option<String>,
    authors: Vec<Author>,
    tags: BTreeSet<StableId>,
}

pub struct Author {
    name: String,
    contact: Option<String>,
}

pub struct CompilerIdentity {
    name: String,
    version: Version,
}

pub struct ProducerIdentity {
    name: String,
    version: Version,
}

pub struct ImportRef {
    package: PackageId,
    version: VersionRequirement,
    path: SourcePath,
    alias: Option<StableId>,
}

pub struct Action {
    id: ActionId,
    display_name: String,
    description: Option<String>,
    parameters: BTreeMap<StableId, ActionParameter>,
}

pub struct ActionParameter {
    type_id: TypeId,
    required: bool,
    description: Option<String>,
}

pub struct ActionInvocation {
    action: ActionId,
    arguments: BTreeMap<StableId, Value>,
}

pub struct Reason {
    code: ReasonCode,
    message: String,
    detail: Option<String>,
}

pub enum OutcomeKind {
    Approve,
    Deny,
    Escalate,
    RequestInformation,
}

pub enum Outcome {
    Approve { reasons: Reasons, actions: Vec<ActionInvocation> },
    Deny { reasons: Reasons, actions: Vec<ActionInvocation> },
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

pub struct ResolvedVocabulary {
    roots: BTreeMap<FactRootId, ResolvedRoot>,
    types: BTreeMap<TypeId, ResolvedType>,
    terms: BTreeMap<StableId, OperationalTerm>,
}

pub struct CompiledDecision {
    id: DecisionId,
    semantics: DecisionSemantics,
    default: OutcomeTemplate,
    rules: Vec<CompiledRule>,
    span: Span,
}

pub struct SourcePackage {
    metadata: PackageMetadata,
    semantics: DecisionSemantics,
    imports: Vec<ImportRef>,
    decisions: Vec<SourceDecision>,
    vocabulary: SourceVocabulary,
    actions: Vec<SourceAction>,
    rules: Vec<SourceRule>,
}

pub struct SourceScenario {
    id: ScenarioId,
    title: String,
    description: Option<String>,
    decision: DecisionId,
    at: UtcInstant,
    given: SourceFacts,
    expect: SourceExpectedDecision,
    tags: BTreeSet<StableId>,
    span: Span,
}

pub enum SourceOperand {
    Literal(SourceValue),
    Fact(UnresolvedFactPath),
    Reserved(ReservedOperand),
}

pub enum ReservedOperand { Today, Now }

pub enum FactValidationError {
    TypeMismatch { expected: TypeId, actual: ValueKind },
    InvalidEnumVariant { type_id: TypeId, variant: StableId },
    OutOfRange,
    UnknownRecordField { field: StableId },
    DerivedFieldSupplied { path: FactPath },
}

pub struct VersionRequirement(String);
pub struct ResolvedRoot {
    type_id: TypeId,
    description: Option<String>,
    span: Span,
}
pub struct ResolvedType { id: TypeId, declaration: TypeDeclaration }
pub struct OperationalTerm {
    id: StableId,
    display_name: String,
    definition: String,
    applies_to: BTreeSet<FactPath>,
    examples: Vec<String>,
    counterexamples: Vec<String>,
    span: Span,
}
pub struct CompiledRule {
    id: RuleId,
    title: Option<String>,
    priority: i32,
    specificity: u32,
    override_rank: u8,
    condition: Expr,
    outcome: OutcomeTemplate,
    rationale: Option<String>,
    span: Span,
}
pub struct SourceDecision {
    id: DecisionId,
    title: String,
    asks: String,
    input_roots: BTreeSet<FactRootId>,
    default: OutcomeTemplate,
    span: Span,
}
pub struct SourceVocabulary {
    roots: BTreeMap<FactRootId, SourceRoot>,
    types: BTreeMap<TypeId, TypeDeclaration>,
    terms: BTreeMap<StableId, OperationalTerm>,
}
pub struct SourceAction {
    id: ActionId,
    display_name: String,
    description: Option<String>,
    parameters: BTreeMap<StableId, ActionParameter>,
    span: Span,
}
pub struct SourceRule {
    id: RuleId,
    title: Option<String>,
    priority: i32,
    condition: SourceCondition,
    outcome: OutcomeTemplate,
    rationale: Option<String>,
    explicit_override: bool,
    span: Span,
}
pub struct SourceFacts { roots: BTreeMap<FactRootId, SourceValue> }
pub struct SourceExpectedDecision {
    outcome: OutcomeKind,
    determining_rules: BTreeSet<QualifiedRuleId>,
    required_facts: BTreeSet<FactPath>,
    reason_codes: BTreeSet<ReasonCode>,
}
pub struct SourceRoot {
    id: FactRootId,
    type_id: TypeId,
    description: Option<String>,
    span: Span,
}
pub struct SourceValue(serde_json::Value);

pub enum SourceCondition {
    All { terms: Vec<SourceCondition>, span: Span },
    Any { terms: Vec<SourceCondition>, span: Span },
    Not { term: Box<SourceCondition>, span: Span },
    Predicate(SourcePredicate),
}

pub enum Expr {
    All { terms: Vec<Expr>, span: Span },
    Any { terms: Vec<Expr>, span: Span },
    Not { term: Box<Expr>, span: Span },
    Predicate(Predicate),
    Constant { value: Truth, span: Span },
}

pub enum Truth { True, False, Unknown, Invalid }
pub struct Predicate {
    fact: FactPath,
    operator: Operator,
    operand: Option<Value>,
    span: Span,
}

pub struct CaseFactsV1 {
    roots: BTreeMap<FactRootId, Value>,
}

pub struct EvaluationV1 {
    package_hash: ContentHash,
    decision: DecisionId,
    facts_hash: ContentHash,
    evaluated_at: UtcInstant,
    timezone_database: TimeZoneDatabaseIdentity,
}

pub struct DiagnosticReport {
    payload: DiagnosticReportV1,
}

pub struct StableIdError(String);
pub struct DecimalValueError(String);
pub struct ContentHashError(String);
pub struct DiagnosticCodeError(String);
pub struct SourceParseError { kind: SourceParseErrorKind, span: Option<Span> }
pub enum SourceParseErrorKind { Syntax, Shape, ResourceLimit }
pub struct FactBuildError { path: FactPath, message: String }
pub struct ScenarioBuildError { field: String, message: String }
pub struct DiagnosticBuildError { field: String, message: String }

pub enum TypeDeclaration {
    Enum {
        description: Option<String>,
        variants: BTreeMap<StableId, EnumVariant>,
        span: Span,
    },
    Record {
        description: Option<String>,
        fields: BTreeMap<StableId, FieldDeclaration>,
        closed: bool,
        span: Span,
    },
    List {
        description: Option<String>,
        items: TypeId,
        min_items: Option<u32>,
        max_items: Option<u32>,
        span: Span,
    },
}

pub struct EnumVariant {
    display_name: String,
    description: Option<String>,
    deprecated: bool,
    span: Span,
}

pub struct FieldDeclaration {
    type_id: TypeId,
    presence: FieldPresence,
    description: Option<String>,
    span: Span,
}

pub enum FieldPresence { Required, Optional, Derived }

pub enum ValueKind {
    Null,
    Boolean,
    Integer,
    Decimal,
    Text,
    Date,
    DateTime,
    Duration,
    Enum,
    List,
    Record,
}
```

These compiler-owned checked records are not interchange artifacts; their constructors enforce
the exhaustive authored grammar and the compilation invariants.

## Authored language

### Exact file layout

Each package root MUST contain:

```text
rulery.yaml
vocabulary.yaml
actions.yaml
rules/*.yaml
scenarios/*.yaml
```

The three root files are REQUIRED. `rules/` and `scenarios/` are REQUIRED directories and MAY
be empty. Files in each glob are read in ascending normalized relative-path byte order.

`rulery.yaml` owns package metadata, language version, semantics, imports, and decisions.
`vocabulary.yaml` owns roots, named composite types, and operational terms. `actions.yaml` owns
action declarations. Rule files group rules by decision. Scenario files each define one
scenario.

Boolean, integer, decimal, text, date, date-time, and duration are primitive built-ins with IDs
`boolean`, `integer`, `decimal`, `text`, `date`, `date-time`, and `duration`. They MUST NOT be
redeclared. Enums, records, and lists are named declarations.

Private YAML DTOs belong solely to `rulery-syntax`. The public source AST is format-neutral.
Every source predicate has exactly one `SourceOperator`; multi-option predicate structs are
forbidden.

**Normative Rust data declarations:**

```rust
pub struct SourcePredicate {
    fact: UnresolvedFactPath,
    operator: SourceOperator,
    span: Span,
}

pub enum SourceOperator {
    Equal(SourceOperand),
    NotEqual(SourceOperand),
    LessThan(SourceOperand),
    LessThanOrEqual(SourceOperand),
    GreaterThan(SourceOperand),
    GreaterThanOrEqual(SourceOperand),
    Contains(SourceOperand),
    NotContains(SourceOperand),
    StartsWith(SourceOperand),
    EndsWith(SourceOperand),
    IsOneOf(Vec<SourceOperand>),
    IsAbsent,
    IsPresent,
    IsValid,
    IsInvalid,
    Before(SourceOperand),
    OnOrAfter(SourceOperand),
    IsExpired,
    IsUnexpired,
}

pub struct ParsedPackage {
    package: SourcePackage,
    scenarios: Vec<SourceScenario>,
    source_map: SourceMap,
}
```

Unknown YAML fields MUST be rejected. An empty `all` lowers to true and an empty `any` lowers
to false, each with an advisory diagnostic. `override: true` requires non-empty rationale.

### Exhaustive authored grammar

All mappings in this section reject unknown fields. A field without a default is REQUIRED.
Collections default to empty only where stated. IDs, paths, decimals, dates, date-times, and
durations use the validated grammars in this specification. YAML aliases, merge keys, duplicate
mapping keys, non-string mapping keys, and custom tags are rejected.

#### Manifest and imports

| Object           | Field              | Type                             | Required/default |
| ---------------- | ------------------ | -------------------------------- | ---------------- |
| manifest         | `package`          | package metadata                 | required         |
| manifest         | `semantics`        | package semantics                | required         |
| manifest         | `imports`          | list of import                   | default empty    |
| manifest         | `decisions`        | non-empty list of decision       | required         |
| package metadata | `id`               | `PackageId`                      | required         |
| package metadata | `display_name`     | non-empty text                   | required         |
| package metadata | `version`          | semantic version                 | required         |
| package metadata | `language_version` | integer `1`                      | required         |
| package metadata | `description`      | non-empty text                   | optional         |
| package metadata | `authors`          | list of author                   | default empty    |
| package metadata | `tags`             | set of `StableId`                | default empty    |
| author           | `name`             | non-empty text                   | required         |
| author           | `contact`          | non-empty text                   | optional         |
| import           | `package`          | `PackageId`                      | required         |
| import           | `version`          | semantic-version requirement     | required         |
| import           | `path`             | normalized relative package path | required         |
| import           | `alias`            | `StableId`                       | optional         |

An import path MUST be relative, MUST NOT be empty, and MUST remain below the declaring package
root after canonical resolution. Import aliases are unique in one manifest. Decisions contain
`id`, non-empty `title`, non-empty `asks`, a non-empty set `input_roots`, and `default`.
Decision IDs are unique. A default is an outcome object without `required_facts` unless its kind
is `request_information`. Package semantics apply to every decision; v0.1 has no decision-level
semantics override.

#### Semantics and precedence

| Object              | Field           | Type                                                                           | Required/default             |
| ------------------- | --------------- | ------------------------------------------------------------------------------ | ---------------------------- |
| semantics           | `timezone`      | canonical IANA name                                                            | required                     |
| semantics           | `expiry`        | `inclusive` or `exclusive`                                                     | required                     |
| semantics           | `missing_facts` | missing strategy                                                               | required                     |
| semantics           | `invalid_facts` | invalid strategy                                                               | required                     |
| semantics           | `precedence`    | precedence object                                                              | required                     |
| missing strategy    | `kind`          | `preserve_unknown`, `closed_world_false`, `request_information`, or `escalate` | required                     |
| missing escalation  | `destination`   | `EscalationId`                                                                 | required only for `escalate` |
| invalid strategy    | `kind`          | `reject_evaluation`, `preserve_invalid`, or `escalate`                         | required                     |
| invalid escalation  | `destination`   | `EscalationId`                                                                 | required only for `escalate` |
| precedence          | `kind`          | `safety_first`, `priority_first`, or `explicit`                                | required                     |
| explicit precedence | `primary`       | `outcome` or `priority`                                                        | required                     |
| explicit precedence | `outcome_ranks` | outcome-to-u16 mapping                                                         | required                     |

An explicit rank mapping has exactly the keys `approve`, `deny`, `escalate`, and
`request_information`, with four distinct unsigned 16-bit values. Non-escalation strategies
reject `destination`; escalation strategies require it.

#### Vocabulary

| Object     | Field                         | Type                                 | Required/default |
| ---------- | ----------------------------- | ------------------------------------ | ---------------- |
| vocabulary | `types`                       | map `TypeId` to named type           | default empty    |
| vocabulary | `roots`                       | non-empty map `FactRootId` to root   | required         |
| vocabulary | `terms`                       | map `StableId` to term               | default empty    |
| root       | `type`                        | primitive or named `TypeId`          | required         |
| root       | `description`                 | non-empty text                       | optional         |
| enum       | `kind`                        | `enum`                               | required         |
| enum       | `variants`                    | non-empty map `StableId` to variant  | required         |
| record     | `kind`                        | `record`                             | required         |
| record     | `closed`                      | Boolean                              | default true     |
| record     | `fields`                      | map `StableId` to field              | default empty    |
| list       | `kind`                        | `list`                               | required         |
| list       | `items`                       | primitive or named `TypeId`          | required         |
| list       | `min_items`, `max_items`      | unsigned integer                     | optional         |
| variant    | `display_name`                | non-empty text                       | required         |
| variant    | `description`                 | non-empty text                       | optional         |
| variant    | `deprecated`                  | Boolean                              | default false    |
| field      | `type`                        | primitive or named `TypeId`          | required         |
| field      | `presence`                    | `required`, `optional`, or `derived` | required         |
| field      | `description`                 | non-empty text                       | optional         |
| term       | `display_name`                | non-empty text                       | required         |
| term       | `definition`                  | non-empty text                       | required         |
| term       | `applies_to`                  | non-empty set of `FactPath`          | required         |
| term       | `examples`, `counterexamples` | list of non-empty text               | default empty    |

Named types are only enum, record, and list. Primitive constraints are not authored in v0.1.
`min_items <= max_items` when both exist. Closed records reject undeclared supplied fields.
Derived fields cannot be supplied as case facts.

#### Actions, rules, conditions, and operands

| Object       | Field          | Type                        | Required/default               |
| ------------ | -------------- | --------------------------- | ------------------------------ |
| actions file | `actions`      | map `ActionId` to action    | default empty                  |
| action       | `display_name` | non-empty text              | required                       |
| action       | `description`  | non-empty text              | optional                       |
| action       | `parameters`   | map `StableId` to parameter | default empty                  |
| parameter    | `type`         | primitive or named `TypeId` | required                       |
| parameter    | `required`     | Boolean                     | default true                   |
| parameter    | `description`  | non-empty text              | optional                       |
| rule file    | `decision`     | `DecisionId`                | required                       |
| rule file    | `rules`        | list of rule                | default empty                  |
| rule         | `id`           | `RuleId`                    | required                       |
| rule         | `title`        | non-empty text              | optional                       |
| rule         | `priority`     | signed 32-bit integer       | default `0`                    |
| rule         | `when`         | condition                   | required                       |
| rule         | `effect`       | outcome                     | required                       |
| rule         | `override`     | Boolean                     | default false                  |
| rule         | `rationale`    | non-empty text              | required when override is true |

A condition is exactly one of `all`, `any`, `not`, or a predicate mapping. `all` and `any`
contain condition lists; `not` contains one condition. A predicate contains exactly `fact`,
`operator`, and, only for a binary operator, `value`. Unary operators reject `value`.

| Operator                                            | Arity  | Allowed left type         | Right operand                         |
| --------------------------------------------------- | ------ | ------------------------- | ------------------------------------- |
| `equal`, `not_equal`                                | binary | any declared type or null | typed literal or fact                 |
| `less_than`, `less_than_or_equal`                   | binary | ordered scalar            | same-typed literal or fact            |
| `greater_than`, `greater_than_or_equal`             | binary | ordered scalar            | same-typed literal or fact            |
| `contains`, `not_contains`                          | binary | text or list              | text or list item                     |
| `starts_with`, `ends_with`                          | binary | text                      | text                                  |
| `is_one_of`                                         | binary | scalar                    | non-empty list of same-typed literals |
| `before`, `on_or_after`                             | binary | date or date-time         | same type or tagged reserved operand  |
| `is_expired`, `is_unexpired`                        | unary  | date                      | none                                  |
| `is_absent`, `is_present`, `is_valid`, `is_invalid` | unary  | any path                  | none                                  |

A source operand has exactly one of these forms:

```yaml
- value: { reserved: today }
- value: { reserved: now }
- value: { fact: member.training.valid-until }
- value: { literal: { fact: ordinary-record-field-value } }
- value: today
```

`reserved` accepts only `today` or `now`. `fact` accepts one `FactPath`. `literal` forces its
value to decode as an ordinary typed literal and is REQUIRED for a record literal whose only key
is `fact` or `reserved`. Any untagged scalar, sequence, or mapping is otherwise a literal. Thus
the final `today` above is an ordinary string or enum symbol, never the clock operand. Bare
strings are enum symbols when the expected type is enum and text otherwise. Decimal source
literals MUST be quoted canonical decimal strings. Date, date-time, and duration literals MUST
be quoted to prevent YAML implicit typing. Fact operands resolve against the same vocabulary.

#### Outcomes and scenarios

| Object              | Field               | Type                               | Required/default             |
| ------------------- | ------------------- | ---------------------------------- | ---------------------------- |
| outcome             | `kind`              | four outcome kinds                 | required                     |
| outcome             | `reasons`           | non-empty list of reason           | required                     |
| outcome             | `actions`           | list of invocation                 | default empty                |
| escalation          | `destination`       | `EscalationId`                     | required only for escalation |
| information request | `required_facts`    | non-empty set of `FactPath`        | required only for request    |
| reason              | `code`              | `ReasonCode`                       | required                     |
| reason              | `message`           | non-empty text                     | required                     |
| reason              | `detail`            | non-empty text                     | optional                     |
| invocation          | `action`            | `ActionId`                         | required                     |
| invocation          | `arguments`         | map parameter ID to source operand | default empty                |
| scenario            | `id`                | `ScenarioId`                       | required                     |
| scenario            | `title`             | non-empty text                     | required                     |
| scenario            | `description`       | non-empty text                     | optional                     |
| scenario            | `decision`          | `DecisionId`                       | required                     |
| scenario            | `at`                | UTC RFC 3339 date-time             | required                     |
| scenario            | `given`             | root-to-ergonomic-value map        | required                     |
| scenario            | `expect`            | expected decision                  | required                     |
| scenario            | `tags`              | set of `StableId`                  | default empty                |
| expectation         | `outcome`           | outcome kind                       | required                     |
| expectation         | `determining_rules` | set of `QualifiedRuleId`           | default empty                |
| expectation         | `required_facts`    | set of `FactPath`                  | default empty                |
| expectation         | `reason_codes`      | set of `ReasonCode`                | default empty                |

Outcome-specific fields are rejected on other kinds. Scenario fact values are decoded using the
resolved vocabulary; explicit null is YAML `null`, while an omitted key is absent.

### Normative authored YAML example

The following five listings form one internally consistent normative source fixture.

**Normative `rulery.yaml`:**

```yaml
package:
  id: community-tool-library
  display_name: Community Tool Library
  version: 0.1.0
  language_version: 1
  description: Rules for member access and tool checkout.
  authors:
    - name: Community Tool Library
  tags: [safety, checkout]
semantics:
  timezone: America/New_York
  expiry: inclusive
  missing_facts:
    kind: request_information
  invalid_facts:
    kind: reject_evaluation
  precedence:
    kind: safety_first
imports: []
decisions:
  - id: checkout
    title: Tool checkout eligibility
    asks: Can this member borrow this tool now?
    input_roots: [member, tool]
    default:
      kind: deny
      reasons:
        - code: no-authorizing-rule
          message: No rule authorizes this checkout.
      actions: []
```

**Normative `vocabulary.yaml`:**

```yaml
types:
  account-status:
    kind: enum
    variants:
      active: { display_name: Active, deprecated: false }
      suspended: { display_name: Suspended, deprecated: false }
  training:
    kind: record
    closed: true
    fields:
      completed-at: { type: date, presence: required }
      valid-until: { type: date, presence: required }
  member:
    kind: record
    closed: true
    fields:
      account-status: { type: account-status, presence: required }
      training: { type: training, presence: optional }
      unresolved-damage-reports: { type: integer, presence: required }
  tool-category:
    kind: enum
    variants:
      hand-tool: { display_name: Hand tool, deprecated: false }
      power-tool: { display_name: Power tool, deprecated: false }
      hazardous-equipment:
        { display_name: Hazardous equipment, deprecated: false }
  tool:
    kind: record
    closed: true
    fields:
      category: { type: tool-category, presence: required }
      reserved-for-member-id: { type: text, presence: optional }
roots:
  member: { type: member }
  tool: { type: tool }
terms:
  current-training:
    display_name: Current training
    definition: Training is current through its valid-until date.
    applies_to: [member.training.valid-until]
    examples: [A record valid through 2026-09-16 is current on 2026-09-16.]
    counterexamples:
      [A record valid through 2026-09-15 is expired on 2026-09-16.]
```

**Normative `actions.yaml`:**

```yaml
actions:
  create-checkout-record:
    display_name: Create checkout record
    description: Records an approved checkout obligation.
    parameters: {}
  show-tool-specific-safety-notice:
    display_name: Show tool-specific safety notice
    description: Presents the applicable safety notice.
    parameters: {}
```

**Normative `rules/checkout.yaml`:**

```yaml
decision: checkout
rules:
  - id: deny-suspended-member
    title: Suspended members cannot borrow
    priority: 1000
    when:
      fact: member.account-status
      operator: equal
      value: suspended
    effect:
      kind: deny
      reasons:
        - code: suspended-account
          message: Suspended accounts cannot borrow shared equipment.
      actions: []
  - id: request-training-record
    title: Training is required for power tools
    priority: 900
    when:
      all:
        - fact: tool.category
          operator: equal
          value: power-tool
        - fact: member.training
          operator: is_absent
    effect:
      kind: request_information
      required_facts: [member.training]
      reasons:
        - code: missing-training-record
          message: A current power-tool training record is required.
      actions: []
  - id: deny-expired-training
    title: Expired training blocks power-tool checkout
    priority: 800
    when:
      all:
        - fact: tool.category
          operator: equal
          value: power-tool
        - fact: member.training.valid-until
          operator: is_expired
    effect:
      kind: deny
      reasons:
        - code: expired-training
          message: Power-tool training has expired.
      actions: []
  - id: approve-qualified-checkout
    title: A qualified member may borrow an available power tool
    priority: 100
    when:
      all:
        - fact: member.account-status
          operator: equal
          value: active
        - fact: tool.category
          operator: equal
          value: power-tool
        - fact: member.training.valid-until
          operator: is_unexpired
        - fact: member.unresolved-damage-reports
          operator: equal
          value: 0
        - fact: tool.reserved-for-member-id
          operator: is_absent
    effect:
      kind: approve
      reasons:
        - code: qualified-checkout
          message: The member meets the eligibility requirements.
      actions:
        - action: create-checkout-record
          arguments: {}
        - action: show-tool-specific-safety-notice
          arguments: {}
```

**Normative `scenarios/expired-training-is-denied.yaml`:**

```yaml
id: expired-training-is-denied
title: Expired training prevents power-tool checkout
decision: checkout
at: "2026-09-16T16:00:00.000000000Z"
given:
  member:
    account-status: active
    training:
      completed-at: "2025-01-10"
      valid-until: "2026-09-15"
    unresolved-damage-reports: 0
  tool:
    category: power-tool
expect:
  outcome: deny
  determining_rules:
    - package: community-tool-library
      rule: deny-expired-training
  required_facts: []
  reason_codes: [expired-training]
tags: [safety]
```

## Compilation and package assembly

### Assembly and imports

Only local imports exist in v0.1. The facade owns recursive assembly because it composes the
store and parser ports. Imports are traversed depth-first; siblings are visited in ascending
`PackageId` order. An import is resolved relative to its declaring package and MUST match its
declared package ID and semantic-version requirement.

Exactly one resolved version per `PackageId` is permitted in a graph. A cycle, unresolved
version, duplicate ID with different source content, duplicate ID at different locations,
declared-ID mismatch, or lock mismatch emits an error diagnostic and prevents compilation.
Repeated references to the same package, version, normalized local location, and content hash
are deduplicated.

Every parsed package initially has package-local `SourceKey` values. After resolving the full
graph, assembly sorts packages by `PackageId`, sorts each package's files by normalized relative
path, assigns consecutive globally unique keys starting at zero, rewrites every span, and emits
one merged `SourceMap`. Failure to rewrite any referenced key is an internal error.

**Normative Rust signatures and data declarations:**

```rust
pub enum LockMode {
    Update,
    Frozen,
}

pub struct PackageIntegritySet {
    root: SourceIntegrity,
    imports: BTreeMap<PackageId, SourceIntegrity>,
}

pub struct CompilationInput {
    root: ParsedPackage,
    imports: BTreeMap<PackageId, ParsedPackage>,
    source_map: SourceMap,
    integrity: PackageIntegritySet,
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

pub struct RulebookLockV1 {
    root: LockedRoot,
    language_version: LanguageVersion,
    imports: Vec<LockedImport>,
}

pub struct RulebookLock {
    payload: RulebookLockV1,
}

pub struct LockedRoot {
    package: PackageId,
    version: Version,
    source: NormalizedSourceLocation,
    content_hash: ContentHash,
}

pub struct LockedImport {
    package: PackageId,
    version: Version,
    source: NormalizedSourceLocation,
    content_hash: ContentHash,
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

The facade owns `PackageAssembler<S, P>`. It implements `PackageAssemblyService` exactly when
`S: PackageStore` and `P: SourceParser`. Assembly obtains only bytes and integrity from the store,
parses through `P`, validates the graph, and performs global source-key remapping in the facade.

`Update` accepts a missing or stale lock, resolves the complete graph deterministically, and
returns a complete replacement lock. Assembly MUST NOT write it. Only the `rulery lock` use case
may call `write_lock`.

`Frozen` requires a lock. Root package ID, root version, language version, root source location,
root content hash, and every transitive import's ID, resolved version, normalized source
location, and content hash MUST match exactly. Missing, stale, additional, or absent entries are
errors; no replacement lock is produced. A package with no imports still has a lock containing
its root identity, language version, root content hash, and an empty import set.

The lock contains the complete transitive closure, sorted by `PackageId`; it does not merely
record direct imports. The lock envelope is defined below.

Imported scenarios are parsed to validate their documents but are not assembled, compiled, or
run. `PackageAssembly::source_scenarios` contains only remapped root-package scenarios. Imported
rules, vocabulary, actions, and decisions remain available through qualified resolution.

### Compilation

The compiler validates source, resolves imports and vocabulary references, type-checks,
normalizes, lowers to IR, computes static specificity, records provenance, and hashes the
compiled body. Compilation MUST be deterministic for identical `CompilationInput`.

**Normative Rust signatures and data declarations:**

```rust
pub struct CompiledPackage {
    payload: CompiledPackageV1,
}

pub struct CompilationOutput {
    package: Option<CompiledPackage>,
    diagnostics: DiagnosticReport,
}

pub trait PolicyCompiler: Send + Sync {
    fn compile(&self, input: &CompilationInput) -> CompilationOutput;
}
```

`package` MUST be `None` when any error diagnostic exists. The compiled package MUST be
self-sufficient for evaluation: it retains resolved vocabulary, source map, complete import
integrity, compiler identity, language version, and content hash. The engine MUST need no source
parser, filesystem, or vocabulary service.

## Evaluation semantics

### Four-valued truth

`Unknown` means required evidence is absent or semantically indeterminate. `Invalid` means
evidence exists but violates its declared type or operator contract. Logical operations use the
following complete tables.

| `AND`   | True    | False | Unknown | Invalid |
| ------- | ------- | ----- | ------- | ------- |
| True    | True    | False | Unknown | Invalid |
| False   | False   | False | False   | False   |
| Unknown | Unknown | False | Unknown | Invalid |
| Invalid | Invalid | False | Invalid | Invalid |

| `OR`    | True | False   | Unknown | Invalid |
| ------- | ---- | ------- | ------- | ------- |
| True    | True | True    | True    | True    |
| False   | True | False   | Unknown | Invalid |
| Unknown | True | Unknown | Unknown | Invalid |
| Invalid | True | Invalid | Invalid | Invalid |

| A       | `NOT A` |
| ------- | ------- |
| True    | False   |
| False   | True    |
| Unknown | Unknown |
| Invalid | Invalid |

Short-circuiting MAY avoid computation but MUST NOT omit required trace nodes. Omitted nodes are
recorded as `NotEvaluated` with the decisive parent result.

### Predicate behavior

The following table is normative. `valid` means the operand conforms to the resolved vocabulary.
An incompatible literal is a compile error; an incompatible runtime fact is `Invalid`.

| Predicate category     | Absent fact | Null fact                        | Malformed fact | Valid fact                     |
| ---------------------- | ----------- | -------------------------------- | -------------- | ------------------------------ |
| `is_absent`            | True        | False                            | False          | False                          |
| `is_present`           | False       | True                             | True           | True                           |
| `is_valid`             | Unknown     | False                            | False          | True                           |
| `is_invalid`           | Unknown     | False                            | True           | False                          |
| equality or inequality | Unknown     | valid only against explicit null | Invalid        | typed comparison               |
| ordering or range      | Unknown     | Invalid                          | Invalid        | typed comparison               |
| text operation         | Unknown     | Invalid                          | Invalid        | Unicode scalar-value operation |
| list operation         | Unknown     | Invalid unless declared list     | Invalid        | typed list operation           |
| enum operation         | Unknown     | Invalid                          | Invalid        | same declared enum type        |
| temporal operation     | Unknown     | Invalid                          | Invalid        | operation-specific comparison  |

Additional rules are normative:

- `null == null` is True; `null != null` is False. Comparing null with non-null is False for
  equality and True for inequality. Other operations on null are Invalid.
- Equality is structural and type-strict. Integer and decimal are not implicitly equal. Records
  compare by identical key sets and recursively equal values. Lists compare in order.
- Ordering is defined only for two integers, two decimals, two texts, two dates, two UTC
  instants, or two durations. Text ordering is Unicode scalar-value lexicographic ordering with
  no locale or normalization. Boolean, enum, list, record, and null ordering is Invalid.
- `contains` on text tests a contiguous Unicode scalar-value subsequence. On a list it tests for
  a type-strict structurally equal element. `not_contains` is logical negation of a decisive
  `contains`; it preserves Unknown and Invalid.
- `starts_with` and `ends_with` accept text operands only and compare Unicode scalar values.
- `is_one_of` is an OR of type-strict equality against a non-empty literal list. An empty source
  list is a compile error. Enum operands and symbols MUST share one resolved enum type.
- `before` and `on_or_after` accept two dates or two UTC instants. `today` has date type and
  `now` has UTC-instant type. Mixed date/date-time comparison is Invalid.
- `is_expired` and `is_unexpired` accept a date fact and no right operand. For inclusive expiry,
  expired is `fact < today`; for exclusive expiry, expired is `fact <= today`. `is_unexpired` is
  the decisive logical negation and preserves Unknown and Invalid.
- Date expiry is governed by inherited package `DateExpiryPolicy`, not by general ordering.

### Missing and invalid strategies

**Normative Rust data declarations:**

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

pub enum PrecedenceModel {
    SafetyFirst,
    PriorityFirst,
    Explicit(ExplicitPrecedence),
}

pub struct ExplicitPrecedence {
    outcome_ranks: BTreeMap<OutcomeKind, u16>,
    primary_dimension: PrecedenceDimension,
}

pub enum PrecedenceDimension {
    Outcome,
    Priority,
}
```

Strategies are compiled decision semantics. Runtime callers MUST NOT override them with flags or
booleans. `ClosedWorldFalse` converts only absent-fact predicate results to False before logical
composition; it never converts null or malformed evidence. `RequestInformation` creates an
information-request candidate containing all decision-relevant absent paths. Missing-fact
`Escalate` creates an escalation candidate at its declared destination.

`RejectEvaluation` returns `EvaluationError::InvalidFact` when malformed evidence is
decision-relevant. `PreserveInvalid` retains Invalid for relevance and default processing.
Invalid-fact `Escalate` creates an escalation candidate. Every strategy application is recorded
in the trace.

Strategy candidates use the same semantic keys as authored candidates. Their priority is the
maximum priority among decision-relevant unresolved rules, their specificity is the maximum
specificity among those rules at that priority, and their override rank is the maximum override
rank among those rules at the preceding components. Missing paths and invalid facts are
aggregated as sorted sets by canonical path. One candidate is created per strategy outcome and,
for escalation, per destination. Distinct greatest-key destinations conflict.

Generated reason codes and messages are exact:

| Strategy result         | Reason code               | Message                                |
| ----------------------- | ------------------------- | -------------------------------------- |
| Request information     | `missing-required-facts`  | `Decision-relevant facts are missing.` |
| Missing-fact escalation | `missing-facts-escalated` | `Decision-relevant facts are missing.` |
| Invalid-fact escalation | `invalid-facts-escalated` | `Decision-relevant facts are invalid.` |

The request candidate contains the union of all decision-relevant missing paths. Strategy
candidates participate in ordinary greatest-key selection. Equal-key identical strategy outcomes
merge their reasons, required facts, and actions as canonical sets. Equal-key non-identical
outcomes or destinations produce a runtime-conflict trace. `RejectEvaluation` takes effect before
candidate selection when any invalid fact is decision-relevant.

An unresolved rule is decision-relevant exactly when at least one completion of its unresolved
operands can give it a semantic precedence key that is greater than the best decisive key, or
equal to that key with an incompatible instantiated outcome. If no completion can alter the
selected outcome or create an unresolved conflict, it is not decision-relevant. The engine MUST
retain irrelevant unresolved facts in the trace but MUST NOT let them displace the decisive
candidate. Static analysis MUST use the same relevance test.

### Outcomes

Reasons and requested facts are validated non-empty collections. Reasons MUST have non-empty
stable codes and non-empty human messages. Required facts MUST contain at least one `FactPath`.

**Normative Rust data declarations:**

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

Actions are declarative obligations; v0.1 MUST NOT execute them.

### Precedence and conflict

All tuple components compare lexicographically in descending order. Default outcome ranks are
`deny = 4000`, `escalate = 3000`, `request_information = 2000`, and `approve = 1000`.
`override_rank` is `1` for an authored explicit override and `0` otherwise.

Specificity is computed statically as:

```text
atomic predicate count
+ sum of referenced fact-path segment counts
+ 2 * count of temporal and explicit range predicates
```

Logical container nodes, negation, constants, and duplicate references add zero. Counts use
checked `u32` arithmetic; overflow is a compile error. Explicit override is not part of
specificity because it has its own tuple component.

Exact semantic keys are:

```text
SafetyFirst  = (outcome_rank, priority, specificity, override_rank)
PriorityFirst = (priority, outcome_rank, specificity, override_rank)
Explicit(primary = outcome)
             = (explicit_outcome_rank, priority, specificity, override_rank)
Explicit(primary = priority)
             = (priority, explicit_outcome_rank, specificity, override_rank)
```

An explicit rank map MUST contain each of the four outcome kinds exactly once and all ranks MUST
be distinct. Higher signed priority wins. `RuleId` and `QualifiedRuleId` are excluded from
semantic precedence and are used only to sort presentation output.

All true candidates with the greatest semantic key are determining candidates. If they
instantiate identical outcomes, the result is that outcome and all their qualified rule IDs are
determining rules. If greatest-key candidates instantiate incompatible outcomes, evaluation
returns a runtime-conflict trace with no outcome; the engine MUST NOT break the tie by rule ID,
source order, map order, or package order. Static conflict analysis MUST report the conflict
whenever its analysis is complete for the relevant partition.

### Evaluation algorithm and trace

Evaluation MUST:

1. locate the decision in the compiled package;
2. validate supplied facts against retained resolved vocabulary;
3. evaluate every rule and every expression node under the tables above;
4. apply compiled missing and invalid strategies only to decision-relevant uncertainty;
5. instantiate true candidates and strategy candidates;
6. compare exact semantic keys;
7. select determining candidates or return a partial trace containing `ConflictTrace`;
8. use the declared default only when no candidate or relevant strategy candidate exists; and
9. emit a deterministic trace.

**Normative trace data declarations:**

```rust
pub struct DecisionTraceV1 {
    evaluation_id: EvaluationId,
    package: PackageId,
    package_version: Version,
    decision: DecisionId,
    evaluated_at: UtcInstant,
    timezone: PolicyTimeZone,
    timezone_database: TimeZoneDatabaseIdentity,
    package_hash: ContentHash,
    facts_hash: ContentHash,
    compiler: CompilerIdentity,
    language_version: LanguageVersion,
    outcome: Option<Outcome>,
    determining_rules: Vec<QualifiedRuleId>,
    superseded_rules: Vec<SupersededRuleTrace>,
    rule_traces: Vec<RuleTrace>,
    missing_facts: BTreeSet<FactPath>,
    invalid_facts: Vec<InvalidFactTrace>,
    strategy_applications: Vec<StrategyApplicationTrace>,
    conflict: Option<ConflictTrace>,
    trace_hash: ContentHash,
}

pub struct DecisionTrace {
    payload: DecisionTraceV1,
}

pub struct RuleTrace {
    rule: QualifiedRuleId,
    result: Truth,
    selected: bool,
    relevance: DecisionRelevance,
    condition: ExpressionTrace,
    candidate: Option<CandidateTrace>,
    source_span: Span,
}

pub enum ExpressionTrace {
    All { result: Truth, children: Vec<ExpressionTrace>, span: Span },
    Any { result: Truth, children: Vec<ExpressionTrace>, span: Span },
    Not { result: Truth, child: Box<ExpressionTrace>, span: Span },
    Predicate(PredicateTrace),
    Constant { result: Truth, value: bool, span: Span },
    NotEvaluated { result: Truth, reason: ShortCircuitReason, span: Span },
}

pub struct PredicateTrace {
    result: Truth,
    operator: Operator,
    lhs: EvaluatedOperand,
    rhs: Option<EvaluatedOperand>,
    span: Span,
}

pub enum EvaluatedOperand {
    Value { value: Value, source_path: Option<FactPath> },
    Missing { path: FactPath },
    Invalid { path: Option<FactPath>, error: FactValidationError },
    ClockValue { value: Value, name: ClockOperand },
}

pub struct CandidateTrace {
    outcome: Outcome,
    semantic_key: SemanticPrecedenceKey,
}

pub enum SemanticPrecedenceKey {
    SafetyFirst(u16, i32, u32, u8),
    PriorityFirst(i32, u16, u32, u8),
    ExplicitOutcome(u16, i32, u32, u8),
    ExplicitPriority(i32, u16, u32, u8),
}

pub struct SupersededRuleTrace {
    rule: QualifiedRuleId,
    reason: SupersessionReason,
}

pub enum SupersessionReason {
    LowerSemanticKey,
    EqualKeyIdenticalOutcome,
    IrrelevantUnknown,
    IrrelevantInvalid,
}

pub struct InvalidFactTrace {
    path: Option<FactPath>,
    error: FactValidationError,
    span: Option<Span>,
}

pub struct StrategyApplicationTrace {
    strategy: StrategyKind,
    relevant_paths: BTreeSet<FactPath>,
    candidate: Option<CandidateTrace>,
}

pub struct ConflictTrace {
    key: SemanticPrecedenceKey,
    rules: Vec<QualifiedRuleId>,
    outcomes: Vec<Outcome>,
}

pub enum DecisionRelevance {
    Decisive,
    RelevantUnresolved,
    Irrelevant,
}

pub enum ShortCircuitReason { DecisiveAnd, DecisiveOr }
pub enum ClockOperand { Today, Now }
pub enum StrategyKind { Missing, Invalid }
pub enum Operator { SourceEquivalent(SourceOperator) }

pub enum EvaluationError {
    UnknownDecision(DecisionId),
    InvalidFact(FactValidationError),
    TimeZone(TimeZoneError),
    Internal(DiagnosticCode),
}

pub enum TraceDetail {
    Complete,
    Compact,
}

pub struct EvaluationContext {
    clock: Arc<dyn Clock>,
    time_zones: Arc<dyn TimeZoneDatabase>,
    trace_detail: TraceDetail,
}
```

A successful trace has `outcome = Some`, `conflict = None`, and at least one determining rule or
an instantiated default. A runtime-conflict trace has `outcome = None`, `conflict = Some`, no
determining rules, and every tied candidate in `ConflictTrace`. Every other combination is
invalid. Runtime conflict is returned as `Ok(DecisionTrace)` so callers retain complete evidence;
the facade converts that typed conflict into `RUL205` with `ConflictEvidence`, preserving
`Evaluator` as the producer.

The `outcome` and `conflict` fields are always present in `DecisionTraceV1` JSON. The absent side
serializes as JSON null; these two fields are exceptions to the general omitted-Option rule.

The trace MUST contain every rule in ascending `QualifiedRuleId` presentation order, every
expression node, actual operand states, all missing and invalid facts, candidate keys,
determining and superseded rules with reasons, strategy applications, conflict participants,
clock and timezone identities, package/compiler/language identities, and hashes. Sensitive facts
MUST NOT be persisted unless explicitly requested.

`DecisionTrace` is the validated in-memory domain wrapper around `DecisionTraceV1`; envelope
serialization exposes its payload. `TraceDetail::Compact` may omit `NotEvaluated` descendants and
non-determining false expression descendants, but it MUST retain every top-level rule trace and
all identity, uncertainty, candidate, strategy, conflict, and outcome fields. Hashing always uses
the complete logical trace, so both detail modes have the same `trace_hash`.

## Time semantics

Clocks provide UTC instants only. Package semantics supply a validated canonical IANA timezone
and date-expiry policy inherited by every decision. Local dates are derived by converting the UTC
instant with the supplied timezone database, including historical and current daylight-saving
transitions.

**Normative Rust signatures and data declarations:**

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

pub enum DateExpiryPolicy {
    Inclusive,
    Exclusive,
}

pub struct TimeSemantics {
    timezone: PolicyTimeZone,
    expiry: DateExpiryPolicy,
}

pub enum TimeZoneError {
    UnknownZone(PolicyTimeZone),
    DataUnavailable(TimeZoneDatabaseIdentity),
    OutOfRange(UtcInstant),
}
```

`today` is the derived policy-local date. `now` is the unmodified UTC instant. With inclusive
expiry, an item with expiry date `D` is valid when `today <= D`. With exclusive expiry it is
valid when `today < D`. Ambiguous or nonexistent local wall times never arise in this conversion
because the input is an instant. Unknown timezone names, unavailable timezone data, and failed
conversion are evaluation errors. Trace reproducibility requires the database identity.

## Canonical serialization and hashing

### Canonical payloads

Canonical JSON MUST be RFC 8785 JSON Canonicalization Scheme bytes produced by `serde_jcs`.
Ordinary `serde_json::to_vec` is not a hashing primitive. Maps have string keys and use JCS key
order. Lists retain semantic order. Sets serialize as arrays sorted lexicographically by each
element's complete canonical byte encoding. Optional absent fields are omitted; explicit null is
serialized only where null is semantic. Strings use RFC 8785 escaping.

Because JCS numbers cannot represent all Rulery integers exactly, signed and unsigned integers
inside hashed payloads serialize as canonical base-10 strings. Decimal, date, instant, duration,
UUID, stable ID, enum symbol, timezone, and hash encodings are those in Core contracts. These
rules apply recursively.

### Framing, integrity, and domains

Hashing uses BLAKE3-256. `hash_parts` feeds these bytes in exact order:

1. the domain byte length as unsigned 64-bit big-endian;
2. the domain UTF-8 bytes; and
3. for each part, its byte length as unsigned 64-bit big-endian followed by its bytes.

No separator, terminator, BOM, or newline is added.

| Domain              | Exact UTF-8 domain bytes     |
| ------------------- | ---------------------------- |
| `CompiledPackageV1` | `rulery.compiled-package.v1` |
| `CaseFactsV1`       | `rulery.case-facts.v1`       |
| `DecisionTraceV1`   | `rulery.decision-trace.v1`   |
| `EvaluationV1`      | `rulery.evaluation.v1`       |

**Normative Rust signature:**

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

pub type EvaluationId = ContentHash;
```

Exact part order is:

| Hash             | Ordered parts                                                                                                                               |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| Compiled package | JCS bytes of `CompiledPackageV1` with `content_hash` omitted                                                                                |
| Case facts       | JCS bytes of `CaseFactsV1`                                                                                                                  |
| Decision trace   | JCS bytes of `DecisionTraceV1` with `trace_hash` omitted                                                                                    |
| Evaluation ID    | raw 32-byte package hash; decision ID UTF-8; raw 32-byte facts hash; signed i128 big-endian UTC nanoseconds; timezone database identity JCS |

`EvaluationV1` names those five logical fields, but its hash is the ordered five-part framing in
the table, not JCS of the record.

Only payloads are hashed. The seven envelope `schema` tags and the `payload` wrapper are never
hashed. A compiled package body conceptually includes metadata, resolved vocabulary, decisions,
actions, source catalog, compiler identity, language version, and import integrity; JCS controls
object-key order. Display prose and renderer output MUST NOT enter semantic hashes.

`EvaluationId` is exactly the `EvaluationV1` `ContentHash`; there is no UUID truncation or
conversion.

Source integrity is byte integrity, not semantic hashing. Each document hash is BLAKE3-256 over
its exact bytes, including line endings and without framing. A bundle hash is BLAKE3-256 over,
for each document in ascending normalized path order: u64 big-endian path length, path UTF-8,
u64 big-endian byte length, and exact bytes. No domain, count, separator, or envelope is added.
The store MUST recompute document and bundle hashes after loading and before parsing. Locks store
the bundle hash.

### Fixed conformance vectors

These vectors were computed with `b3sum 1.8.7`. Hex is the complete framed input. The first
three use one part containing the JCS bytes `{}` (`7b7d`). The Evaluation vector uses five parts:
32 zero bytes, UTF-8 `checkout`, 32 bytes of `11`, signed i128 zero, and JCS
`{"implementation":"test","version":"1"}`.

```text
CompiledPackageV1
hex: 000000000000001a72756c6572792e636f6d70696c65642d7061636b6167652e763100000000000000027b7d
hash: blake3:1c6402278430173b5964f65d31c1ec06a9765c13c6e34bad762fcdee8ffbd6c8

CaseFactsV1
hex: 000000000000001472756c6572792e636173652d66616374732e763100000000000000027b7d
hash: blake3:fa785b19b5601f57afd5548934d2c1e4b20bc1a8ab25b89e6bece83d041d31aa

DecisionTraceV1
hex: 000000000000001872756c6572792e6465636973696f6e2d74726163652e763100000000000000027b7d
hash: blake3:4773d0e94079df21090915deecb6b2f41327122e6b479afb63d5d082f174aced

EvaluationV1
hex: 000000000000001472756c6572792e6576616c756174696f6e2e7631000000000000002000000000000000000000000000000000000000000000000000000000000000000000000000000008636865636b6f75740000000000000020111111111111111111111111111111111111111111111111111111111111111100000000000000100000000000000000000000000000000000000000000000277b22696d706c656d656e746174696f6e223a2274657374222c2276657273696f6e223a2231227d
hash: blake3:4c11eb70b89e8543605015c325e309fde68d9007c2086b97bad892693cae9279
```

Implementations MUST reproduce all four vectors byte for byte and MUST add package, facts, trace,
source-document, and source-bundle vectors using non-empty production-shaped payloads.

## Versioned envelopes

These seven envelopes are the complete v0.1 persisted and interchange set. Each JSON object has
exactly `schema` and `payload`; unknown envelope fields and unknown schema tags are rejected.

| Artifact          | Schema tag                    | Required v1 payload fields                                                                                    |
| ----------------- | ----------------------------- | ------------------------------------------------------------------------------------------------------------- |
| Compiled package  | `rulery.compiled-package/v1`  | metadata, vocabulary, decisions, actions, source_map, integrity, compiler, language_version, content_hash     |
| Decision trace    | `rulery.decision-trace/v1`    | every field of `DecisionTraceV1` above                                                                        |
| Diagnostic report | `rulery.diagnostic-report/v1` | diagnostics, producer_identity, language_version                                                              |
| Rulebook lock     | `rulery.rulebook-lock/v1`     | root, language_version, imports                                                                               |
| Scenario result   | `rulery.scenario-result/v1`   | package_hash, scenario, status, failures, trace                                                               |
| Analysis report   | `rulery.analysis-report/v1`   | package_hash, options, completeness, diagnostics, reachability, overlaps, coverage, witnesses, semantic_diffs |
| Decision table    | `rulery.decision-table/v1`    | package_hash, decision, columns, rows, source_references                                                      |

Every V1 payload and nested wire struct uses `deny_unknown_fields`. Tagged wire enums reject
unknown tags and fields. Missing required fields are errors. Optional fields are omitted when
absent; collections are present even when empty unless their declaration below is `Option`.

**Normative V1 payload declarations:**

```rust
pub struct CompiledPackageV1 {
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

pub struct DiagnosticReportV1 {
    diagnostics: Vec<Diagnostic>,
    producer_identity: ProducerIdentity,
    language_version: LanguageVersion,
}

pub struct ScenarioResultV1 {
    package_hash: ContentHash,
    scenario: ScenarioId,
    status: ScenarioStatus,
    failures: Vec<ScenarioFailure>,
    trace: Option<DecisionTraceV1>,
}

pub struct AnalysisReportV1 {
    package_hash: ContentHash,
    options: AnalysisOptions,
    completeness: AnalysisCompleteness,
    diagnostics: DiagnosticReportV1,
    reachability: Vec<RuleReachability>,
    overlaps: Vec<RuleOverlap>,
    coverage: Vec<DecisionCoverage>,
    witnesses: Vec<WitnessCase>,
    semantic_diffs: Vec<PolicyDiff>,
}

pub struct DecisionTableV1 {
    package_hash: ContentHash,
    decision: DecisionId,
    columns: Vec<DecisionColumn>,
    rows: Vec<DecisionRow>,
    source_references: BTreeMap<RuleId, Vec<Span>>,
}

pub enum ScenarioStatus {
    Passed,
    Failed,
    Invalid,
    Error,
}

pub struct ScenarioFailure {
    field: ScenarioExpectationField,
    expected: serde_json::Value,
    actual: serde_json::Value,
}

pub enum ScenarioExpectationField {
    Outcome,
    DeterminingRules,
    RequiredFacts,
    ReasonCodes,
}

pub struct DecisionColumn {
    id: StableId,
    label: String,
    role: ColumnRole,
}

pub struct DecisionRow {
    rule: QualifiedRuleId,
    cells: Vec<DecisionCell>,
    outcome: OutcomeKind,
}

pub enum ColumnRole {
    Condition,
    Outcome,
    Reason,
    Action,
    Priority,
}

pub enum DecisionCell {
    Any,
    Equal(Value),
    NotEqual(Value),
    Range {
        minimum: Option<Value>,
        maximum: Option<Value>,
        inclusive_minimum: bool,
        inclusive_maximum: bool,
    },
    Present,
    Absent,
    Derived(String),
}
```

`RulebookLockV1` and `DecisionTraceV1` are declared in their owning sections and are equally
complete V1 payload declarations. `CompiledPackage` is a validated in-memory wrapper around
`CompiledPackageV1`; `RulebookLock` and `DecisionTrace` use the same wrapper rule. Wrappers expose
read-only payload access and validate all invariants during construction and deserialization.

**Normative Rust envelope declarations:**

```rust
#[serde(tag = "schema", content = "payload")]
pub enum CompiledPackageEnvelope {
    #[serde(rename = "rulery.compiled-package/v1")]
    V1(CompiledPackageV1),
}

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

## Diagnostics

### Model and invariants

`DiagnosticCode` is an owned validated newtype matching `RUL[0-9]{3}`. Its custom
`Deserialize` MUST call `DiagnosticCode::new`. Well-formed unknown codes remain representable
for forward compatibility. Known meanings come only from the registry below.

Numeric ranges allocate space for diagnostic families:

| Range           | Family          |
| --------------- | --------------- |
| `RUL000-RUL049` | Syntax          |
| `RUL050-RUL099` | Vocabulary      |
| `RUL100-RUL149` | Clarity         |
| `RUL150-RUL199` | Semantics       |
| `RUL200-RUL249` | RuleInteraction |
| `RUL250-RUL299` | Coverage        |
| `RUL300-RUL349` | Scenario        |
| `RUL350-RUL399` | Change          |
| `RUL400-RUL449` | Export          |
| `RUL450-RUL499` | Reserved        |
| `RUL500-RUL549` | Evidence        |
| `RUL550-RUL899` | Reserved        |
| `RUL900-RUL999` | Internal        |

`DiagnosticCode::family` returns the registry family for a known code. `RUL000` is well formed
but unassigned, and any unknown code below `RUL900` returns `Reserved`; every unknown
`RUL900-RUL999` code returns `Internal`.

**Normative Rust data declarations:**

```rust
pub enum Severity {
    Error,
    Warning,
    Advice,
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

pub enum EvidenceRequirement {
    None,
    Provenance,
    Witness,
    Trace,
    BehaviorChange,
    AnalysisLimit,
    Proof,
    Conflict,
    Properties,
    OneOf(Vec<EvidenceRequirement>),
}

pub enum DiagnosticProducer {
    Store,
    Parser,
    Compiler,
    Evaluator,
    Analyzer,
    ScenarioCompiler,
    ScenarioRunner,
    Renderer,
    Facade,
}

pub enum Suppressibility {
    Never,
    WithJustification,
    WithJustificationAndExpiry,
}

pub enum FindingConfidence {
    Proven,
    Witnessed,
    Heuristic,
    Inconclusive,
}

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

pub struct DiagnosticCode {
    value: String,
}

pub struct DiagnosticDefinition {
    code: DiagnosticCode,
    default_severity: Severity,
    title: &'static str,
    producer: DiagnosticProducer,
    suppressibility: Suppressibility,
    required_evidence: EvidenceRequirement,
}

impl DiagnosticCode {
    pub fn new(value: impl Into<String>) -> Result<Self, DiagnosticCodeError>;
    pub fn as_str(&self) -> &str;
    pub fn family(&self) -> DiagnosticFamily;
}

pub fn diagnostic_definition(
    code: &DiagnosticCode,
) -> Option<&'static DiagnosticDefinition>;

pub enum LabelStyle {
    Primary,
    Secondary,
    Context,
}

pub struct DiagnosticLabel {
    span: Span,
    style: LabelStyle,
    message: Option<String>,
}

pub enum NoteKind {
    Rationale,
    Semantics,
    Limitation,
    Compatibility,
    Security,
    Privacy,
    Fairness,
    Performance,
}

pub struct DiagnosticNote {
    kind: NoteKind,
    message: String,
}

pub enum HelpPriority {
    Primary,
    Alternative,
    Educational,
}

pub struct HelpItem {
    priority: HelpPriority,
    message: String,
}

pub enum DiagnosticEvidence {
    Witness(WitnessEvidence),
    Trace(TraceEvidence),
    BehaviorChange(BehaviorChangeEvidence),
    Provenance(ProvenanceEvidence),
    AnalysisLimit(AnalysisLimitEvidence),
    Proof(ProofEvidence),
    Conflict(ConflictEvidence),
    Properties(BTreeMap<String, serde_json::Value>),
}

pub struct WitnessEvidence {
    witness_id: StableId,
    summary: String,
    facts: serde_json::Value,
    facts_hash: ContentHash,
    decision: DecisionId,
    matched_rules: Vec<QualifiedRuleId>,
    outcome_kinds: Vec<OutcomeKind>,
}

pub struct TraceEvidence {
    evaluation_id: EvaluationId,
    summary: String,
    trace_hash: ContentHash,
}

pub struct BehaviorChangeEvidence {
    before_outcome: OutcomeKind,
    after_outcome: OutcomeKind,
    before_rules: Vec<QualifiedRuleId>,
    after_rules: Vec<QualifiedRuleId>,
    witness_facts_hash: ContentHash,
}

pub struct ProvenanceEvidence {
    package_hash: ContentHash,
    compiler_version: Version,
    language_version: LanguageVersion,
}

pub struct ProofEvidence {
    method: String,
    constraints_hash: ContentHash,
    subjects: DiagnosticSubjects,
}

pub struct ConflictEvidence {
    evaluation_id: EvaluationId,
    trace_hash: ContentHash,
    semantic_key: Vec<String>,
    rules: Vec<QualifiedRuleId>,
    outcomes: Vec<OutcomeKind>,
}

pub enum AnalysisPhase {
    DomainConstruction,
    ConstraintSolving,
    WitnessSearch,
    CoverageEnumeration,
    DiffEnumeration,
}

pub struct AnalysisLimitEvidence {
    phase: AnalysisPhase,
    limit_name: String,
    configured_limit: u64,
    observed_value: u64,
    unresolved_paths: Vec<FactPath>,
}

pub enum FixApplicability {
    MachineApplicable,
    RequiresReview,
    HasPlaceholders,
}

pub struct SuggestedFix {
    id: StableId,
    title: String,
    applicability: FixApplicability,
    edits: Vec<TextEdit>,
}

pub struct TextEdit {
    span: Span,
    replacement: String,
}

pub struct DiagnosticProperties {
    producer: DiagnosticProducer,
    suppressibility: Suppressibility,
    confidence: FindingConfidence,
    subjects: DiagnosticSubjects,
    tags: Vec<String>,
}

pub struct DiagnosticSubjects {
    package_id: Option<PackageId>,
    decision_id: Option<DecisionId>,
    rule_ids: Vec<QualifiedRuleId>,
    scenario_id: Option<ScenarioId>,
    fact_paths: Vec<FactPath>,
}
```

`DiagnosticCode::new` accepts exactly `RUL` followed by three ASCII digits. Its custom
`Deserialize` implementation MUST deserialize a string and call `DiagnosticCode::new`; it MUST
NOT bypass validation. `diagnostic_definition` returns the immutable known-code registry entry
or `None` for a well-formed forward-compatible unknown code.

`None` requires an empty evidence list. `Properties` requires the `Properties` evidence variant
and does not require compiled identities. Parser and store diagnostics emitted before compilation
MUST use only those two requirements. `ProvenanceEvidence` is reserved for stages that already
have package, compiler, and language identities. `Proof` requires `ProofEvidence`; `Conflict`
requires `ConflictEvidence` referencing the partial conflict trace.

Invariant-bearing fields are private and constructed through validated builders. A diagnostic
MUST use its registry producer and default severity unless the definition explicitly permits a
severity override. Suppression MUST follow the registry. Internal errors and syntax errors are
never suppressible.

Every analysis diagnostic records confidence. `Proven` requires a sound proof; `Witnessed`
requires replayable evidence; `Heuristic` cannot be promoted by `--deny-warnings`;
`Inconclusive` requires `AnalysisLimitEvidence`. Evidence uses typed IDs when available.
`WithJustification` requires a non-empty authored reason. `WithJustificationAndExpiry` also
requires a future policy-local expiry date. `MachineApplicable` means syntax-safe only unless the
registry explicitly guarantees semantic preservation.

Edits within one fix MUST be non-empty, target known source spans, and not overlap. Two edits
overlap when they address the same `SourceKey` and their half-open ranges intersect; two empty
insertions at the same offset also overlap. Edits are applied by source and descending start
offset.

### Complete v0.1 known-code registry

The following rows are the complete known v0.1 registry. All other well-formed codes are unknown.
`Evidence` is REQUIRED evidence for emission; `none` means labels and properties suffice.

| Code / constant                        | Title                          | Family          | Default | Producer         | Suppressibility            | Evidence        |
| -------------------------------------- | ------------------------------ | --------------- | ------- | ---------------- | -------------------------- | --------------- |
| `RUL001 SYNTAX_INVALID`                | Invalid source syntax          | Syntax          | Error   | Parser           | Never                      | none            |
| `RUL002 DUPLICATE_DECLARATION`         | Duplicate declaration          | Syntax          | Error   | Compiler         | Never                      | provenance      |
| `RUL003 UNKNOWN_SYMBOL`                | Unknown symbol                 | Syntax          | Error   | Compiler         | Never                      | provenance      |
| `RUL004 INVALID_FACT_PATH`             | Invalid fact path              | Syntax          | Error   | Compiler         | Never                      | provenance      |
| `RUL005 TYPE_MISMATCH`                 | Type mismatch                  | Syntax          | Error   | Compiler         | Never                      | provenance      |
| `RUL006 SOURCE_RESOURCE_LIMIT`         | Source resource limit exceeded | Syntax          | Error   | Parser           | Never                      | properties      |
| `RUL100 UNDEFINED_OPERATIONAL_TERM`    | Undefined operational term     | Clarity         | Warning | Compiler         | WithJustificationAndExpiry | provenance      |
| `RUL101 TEMPORAL_TERM_UNDEFINED`       | Temporal term undefined        | Clarity         | Error   | Compiler         | Never                      | provenance      |
| `RUL102 MISSING_OUTCOME_REASON`        | Missing outcome reason         | Clarity         | Error   | Compiler         | Never                      | provenance      |
| `RUL150 UNKNOWN_FACT_POLICY_UNHANDLED` | Unknown-fact policy unhandled  | Semantics       | Error   | Compiler         | Never                      | provenance      |
| `RUL151 DEFAULT_OUTCOME_MISSING`       | Default outcome missing        | Semantics       | Error   | Compiler         | Never                      | provenance      |
| `RUL152 PRECEDENCE_AMBIGUOUS`          | Precedence is ambiguous        | Semantics       | Error   | Compiler         | Never                      | provenance      |
| `RUL153 TIMEZONE_UNAVAILABLE`          | Timezone unavailable           | Semantics       | Error   | Evaluator        | Never                      | properties      |
| `RUL200 CONFLICTING_RULES`             | Conflicting rules              | RuleInteraction | Error   | Analyzer         | WithJustificationAndExpiry | witness         |
| `RUL201 SHADOWED_APPROVAL`             | Shadowed approval              | RuleInteraction | Warning | Analyzer         | WithJustificationAndExpiry | witness         |
| `RUL202 UNREACHABLE_RULE`              | Unreachable rule               | RuleInteraction | Warning | Analyzer         | WithJustification          | proof           |
| `RUL203 REDUNDANT_RULE`                | Redundant rule                 | RuleInteraction | Warning | Analyzer         | WithJustification          | witness         |
| `RUL204 UNDECLARED_OVERRIDE`           | Undeclared override            | RuleInteraction | Warning | Analyzer         | WithJustificationAndExpiry | witness         |
| `RUL205 RUNTIME_CONFLICT`              | Runtime conflict               | RuleInteraction | Error   | Evaluator        | Never                      | conflict        |
| `RUL250 UNCOVERED_COMBINATION`         | Uncovered combination          | Coverage        | Warning | Analyzer         | WithJustificationAndExpiry | witness         |
| `RUL251 UNHANDLED_ENUM_VARIANT`        | Unhandled enum variant         | Coverage        | Warning | Analyzer         | WithJustificationAndExpiry | witness         |
| `RUL252 UNHANDLED_MISSING_FACT`        | Unhandled missing fact         | Coverage        | Warning | Analyzer         | WithJustificationAndExpiry | witness         |
| `RUL253 TEMPORAL_BOUNDARY_UNCOVERED`   | Temporal boundary uncovered    | Coverage        | Warning | Analyzer         | WithJustificationAndExpiry | witness         |
| `RUL254 ANALYSIS_INCONCLUSIVE`         | Analysis inconclusive          | Coverage        | Warning | Analyzer         | Never                      | analysis-limit  |
| `RUL300 SCENARIO_FAILED`               | Scenario failed                | Scenario        | Error   | ScenarioRunner   | Never                      | trace           |
| `RUL301 SCENARIO_UNDECLARED_FACT`      | Scenario uses undeclared fact  | Scenario        | Error   | ScenarioCompiler | Never                      | provenance      |
| `RUL302 STALE_SCENARIO`                | Scenario is stale              | Scenario        | Warning | ScenarioCompiler | WithJustificationAndExpiry | provenance      |
| `RUL350 OUTCOME_CHANGED`               | Outcome changed                | Change          | Warning | Analyzer         | WithJustificationAndExpiry | behavior-change |
| `RUL351 MORE_RESTRICTIVE_CHANGE`       | More restrictive change        | Change          | Warning | Analyzer         | WithJustificationAndExpiry | behavior-change |
| `RUL352 MORE_PERMISSIVE_CHANGE`        | More permissive change         | Change          | Warning | Analyzer         | WithJustificationAndExpiry | behavior-change |
| `RUL353 DEFAULT_CHANGED`               | Default changed                | Change          | Warning | Analyzer         | WithJustificationAndExpiry | behavior-change |
| `RUL354 PRECEDENCE_CHANGED`            | Precedence changed             | Change          | Warning | Analyzer         | WithJustificationAndExpiry | behavior-change |
| `RUL400 LOSSY_TARGET_EXPORT`           | Lossy target export            | Export          | Error   | Renderer         | Never                      | behavior-change |
| `RUL401 UNSUPPORTED_TARGET_OUTCOME`    | Unsupported target outcome     | Export          | Error   | Renderer         | Never                      | provenance      |
| `RUL500 LOCKFILE_OUT_OF_DATE`          | Lockfile out of date           | Evidence        | Error   | Store            | Never                      | properties      |
| `RUL501 IMPORT_HASH_CHANGED`           | Import hash changed            | Evidence        | Error   | Store            | Never                      | properties      |
| `RUL502 LOCKFILE_MISSING`              | Required lockfile missing      | Evidence        | Error   | Store            | Never                      | none            |
| `RUL503 IMPORT_CYCLE`                  | Import cycle                   | Evidence        | Error   | Facade           | Never                      | properties      |
| `RUL504 IMPORT_RESOURCE_LIMIT`         | Import resource limit exceeded | Evidence        | Error   | Facade           | Never                      | properties      |
| `RUL900 INTERNAL_INVARIANT`            | Internal invariant violated    | Internal        | Error   | Facade           | Never                      | provenance      |

Registry constants MUST retain these exact names and codes. `diagnostic_definition` returns the
row for a known code and `None` for an unknown code.

## Analysis

### Finite partitions and options

Analysis constructs one partition per decision from referenced fact paths in ascending order.
For each path it creates disjoint cells, then takes the lexicographic Cartesian product and
removes vocabulary-unsatisfiable cells. Cells are ordered by path and canonical value bytes.

Supported finite domains are Boolean values; every declared enum variant; absent, null, valid,
and malformed presence states where vocabulary permits them; and scalar equivalence classes
induced by every authored equality, ordering, range, and temporal boundary. Integer, decimal,
date, date-time, duration, and text classes contain each boundary, each open interval between
adjacent boundaries, and the two exterior intervals. A deterministic representative is selected:
the boundary itself, the next representable value for a lower-open class, or the previous
representable value for an upper-open class. Text has no next-value arithmetic, so non-literal
text intervals are unsupported. Lists and records support presence cells only. An unsupported
cell makes completeness inconclusive unless `explicit_domains` supplies finite values.

**Normative analysis declarations:**

```rust
pub struct AnalysisOptions {
    max_states: u64,
    max_witnesses: u32,
    explicit_domains: BTreeMap<FactPath, FiniteDomain>,
    include_reachability: bool,
    include_interactions: bool,
    include_coverage: bool,
}

pub struct FiniteDomain {
    values: Vec<FactPartitionValue>,
}

pub enum FactPartitionValue {
    Absent,
    Null,
    Valid(Value),
    Malformed(FactValidationError),
}

pub struct DecisionPartition {
    decision: DecisionId,
    paths: Vec<FactPath>,
    cells: Vec<PartitionCell>,
    completeness: AnalysisCompleteness,
}

pub struct PartitionCell {
    assignments: BTreeMap<FactPath, FactPartitionValue>,
}
```

Field order above is wire order before JCS object sorting. Defaults are `max_states = 100000`,
`max_witnesses = 1000`, empty explicit domains, and all three include flags true. Zero budgets
are valid and produce immediate inconclusive results for work requiring a state or witness.

### Soundness, completeness, and budgets

An analysis claim is sound when every reported universal claim is true and every existential
claim has a case that the evaluator reproduces. An analysis is complete for a declared domain
when all cases in that finite domain have been examined or a sound symbolic proof covers them.
Rulery v0.1 MUST NOT claim completeness for an unbounded domain.

`AnalysisOptions` MUST include maximum examined states and maximum generated witnesses. A state
is charged when the analyzer asks whether one complete or partial fact assignment is satisfiable;
cache hits are not charged. The budget is global per command and consumed in ascending decision
ID and analysis-kind order. When the next state would exceed the budget, analysis stops that
proof, marks the affected result `Inconclusive`, records the exact examined count and limit, and
continues only work that consumes no additional states. Budget exhaustion is not proof of
reachability, unreachability, overlap, conflict, or coverage.

Every reported overlap, conflict, reachable rule, uncovered case, or behavior change MUST carry
a synthetic `WitnessCase` that evaluates to the claimed result using a fixed clock and timezone
database identity. Unreachability MAY carry a sound proof certificate instead. Witnesses MUST be
minimal under ascending `FactPath` deletion: removing any included fact must stop reproducing the
claim. Witness hashes use the CaseFacts domain.

Coverage is defined only over the analyzer's declared finite partition. The denominator is the
number of satisfiable partition cells after vocabulary constraints and before rule evaluation.
The numerator is the number of those cells resolved by at least one non-default decisive rule
without relevant Unknown, Invalid, or conflict. If the denominator is zero, coverage is 10000
basis points. If enumeration is incomplete, the percentage is absent, not estimated.

**Normative Rust signatures and data declarations:**

```rust
pub trait PolicyAnalyzer: Send + Sync {
    fn analyze(
        &self,
        package: &CompiledPackage,
        options: &AnalysisOptions,
    ) -> AnalysisReport;
}

pub trait PolicyDiffer: Send + Sync {
    fn diff(
        &self,
        before: &CompiledPackage,
        after: &CompiledPackage,
        decision: Option<&DecisionId>,
        options: &AnalysisOptions,
    ) -> AnalysisReport;
}

pub struct WitnessCase {
    title: String,
    facts: CaseFacts,
    at: UtcInstant,
    timezone_database: TimeZoneDatabaseIdentity,
    relevant_paths: BTreeSet<FactPath>,
    claim: WitnessClaim,
    hash: ContentHash,
    synthetic: bool,
}

pub enum WitnessClaim {
    ReachableRule(QualifiedRuleId),
    RuleOverlap(QualifiedRuleId, QualifiedRuleId),
    Conflict(Vec<QualifiedRuleId>),
    Uncovered(DecisionId),
    BehaviorChange(DecisionId),
}

pub struct ProofCertificate {
    method: String,
    constraints_hash: ContentHash,
}

pub enum AnalysisCompleteness {
    Complete,
    Inconclusive { examined: u64, limit: u64 },
}

pub struct CoverageReport {
    numerator: u64,
    denominator: u64,
    percent_basis_points: Option<u16>,
    completeness: AnalysisCompleteness,
    uncovered: Vec<UncoveredCase>,
    rule_coverage: Vec<RuleCoverage>,
}

pub struct AnalysisReport {
    payload: AnalysisReportV1,
}

pub struct RuleReachability {
    rule: QualifiedRuleId,
    status: ReachabilityStatus,
    witness: Option<WitnessCase>,
    proof: Option<ProofCertificate>,
}

pub enum ReachabilityStatus {
    Reachable,
    Unreachable,
    Inconclusive,
}

pub struct RuleOverlap {
    left: QualifiedRuleId,
    right: QualifiedRuleId,
    classification: OverlapClassification,
    witness: WitnessCase,
}

pub enum OverlapClassification {
    Compatible,
    Conflict,
    ShadowedApproval,
    Redundant,
    ExplicitOverride,
}

pub struct DecisionCoverage {
    decision: DecisionId,
    report: CoverageReport,
}

pub struct UncoveredCase {
    cell: PartitionCell,
    category: UncoveredCategory,
    witness: WitnessCase,
}

pub struct RuleCoverage {
    rule: QualifiedRuleId,
    reached_cells: u64,
    determining_cells: u64,
}

pub enum UncoveredCategory {
    DefaultOnly,
    UnhandledEnumVariant,
    MissingFact,
    InvalidFact,
    TemporalBoundary,
    NoMatchingRule,
    UnsupportedDomain,
}

pub struct PolicyDiff {
    decision: DecisionId,
    changes: Vec<OutcomeChange>,
    completeness: AnalysisCompleteness,
}

pub struct OutcomeChange {
    before: Outcome,
    after: Outcome,
    classification: OutcomeChangeKind,
    witness: WitnessCase,
}

pub enum OutcomeChangeKind {
    MorePermissive,
    MoreRestrictive,
    IntroducesEscalation,
    IntroducesInformationRequest,
    PrecedenceOnly,
    ReasonOnly,
    Unknown,
}
```

### Semantic diff

Semantic diff compares two compiled packages over the union of affected finite partitions and
symbolic constraints. It MUST report changes in outcomes, reason codes, required facts, actions,
defaults, precedence, missing strategy, invalid strategy, timezone, expiry policy, vocabulary,
and import integrity. Each behavioral change MUST include one witness evaluated against both
packages at the same UTC instant and timezone database identity. Classification is
`MorePermissive`, `MoreRestrictive`, `IntroducesEscalation`,
`IntroducesInformationRequest`, `PrecedenceOnly`, `ReasonOnly`, or `Unknown`.

If the budget is exhausted, the report is incomplete and MUST NOT assert that unreported
behavior is unchanged. Source-only changes with identical canonical compiled payloads are
non-semantic and MAY be reported separately.

## Scenarios

Source scenarios are parsed by syntax, then compiled against a `CompiledPackage` by
`ScenarioCompiler`. Compilation resolves the decision, typed facts, qualified rules, reason
codes, and required fact paths. A scenario with an invalid expectation does not run.

Only root-package scenarios are passed to `ScenarioCompiler`; imported scenarios never run. The
runner constructs a `FixedClock` from required scenario `at`, which replaces the caller's clock
for that run. The caller's timezone database and trace detail remain in effect.

**Normative Rust signatures and data declarations:**

```rust
pub struct ExpectedDecision {
    outcome: OutcomeKind,
    determining_rules: BTreeSet<QualifiedRuleId>,
    required_facts: BTreeSet<FactPath>,
    reason_codes: BTreeSet<ReasonCode>,
}

pub struct CompiledScenario {
    id: ScenarioId,
    title: String,
    description: Option<String>,
    decision: DecisionId,
    at: UtcInstant,
    given: CaseFacts,
    expect: ExpectedDecision,
    tags: BTreeSet<StableId>,
    span: Span,
}

pub struct ScenarioCompilationOutput {
    scenarios: Vec<CompiledScenario>,
    diagnostics: DiagnosticReport,
}

pub struct ScenarioResult {
    payload: ScenarioResultV1,
}

pub trait ScenarioCompiler: Send + Sync {
    fn compile(
        &self,
        package: &CompiledPackage,
        scenarios: &[SourceScenario],
    ) -> ScenarioCompilationOutput;
}

pub trait ScenarioRunner: Send + Sync {
    fn run(
        &self,
        package: &CompiledPackage,
        scenario: &CompiledScenario,
        context: &EvaluationContext,
    ) -> ScenarioResult;
}
```

The runner compares outcome, determining rules, required facts, and reason codes. Every field is
compared exactly. Collections are set-equal: no expected member may be missing and no actual
member may be additional. A future schema may add explicit containment matching, but v0.1 has no
containment default. All mismatches are collected in deterministic field and value order.

## Ports, facade, and macros

### Complete port list

**Normative Rust signatures:**

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

pub trait SourceParser: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;
    fn parse_bundle(&self, bundle: &SourceBundle) -> Result<ParsedPackage, Self::Error>;
}

pub trait DecisionEvaluator: Send + Sync {
    fn evaluate(
        &self,
        package: &CompiledPackage,
        decision: &DecisionId,
        facts: &CaseFacts,
        context: &EvaluationContext,
    ) -> Result<DecisionTrace, EvaluationError>;
}

pub trait ArtifactRenderer<T>: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;
    fn render(&self, artifact: &T) -> Result<Vec<u8>, Self::Error>;
}
```

Together with `PolicyCompiler`, `PolicyAnalyzer`, `PolicyDiffer`, `ScenarioCompiler`, and
`ScenarioRunner`, these are v0.1 ports. The complete list is `PackageStore`, `SourceParser`,
`PackageAssemblyService`, `PolicyCompiler`, `DecisionEvaluator`, `PolicyAnalyzer`,
`PolicyDiffer`, `ScenarioCompiler`, `ScenarioRunner`, `ArtifactRenderer<T>`, `Clock`, and
`TimeZoneDatabase`.
No other capability port is part of v0.1. `FilesystemPackageStore`, `YamlSourceParser`,
`SystemClock`, `FixedClock`, `JiffTimeZoneDatabase`, and format renderers are adapters. The CLI
owns stdout and file writes; renderers own neither command parsing nor exit policy. No v0.1
network port exists.

### Facade workflows

The facade is the application composition root for library consumers. It owns recursive package
assembly and exposes workflows rather than filesystem-aware compiler methods.

**Normative facade signatures:**

```rust
pub trait RuleryFacade: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    fn compile_package(
        &self,
        root: &PackagePath,
        lock_mode: LockMode,
    ) -> Result<CompileWorkflowOutput, Self::Error>;
    fn evaluate(
        &self,
        package: &CompiledPackage,
        decision: &DecisionId,
        facts: &CaseFacts,
        context: &EvaluationContext,
    ) -> Result<DecisionTrace, EvaluationError>;
    fn analyze(
        &self,
        package: &CompiledPackage,
        options: &AnalysisOptions,
    ) -> AnalysisReport;
    fn diff(
        &self,
        before: &CompiledPackage,
        after: &CompiledPackage,
        decision: Option<&DecisionId>,
        options: &AnalysisOptions,
    ) -> AnalysisReport;
    fn run_scenarios(
        &self,
        package: &CompiledPackage,
        scenarios: &[CompiledScenario],
        context: &EvaluationContext,
    ) -> Vec<ScenarioResult>;
}

pub struct CompileWorkflowOutput {
    package: Option<CompiledPackage>,
    scenarios: Vec<CompiledScenario>,
    diagnostics: DiagnosticReport,
    proposed_lock: Option<RulebookLock>,
}
```

The workflow order is store load, syntax parse, recursive facade assembly, source-key remap,
compile, then root-scenario compile. Evaluation, analysis, scenario running, and rendering consume
the resulting package without reopening source files.

**Illustrative embedding example:**

```text
facade.compile_package(root, Frozen)
facade.evaluate(package, checkout, facts, EvaluationContext)
facade.analyze(package, AnalysisOptions)
facade.diff(before, after, optional_decision, AnalysisOptions)
facade.run_scenarios(package, root_scenarios, EvaluationContext)
renderer.render(typed_artifact)
```

### Macro contracts and hygiene

The facade provides the default assembly/evaluation workflow and curated re-exports. It exports
declarative `stable_id!`, typed ID literal macros, `facts!`, `scenario!`, `assert_decision!`, and
`diagnostic!`. Expansions MUST use `$crate::__private` and validated public constructors; they
MUST NOT construct private fields.

**Normative Rust data declaration:**

```rust
#[doc(hidden)]
pub mod __private {
    pub use rulery_contracts::*;
    pub use rulery_diagnostics::*;
    pub use rulery_scenarios::*;
}
```

`rulery-macros` owns procedural macros and compile-time literal validation. It MUST NOT depend on
the facade. Procedural expansion locates the facade with `proc_macro_crate`. The facade feature
`macros` enables the optional `rulery-macros` dependency and MUST be disabled by default.
Conformance tests MUST rename the downstream `rulery` dependency to prove both declarative and
procedural expansion hygiene.

Macro invocation grammar is the following EBNF. Literal punctuation is quoted; `{ X }` means
zero or more repetitions and `[ X ]` means optional EBNF content, not Rust tokens.

```text
string = Rust string literal
ident = Rust identifier
expr = one Rust expression

stable-id = "stable_id!(" string ")"
typed-id = typed-id-name "!(" string ")"
typed-id-name = "package_id" | "decision_id" | "rule_id" | "action_id"
              | "scenario_id" | "escalation_id" | "reason_code" | "fact_path"

facts = "facts!({" [ root-entry { "," root-entry } [ "," ] ] "})"
root-entry = ident ":" macro-value
macro-value = "null" | "true" | "false" | integer | constructor | list | record
constructor = ("decimal" | "text" | "date" | "date_time" | "duration")
              "(" string ")"
            | "enum_value(" string "," string ")"
list = "[" [ macro-value { "," macro-value } [ "," ] ] "]"
record = "{" [ ident ":" macro-value { "," ident ":" macro-value } [ "," ] ] "}"

scenario = "scenario!{" "id:" string "," "title:" expr ","
           "decision:" string "," "at:" expr "," "given:" expr ","
           "expect:" expr "," "span:" expr
           [ "," "tags:[" [ string { "," string } [ "," ] ] "]" ] [ "," ] "}"

assert-decision = "assert_decision!(" expr "," "outcome:" outcome-kind ","
                  "determining:[" id-list "]" ","
                  "required_facts:[" path-list "]" ","
                  "reasons:[" code-list "]" [ "," ] ")"

diagnostic = "diagnostic!{" "code:" expr "," "message:" expr ","
             "primary:" expr [ "," "evidence:[" expr-list "]" ] [ "," ] "}"

outcome-kind = "Approve" | "Deny" | "Escalate" | "RequestInformation"
id-list = [ string { "," string } [ "," ] ]
path-list = [ string { "," string } [ "," ] ]
code-list = [ string { "," string } [ "," ] ]
expr-list = [ expr { "," expr } [ "," ] ]
integer = one Rust i64 literal
```

`stable_id!` and typed-ID macros return the corresponding validated value; an invalid or
non-literal argument is a compile error. `facts!` returns
`Result<CaseFacts, FactBuildError>` and performs no vocabulary inference. `scenario!` returns
`Result<SourceScenario, ScenarioBuildError>`, never executes, requires a non-empty title and
caller-supplied `Span`, and uses that span for the scenario node. `assert_decision!` returns `()`
and panics with every exact expectation mismatch. `diagnostic!` returns
`Result<Diagnostic, DiagnosticBuildError>` and obtains title, severity, producer, and
suppressibility from the registry; callers cannot override them.

The optional procedural crate exports exactly `#[derive(RuleFacts)]` in v0.1. Fields require
explicit `#[rulery(path = "...")]`; the container requires `#[rulery(root = "...")]`. Unknown,
duplicate, or malformed attributes and unsupported field types are compile errors. Generated code
implements `TryFrom<GeneratedType> for CaseFacts`, preserves field conversion paths, treats
`Option::None` as absence, emits no unsafe code, and resolves the renamed facade with
`proc_macro_crate`.

## CLI

### Command surface and lock defaults

```text
rulery init [PATH]
rulery fmt [PATH] [--check]
rulery check [PATH] [--frozen] [--deny-warnings] [--format human|json|sarif]
rulery analyze [PATH] [--frozen] [--deny-warnings] [--max-states N]
               [--max-witnesses N] [--format human|json|sarif]
rulery test [PATH] [--frozen] [--deny-warnings] [--filter TAG_OR_ID]
            [--format human|json]
rulery explain [PATH] --decision ID --facts FILE [--at RFC3339]
               [--frozen] [--deny-warnings] [--format human|json]
rulery diff <BEFORE> <AFTER> [--decision ID] [--max-states N]
             [--max-witnesses N] [--frozen] [--deny-warnings]
             [--format human|json]
rulery render [PATH] --format markdown|json|decision-table [--decision ID]
              [--frozen] [--deny-warnings]
rulery lock [PATH]
```

`PATH` defaults to `.`. Output format defaults to `human`. `--at` defaults to the system UTC
clock; a supplied RFC 3339 value is converted to UTC. `--max-states` defaults to `100000`.
`--max-witnesses` defaults to `1000`. `--deny-warnings` promotes registry Warning diagnostics
with `Proven` or `Witnessed` confidence to command-failing errors after suppression; it never
promotes Advice, Heuristic, or Inconclusive findings.
`check`, `analyze`, `test`, `explain`, `diff`, and `render` use `LockMode::Update` unless
`--frozen` is supplied. They never write a lock. `lock` always uses Update and atomically writes
the proposed complete transitive lock only after parse, resolution, compilation, and integrity
validation succeed. `init` and `fmt` do not assemble imports.

There is no implicit CI detection. Environment variables MUST NOT change lock mode. CI MUST pass
`--frozen` explicitly.

`render --decision` selects one decision. It is REQUIRED for `decision-table`, OPTIONAL for
Markdown and JSON, and rejected when the ID is unknown. Without it, Markdown and JSON render all
decisions in ascending `DecisionId` order.

### Output contract

Normal artifacts go to stdout. Diagnostics, progress, and I/O/internal error messages go to
stderr in human mode. JSON emits exactly one of the seven versioned Rulery envelopes, except that
`test` emits one documented JSON array of `ScenarioResultEnvelope` objects. SARIF emits exactly
one SARIF 2.1.0 log and is not a Rulery envelope. In machine modes, stderr contains only fatal
pre-artifact I/O or invocation messages. Successful commands produce no progress text.

| Command       | Stdout artifact and cardinality                                                              |
| ------------- | -------------------------------------------------------------------------------------------- |
| `init`        | one human path line; empty with machine output unavailable                                   |
| `fmt`         | formatted source bytes only when PATH names one file; otherwise one human summary            |
| `fmt --check` | empty                                                                                        |
| `check`       | one human report, `DiagnosticReportEnvelope`, or one SARIF log                               |
| `analyze`     | one human report, `AnalysisReportEnvelope`, or one SARIF log                                 |
| `test`        | one human report or one JSON array of `ScenarioResultEnvelope`, in scenario-ID order         |
| `explain`     | one human explanation or one `DecisionTraceEnvelope`                                         |
| `diff`        | one human report or one `AnalysisReportEnvelope` containing semantic diff results            |
| `render`      | one Markdown document, one `CompiledPackageEnvelope`, or exactly one `DecisionTableEnvelope` |
| `lock`        | one human path line after atomic write                                                       |

No command writes an artifact file except `init`, `fmt` without `--check`, and `lock`. Broken
stdout is `IoFailure` unless a higher-priority exit was already determined and fully emitted.

### Exit-condition matrix

The CLI returns the first matching row in table order.

| Priority | Exit | Name               | Exact condition                                                                              |
| -------- | ---- | ------------------ | -------------------------------------------------------------------------------------------- |
| 1        | 2    | InvalidInvocation  | Command syntax, argument value, or requested format is invalid                               |
| 2        | 3    | IoFailure          | Required read, write, atomic rename, or local path resolution fails                          |
| 3        | 4    | InternalFailure    | Internal invariant, serialization invariant, panic boundary, or unexpected adapter failure   |
| 4        | 1    | DiagnosticsError   | `init` targets an existing non-empty destination                                             |
| 5        | 1    | DiagnosticsError   | Any command emits an unsuppressed Error or a promotable Warning under `--deny-warnings`      |
| 6        | 5    | ScenarioFailure    | `test` compiled successfully and at least one scenario failed exact comparison               |
| 7        | 6    | AnalysisIncomplete | `analyze` or `diff` has no earlier failure but any requested analysis is inconclusive        |
| 8        | 1    | DiagnosticsError   | `fmt --check` detects changes or `check`/`analyze` has policy-configured failing diagnostics |
| 9        | 0    | Success            | Command completed and none of the earlier conditions applies                                 |

For `lock`, a lock mismatch in Update is not itself an error; successful replacement exits zero.
In Frozen mode any missing, stale, or additional lock entry is DiagnosticsError. Human and JSON
formats MUST map the same semantic result to the same exit code.

Command-specific rules are: `fmt --check` returns 1 when bytes would change; `check` returns 1 on
failing diagnostics; `test` returns 5 only after successful compilation and scenario compilation;
`explain` returns 1 for invalid facts or runtime conflict; `analyze` and `diff` return 6 only for
inconclusive analysis without a prior error; `render` returns 1 for an unsupported or lossy
projection; and `lock` returns 3 if atomic write or rename fails. Invalid IDs and mutually
incompatible flags return 2.

## Tool-library conformance example

The authored files in [Authored language](#authored-language) are the canonical tool-library
fixture. At `2026-09-16T16:00:00.000000000Z`, New York local date is `2026-09-16`. The supplied
training expiry is `2026-09-15`, so `deny-expired-training` is true and determines a deny.

**Normative `examples/tool-library/cases/expired-training.yaml`:**

```yaml
member:
  account-status: active
  training:
    completed-at: "2025-01-10"
    valid-until: "2026-09-15"
  unresolved-damage-reports: 0
tool:
  category: power-tool
```

**Illustrative end-to-end CLI use:**

```console
$ rulery lock examples/tool-library
$ rulery check examples/tool-library --frozen
$ rulery test examples/tool-library --frozen
$ rulery explain examples/tool-library --decision checkout \
    --facts examples/tool-library/cases/expired-training.yaml \
    --at 2026-09-16T16:00:00Z --frozen
```

**Illustrative explain JSON envelope shape:**

The hash strings below are the independently computed framing vectors above. This listing is not
a canonical tool-library trace fixture and does not claim that its abbreviated rule trace hashes
to those values; it demonstrates envelope shape and canonical scalar spelling only.

```json
{
  "schema": "rulery.decision-trace/v1",
  "payload": {
    "evaluation_id": "blake3:4c11eb70b89e8543605015c325e309fde68d9007c2086b97bad892693cae9279",
    "package": "community-tool-library",
    "package_version": "0.1.0",
    "decision": "checkout",
    "evaluated_at": "1789574400000000000",
    "timezone": "America/New_York",
    "timezone_database": {
      "implementation": "test",
      "version": "1"
    },
    "package_hash": "blake3:1c6402278430173b5964f65d31c1ec06a9765c13c6e34bad762fcdee8ffbd6c8",
    "facts_hash": "blake3:fa785b19b5601f57afd5548934d2c1e4b20bc1a8ab25b89e6bece83d041d31aa",
    "compiler": {
      "name": "rulery-compiler",
      "version": "0.1.0"
    },
    "language_version": "1",
    "outcome": {
      "kind": "deny",
      "reasons": [
        {
          "code": "expired-training",
          "message": "Power-tool training has expired."
        }
      ],
      "actions": []
    },
    "determining_rules": [
      {
        "package": "community-tool-library",
        "rule": "deny-expired-training"
      }
    ],
    "superseded_rules": [],
    "rule_traces": [
      {
        "rule": {
          "package": "community-tool-library",
          "rule": "deny-expired-training"
        },
        "result": "true",
        "selected": true,
        "relevance": "decisive",
        "condition": {
          "kind": "predicate",
          "result": "true",
          "operator": "is_expired",
          "lhs": {
            "state": "value",
            "value": {
              "kind": "date",
              "value": "2026-09-15"
            },
            "source_path": "member.training.valid-until"
          },
          "span": {
            "source": "0",
            "start": "0",
            "end": "0"
          }
        },
        "candidate": {
          "outcome": {
            "kind": "deny",
            "reasons": [
              {
                "code": "expired-training",
                "message": "Power-tool training has expired."
              }
            ],
            "actions": []
          },
          "semantic_key": ["4000", "800", "9", "0"]
        },
        "source_span": {
          "source": "0",
          "start": "0",
          "end": "0"
        }
      }
    ],
    "missing_facts": [],
    "invalid_facts": [],
    "strategy_applications": [],
    "conflict": null,
    "trace_hash": "blake3:4773d0e94079df21090915deecb6b2f41327122e6b479afb63d5d082f174aced"
  }
}
```

**Normative canonical human explain output for the fixture:**

```text
Decision: DENY
Rulebook: community-tool-library@0.1.0
Decision ID: checkout
Evaluated at: 2026-09-16T16:00:00.000000000Z
Policy date: 2026-09-16 (America/New_York)

Reason:
  [expired-training] Power-tool training has expired.

Determining rule:
  community-tool-library::deny-expired-training

Conditions:
  true  tool.category equal power-tool
  true  member.training.valid-until is expired
  true  inclusive expiry: 2026-09-15 is earlier than 2026-09-16

Required facts: none
Invalid facts: none
Superseded rules: none

Evidence:
  canonical package, facts, and trace hashes are present in JSON output
  timezone database identity is present in JSON output
```

## Conformance and governance

### Required conformance gates

A conforming implementation MUST pass:

- workspace formatting and Clippy with warnings denied;
- unit, property, nextest, and doctest suites;
- architecture checks for the exact dependency allowlist and absence of cycles;
- compile checks for every normative Rust declaration block;
- parse and schema checks for every normative YAML and JSON example;
- round trips for all seven envelopes and every validated newtype;
- all complete truth and predicate tables;
- precedence, unresolved-conflict, uncertainty-relevance, DST, and expiry fixtures;
- independently recomputed fixed hash vectors for all domains;
- scenario exact-comparison fixtures, including every mismatch field;
- analyzer soundness, budget, witness replay, coverage, and semantic-diff fixtures;
- renderer snapshots, including the canonical tool-library explanation; and
- renamed-dependency macro hygiene tests with default features and `macros` enabled.

Trace invariants require that determining rules are selected true rule traces, superseded rules
are unselected rule traces, all reported missing and invalid facts occur in operand traces,
package and facts hashes match canonical inputs, and identical package, facts, UTC instant, and
timezone database identity produce byte-identical canonical trace payloads.

### Safety and governance constraints

- Unknown MUST NOT become False except under declared `ClosedWorldFalse`.
- Null and malformed evidence MUST NOT be treated as absence.
- A default caused after unresolved evidence MUST be distinguishable from evidence-based denial.
- Escalation MUST have a destination and reasons; information requests MUST have required facts.
- Every outcome MUST contain at least one stable reason code and human message.
- Semantic precedence MUST NOT depend on rule identity or source order.
- Semantic diff MUST detect precedence, default, strategy, time, vocabulary, and import changes.
- Synthetic witnesses MUST be marked synthetic and MUST NOT be logged as real case data.
- Trace persistence is opt-in; default CLI behavior keeps case data in process memory only.
- Renderers MUST preserve Unknown and Invalid and MUST NOT call either a failed criterion.
- Exporters MUST fail on semantic loss and MUST NOT lower escalation or information requests to
  denial.
- Fixes MUST satisfy non-overlap validation and MUST state their applicability honestly.
- Local imports MUST remain within the configured package root policy and MUST reject traversal
  outside allowed roots after canonical path resolution.
- Implementations MUST bound source size, import depth, import count, expression depth, and
  analysis states and report deterministic diagnostics when a bound is exceeded.

## Blueprint phases

These phases constrain implementation dependency order; they are not permission to weaken an
earlier contract.

1. **Contracts, diagnostics, vocabulary:** validated types, custom deserialization, source-map
   types, wire schemas, registry, value validation, and fixed scalar/hash vectors.
2. **Syntax, IR, compiler, store:** YAML parsing and source-map construction, raw local loading,
   lock data, resolution, type checking, normalization, lowering, and compiled hashing.
3. **Engine and scenarios:** time ports, four-valued evaluation, relevance, precedence,
   conflicts, complete truth/evaluation conformance, traces, scenario compilation, and exact
   comparison.
4. **Analysis and emit:** sound analyses, budgets, witnesses, coverage, semantic diff, and all
   renderers.
5. **Facade, macros, CLI, conformance, compatibility review:** recursive facade composition,
   end-to-end Update/Frozen lock workflow, hygienic macros, command and exit contracts, fixtures,
   architecture gates, and v0.1 API and wire review.

Each phase MUST pass formatting, compile, serialization, semantic, and conformance gates for its
owned contracts before a dependent phase begins.

## Deferred features

The following are explicitly outside v0.1 and MUST NOT be added to core contracts without a new
approved specification version:

- `rulery-lsp`, visual editors, and native `.rulery` syntax;
- WASM execution;
- Rego, Cedar, SQL, and database adapters;
- remote registries, network imports, and signed releases;
- AI authoring and natural-language extraction;
- user/group/role entity graphs;
- runtime action execution and multi-decision workflow orchestration; and
- complete theorem proving over unbounded numeric domains.

## Canonical success criterion

Rulery v0.1 succeeds when a team can commit the normative YAML package shape, resolve and freeze
its complete local import closure, run it explicitly in CI, receive actionable and reproducible
diagnostics for missing or conflicting logic, execute named scenarios, evaluate a case at a fixed
UTC instant under recorded IANA timezone data, and produce a deterministic explanation trace
that another implementation independently reproduces from the same compiled package and facts.
