# Design: Analysis Input Extraction

## Goal

Derive every static-analysis input from a compiled package so that reachability, interaction, and
coverage analysis is reachable in production, and report the specification ambiguity that blocks a
complete temporal finite domain rather than inventing one.

## Approved Approach

Add an extraction layer in `rulery-analysis` that walks compiled conditions, resolves vocabulary
types, builds finite partitions, evaluates each partition cell through the existing engine, and
feeds the existing `analyze_interactions` and `compute_coverage` functions. The analyzer functions
keep their current behaviour; the layer only supplies their inputs. `ProductionApplication::analyze`
and `ProductionApplication::diff` delegate to the new port implementation instead of failing.

## Why This Note Exists

The partition is the _denominator_ of every coverage claim and the _sample set_ of every
reachability, overlap, conflict, and semantic-diff claim. A partition that silently drops a
referenced fact path, invents a value class that the evaluator cannot reproduce, or claims
completeness for a domain it did not enumerate produces three wrong answers at once: a coverage
percentage that overstates coverage, a rule reported unreachable that is in fact reachable, and a
semantic diff that reports no change because the sampled prefix missed the change. None of these
failures raise an error; they are wrong numbers in a report. The derivation rules therefore had to
be pinned before any code was written.

## Delivery Boundary

1. Add the shared `Expr` inspection helpers to `rulery-ir`, which owns the expression model.
2. Add the partition, rule-input, and cell-evaluation derivation to `rulery-analysis`.
3. Add decision-table projection derivation to `rulery-emit`.
4. Delegate `ApplicationService::analyze` and `::diff` to the new analyzer.

## Crate Ownership

- **Expression identity owner**: `rulery-ir` owns the span-free canonical rendering of a condition
  and the set of fact paths a condition references. Both are properties of `Expr` itself, so both
  live beside the type and are shared by analysis and emission.
- **Analysis input owner**: `rulery-analysis` owns partition specification derivation, cell
  evaluation, rule input derivation, budget accounting, and report assembly. It already depends on
  `rulery-engine`, `rulery-ir`, `rulery-vocabulary`, and `rulery-diagnostics`, which is exactly what
  derivation needs.
- **Diagnostic evidence owner**: `rulery-diagnostics` owns construction of the evidence values its
  registry requires. The evidence structs have private fields, so the analyzer cannot build a
  registry-valid diagnostic until those structs gain constructors.
- **Emission owner**: `rulery-emit` owns decision-table projection, because column and cell
  semantics are decision-table semantics.

No new workspace crate is introduced and no new external dependency is added.

## Public API

### Expression inspection

```rust
/// Renders one condition in a span-free canonical form.
#[must_use]
pub fn canonical_condition(expression: &Expr) -> String;

/// Returns every fact path a condition references, in ascending path order.
#[must_use]
pub fn referenced_fact_paths(expression: &Expr) -> BTreeSet<FactPath>;
```

### Partition domain kinds

```rust
pub enum PartitionDomainKind {
    Boolean,
    Enum(Vec<Value>),
    Integer(Vec<i64>),
    Text(Vec<String>),
    List,
    Record,
    /// Raw domain is unbounded and no authored boundary constrains it.
    Unbounded,
    /// Domain shape that v0.1 cannot partition into a finite cell set.
    Unsupported,
}
```

`Unbounded` and `Unsupported` are additive variants. `Unbounded` names a raw value domain that has
no authored boundary, which the partition collapses to exactly one equivalence class.
`Unsupported` names a raw value domain that has no representation in the normative data contract and
therefore yields presence cells only.

### Analyzer

```rust
pub fn analysis_instant() -> UtcInstant;

pub struct PackageAnalyzer<'a> {
    time_zones: &'a dyn TimeZoneDatabase,
    at: UtcInstant,
}

impl<'a> PackageAnalyzer<'a> {
    #[must_use]
    pub fn new(time_zones: &'a dyn TimeZoneDatabase, at: UtcInstant) -> Self;
}

impl PolicyAnalyzer for PackageAnalyzer<'_> {
    fn analyze(&self, package: &CompiledPackage, options: &AnalysisOptions) -> AnalysisReport;
}

impl PackageAnalyzer<'_> {
    #[must_use]
    pub fn diff(
        &self,
        before: &CompiledPackage,
        after: &CompiledPackage,
        decision: Option<&DecisionId>,
        options: &AnalysisOptions,
    ) -> AnalysisReport;
}
```

### Derived decision input

```rust
pub struct AnalysisContext {
    pub at: UtcInstant,
    pub time_zones: TimeZoneDatabaseIdentity,
}

impl AnalysisContext {
    #[must_use]
    pub fn new(time_zones: &dyn TimeZoneDatabase, at: UtcInstant) -> Self;
}

pub struct CellOutcome {
    pub facts: BTreeMap<FactPath, Value>,
    pub paths: BTreeSet<FactPath>,
    pub reached: BTreeSet<QualifiedRuleId>,
    pub determining: BTreeSet<QualifiedRuleId>,
    pub missing: bool,
    pub invalid: bool,
    pub conflict: bool,
    pub defaulted: bool,
    pub time_zones: TimeZoneDatabaseIdentity,
}

pub struct DecisionAnalysisInput {
    pub decision: DecisionId,
    pub partition: DecisionPartition,
    pub evaluations: Vec<CellEvaluation>,
    pub rules: Vec<RuleAnalysisInput>,
    pub witnesses: Vec<WitnessCase>,
}

impl DecisionAnalysisInput {
    #[must_use]
    pub fn derive(
        package: &CompiledPackage,
        decision: &CompiledDecision,
        evaluator: &dyn PolicyEvaluator,
        context: AnalysisContext,
        options: &AnalysisOptions,
        budget: &mut AnalysisBudget<String, Option<CellOutcome>>,
        witnesses: &mut u32,
    ) -> Self;
}

pub fn derive_partition_specs(
    package: &CompiledPackage,
    decision: &CompiledDecision,
) -> Vec<PartitionSpec>;

pub fn build_case_facts(
    assignments: &BTreeMap<FactPath, Value>,
) -> Result<CaseFacts, CaseFactsError>;
```

`build_case_facts` is the single canonical mapping from authored flat fact paths to the root-keyed
value map that `CaseFacts` stores. Scenario bridging and partition evaluation both use it, so a
witness and a scenario produce byte-identical fact maps for the same authored paths.

`CellOutcome` is public because it is the cached value type of the state budget and because
`derive` takes it as a budget parameter. It is an internal derivation product, not a report type:
nothing outside the crate consumes it.

### Decision-table projection

```rust
impl DecisionProjection {
    #[must_use]
    pub fn from_decision(
        package: &CompiledPackage,
        decision: &DecisionId,
    ) -> Option<Self>;
}
```

## Derivation Rules

### Referenced Fact Paths

A partition is built from the fact paths a decision's rules reference, exactly as the specification
requires at line 2459. Collection is a single depth-first walk of `Expr`:

- `Predicate` contributes the `left` operand's path and the `right` operand's path when present.
  `right` is included even though the operator may be unary, because the compiler only emits a
  `right` operand when the authored operator has one.
- `All`, `Any`, and `Not` contribute the union of their children. Negation and disjunction do not
  change the set.
- `Constant` contributes nothing.

Two properties make this the only acceptable rule. First, the set is an over-approximation: a path
is included even when the predicate that references it can never be satisfied. Omitting a path would
be unsound, because the partition would then enumerate assignments that are not assignments of the
rule's own inputs, and coverage would claim completeness over a domain the rule never constrained.
Second, it is polarity-blind. `Not` and `Any` do not remove or add paths, because a fact that a
negated or disjunctive predicate reads is still a fact the rule's truth depends on.

`Exists` and `Missing` reference their `left` operand like any other operator, so a presence-only
rule still contributes its path. This is what makes a missing-fact finding reachable: without the
path there would be no absent cell to evaluate.

### Boundaries

Boundary literals are collected from every predicate in every rule of the decision, on the same
walk, and are keyed by the compared path. A predicate contributes literals when its `right` operand
is a literal of the same value kind as the path's domain, and contributes every element when the
literal is a list, which covers `IsOneOf` and `Between`. `Contains`, `StartsWith`, `EndsWith`, and
`Matches` contribute their literal. `IsTrue`, `IsFalse`, `Exists`, and `Missing` contribute none.

Polarity does not change the boundary _set_. A negated comparison reads the same literal, and the
truth of a comparison is constant across each derived class, so the negation changes which cells a
rule matches but never which classes must be enumerated. Polarity is resolved by the evaluator,
which sees the real expression.

### Finite Domains

A path's domain kind is derived from the vocabulary, resolving the longest declared root, then
record fields, then an alias chain bounded to sixteen hops. A type id that is absent from the
resolved type map is a built-in, and the built-in is identified by its name, matching the convention
already used for authored literals.

| Resolved declaration                                            | Domain kind   | Cells                                                          | Complete |
| --------------------------------------------------------------- | ------------- | -------------------------------------------------------------- | -------- |
| `Enum { id, variants }`                                         | `Enum`        | one `Valid(EnumValue)` per declared variant                    | yes      |
| built-in `bool` or `boolean`                                    | `Boolean`     | `false`, `true`                                                | yes      |
| built-in `int` or `integer` with boundaries                     | `Integer`     | boundaries, adjacent representatives, exterior representatives | yes      |
| built-in `int` or `integer` without boundaries                  | `Unbounded`   | one whole-domain representative                                | yes      |
| built-in `string` or `text`                                     | `Text`        | authored literals plus presence states                         | no       |
| `List`                                                          | `List`        | presence states                                                | no       |
| `Record`                                                        | `Record`      | presence states                                                | no       |
| built-in `decimal`, `date`, `datetime`, `date-time`, `duration` | `Unsupported` | presence states                                                | no       |
| undeclared or unresolvable path                                 | `Unsupported` | presence states                                                | no       |

The `Unbounded` case is sound rather than convenient. With no authored literal on the path, the
compiler can only have type-checked presence predicates against it, and presence is invariant across
the values of the domain, so the single class is genuinely exhaustive. If a literal comparison had
been authored, its literal would have been collected and the case would not be `Unbounded`.

`Text`, `List`, `Record`, and `Unsupported` are incomplete, so `build_partition` reports
incompleteness and `compute_coverage` suppresses the percentage. Incompleteness is the specification's
own answer at line 2471, not a fallback.

### Presence States

Presence states are derived from what the vocabulary and the value contract can actually construct,
because the partition must only contain cells the evaluator can reproduce.

- `allow_absent` is always true. Omission is always constructible, and a missing required field is a
  reported outcome rather than a reason to prune the cell, so pruning it would hide the
  missing-fact finding that `RUL252` names.
- `allow_null` is always true. `Value::Null` is constructible at any path and the vocabulary has no
  nullability declaration that would forbid it.
- `allow_malformed` is true only for a list path with a declared `min_items` or `max_items` bound,
  because that is the only shape for which evidence exists that the vocabulary validator classifies
  as the out-of-range error the partition value names. A type-mismatched value is also malformed
  evidence, but it yields a different validation error than the one the partition value records, so
  using it would make a serialized cell claim an error the witness does not reproduce.
- `forbidden` is empty in v0.1. Every value the derivation produces is permitted by the vocabulary
  that produced it: enum variants come from the declaration, and boundaries come from
  compiler-checked literals.

A cell whose assignments cannot be expressed as a `CaseFacts` value map is marked unsatisfiable and
makes the partition inconclusive. This happens when a decision references both a path and a strict
descendant of it and the cell assigns a value to both, which no root-keyed value map can express.

### Vocabulary Validator Interaction

`validate_value` in `crates/vocabulary/src/validate.rs` has arms for `(Record, Record)`,
`(Enum, Value::Enum)`, `(List, Value::List)`, and `Alias`, followed by a catch-all at lines 200 to
213 that maps every other `(declaration, value)` pair to
`Malformed(FactValidationError::TypeMismatch)`. There is no arm that accepts a scalar value against
`TypeDeclaration::Primitive` or `TypeDeclaration::Enum`, and the `List` arm checks only item-count
bounds without recursing into elements.

The consequence is that, today, a non-null scalar supplied at a path whose declared type is a
primitive or an enum is classified malformed, so a comparison against that path evaluates to
`Invalid` rather than to `True` or `False`. Only list-shaped evidence, and records nested inside it,
can satisfy a predicate. The engine's own fixtures reflect this: the paths its tests compare against
are declared as list types and read with `Contains`.

This is a property of the existing vocabulary validator and the evaluator contract, not of the
extraction layer, so this design does not change it. It is recorded here because it determines what
the derived analysis can conclude. Two rules follow:

1. **Default partitions produce presence-only findings.** A `List` or `Record` path with no authored
   boundary contributes presence cells, per specification lines 2470 and 2471, and those cells cannot
   make a rule `True`. Reachability therefore reports the rule as `Inconclusive` rather than
   `Unreachable`, overlap analysis reports no pair, and coverage reports an incomplete percentage
   with `RUL254`. That is the correct answer for an incomplete partition: no unreachability claim may
   come from a partition that was not enumerated.
2. **`explicit_domains` is the reachable path to the deeper findings.** The specification's own
   escape hatch at line 2471 supplies finite values for a path the vocabulary cannot enumerate. When
   a caller supplies a `Valid(Value::List(..))` value for a list path, that cell is valid evidence,
   the predicate can be satisfied, and reachability, overlap, conflict, and coverage findings all
   become derivable. The application-level test does exactly this.

The practical consequence for a caller is that `explicit_domains` is not an optimization. Without it,
a rulebook that reads scalar fields reports only presence states.

### Cell Evaluation

One cell becomes one `CellEvaluation`:

1. The cell's `Valid` assignments become witness facts. `Absent` assignments are omitted, `Null`
   assignments become `Value::Null`, and a malformed assignment becomes the constructible
   out-of-range evidence.
2. The facts are evaluated once through the engine at the fixed analysis instant. Reached rules are
   the rules whose top-level truth is `True`; determining rules are the trace's determining rules.
3. `relevant_uncertainty` is true when the trace retains relevant missing or invalid facts, which is
   exactly the engine's own decision-relevance computation rather than a re-derivation.
4. `conflict` is the trace's runtime conflict presence.
5. The uncovered category is the first that applies: `InvalidFact` when relevant invalid evidence
   remains, `MissingFact` when relevant missing evidence remains, `UnhandledEnumVariant` when a cell
   assigns a declared enum variant that no rule of the decision references, `DefaultOnly` when the
   default resolved the cell, and `NoMatchingRule` otherwise. `UnsupportedDomain` and
   `TemporalBoundary` are not derived: an incomplete partition already reports `RUL254` for the whole
   decision, and a temporal boundary cell cannot exist while temporal domains are `Unsupported`.
6. A rejected evaluation, which happens when relevant malformed evidence meets a reject strategy, is
   a satisfiable cell with relevant uncertainty and the `InvalidFact` category. Any other evaluation
   failure makes the partition inconclusive rather than being reported as a coverage result.

### Rule Inputs

One compiled rule becomes one `RuleAnalysisInput`:

- `candidate` is the rule's outcome, priority, specificity, override rank, and qualified identity,
  which is the same candidate the engine instantiates.
- `condition_hash` is the digest of `canonical_condition`. Spans are excluded so two rules with the
  same condition in different source positions compare equal.
- `satisfiable` is `Some(true)` when at least one evaluated cell produced `Truth::True`,
  `Some(false)` only when the partition is complete and no cell produced it, and `None` otherwise.
  Unreachability is a universal claim, so it may only come from complete enumeration.
- `overlaps` is derived from the evaluated cells, not from path intersection. Two rules overlap when
  one cell made both `True`, which makes every reported pair witness-backed. Each pair is recorded in
  one direction, the direction with the lower rule identity, because the interaction analyzer
  requires one canonical direction per pair.
- `explicit_override` is `override_rank == 1`, which is exactly the bit the compiler sets from
  authored `override: true`.
- `undeclared_override` is always false. The compiled rule retains an override rank but no override
  target, so an undeclared override relationship is not derivable from a compiled package, and
  manufacturing one would be a guess.
- `witness` is the minimized witness of the first cell that made the rule `True`. When the
  satisfiability is not witnessed the analyzer discards the field, so it carries the first cell's
  witness and asserts nothing.

### Decision Projection

A decision becomes one `DecisionProjection`:

- Columns are the decision's referenced fact paths in ascending order, one condition column per path.
- A rule's cell for a path is derived from the rule's top-level conjuncts that reference the path. A
  single `Exists` becomes `Present`, a single `Missing` becomes `Absent`, a single equality becomes
  `Equal`, a single inequality becomes `NotEqual`, a single ordering becomes `Range` with the
  authored bound and its inclusivity, no conjunct becomes `Any`, and anything else becomes
  `Derived` carrying the canonical condition text. `Derived` is the lossless escape hatch the cell
  model already provides, and it is what keeps a multi-predicate path lossless.
- `truth_states` is the union of the truth states each subtree can produce. A constant contributes its
  own state. A presence predicate contributes `True` and `False`, because the presence operators
  never produce `Unknown` or `Invalid`. Any other predicate contributes all four states, because an
  absent fact yields `Unknown` and malformed evidence yields `Invalid` for any binary operator.
- `has_actions` and `has_reasons` read the rule's outcome template.
- Precedence and conflict enter the projection only through the row outcome, which is the rule's
  outcome kind. Conflict is a runtime outcome of selection, not a column, so `build_decision_table`
  stays a pure lowering step and `validate_projection` remains the semantic-loss check.

## Determinism And Budget

- **Instant.** `PolicyAnalyzer::analyze` has no clock parameter, so the analyzer uses a fixed
  instant. `analysis_instant()` is the Unix epoch, chosen because it is representable by the
  supported civil calendar and because it is a constant rather than a clock read. No caller-supplied
  clock is consulted.
- **Time-zone database.** The analyzer takes a `TimeZoneDatabase` port and records the identity the
  engine reports into every witness. The identity string is supplied by the caller, so a caller that
  needs byte-reproducible reports supplies a fixed identity. Tests use UTC, where the local date is a
  pure function of the instant and no system time-zone database content is consulted.
- **Ordering.** Paths, cells, rules, and pairs are processed in ascending canonical order. Every
  collection is a `BTreeMap` or `BTreeSet`, so report bytes do not depend on hash order.
- **States.** Every cell evaluation is charged through `AnalysisBudget`, keyed by decision identity
  and canonical fact bytes, so a repeated assignment is a cache hit and is not charged. When the
  budget would be exceeded, the remaining cells are not evaluated, the partition and the aggregate
  completeness become `Inconclusive` with the exact examined count and limit, and coverage
  suppresses its percentage. No unreachability claim is made from an exhausted budget.
- **Witnesses.** Coverage witnesses are the cell's own facts, which the cell's state charge already
  covers. Rule witnesses are minimized under ascending path deletion, and each candidate deletion is
  charged as a partial assignment because the specification charges partial assignments. The witness
  count is bounded by `max_witnesses`; once the bound is reached the unminimized cell assignment is
  used, which remains replayable, and a rule with no reaching cell reports no reachability claim.
- **Semantic diff.** The diff enumerates the union of the before and after partitions, evaluates each
  union cell against both packages at the same instant and the same time-zone identity, and passes the
  cells to the existing `PolicyDiffer` in canonical order, which applies `max_states` itself. A path
  whose domain kind changed between packages is `Unsupported` in the union partition, which makes the
  diff inconclusive instead of silently comparing different domains. A decision whose semantics,
  default, vocabulary, or import integrity changed contributes the matching `StructuralChange`
  values, and equal-kind outcomes contribute `Reasons`, `RequiredFacts`, or `Actions` when only those
  parts differ.
- **Added or removed decisions.** A decision present in only one of the two packages has no
  `PolicyDiffer` input, because there is no pair of outcomes to compare. Rather than report it as
  unchanged, which would be false, the diff marks the whole report `Inconclusive` and emits a
  decision-level `RUL254` for that identity. The added or removed decision is a real behavioural
  change that the v0.1 differ cannot classify, so it is reported as an analysis limit rather than
  guessed at.
- **Rejected evaluations.** A cell whose evaluation is rejected, which happens when relevant
  malformed evidence meets `InvalidFactStrategy::RejectEvaluation`, has no outcome to project. The
  partition becomes incomplete, coverage suppresses its percentage, and the decision emits a
  decision-level `RUL254` with the examined count and limit. This is deliberately uniform: any
  evaluation failure stops enumeration and makes the decision inconclusive, with no per-cell
  special case that could be mistaken for a coverage result.

## Specification Ambiguity

The finite-domain prose lists decimal, date, date-time, and duration as supported equivalence-class
domains at specification lines 2465 to 2467, but the normative data declaration for the partition,
at lines 2473 to 2506, has no domain kind that can hold a temporal or decimal class, and the
implemented `PartitionDomainKind` therefore has no variant for them. A temporal domain would need a
representative-selection rule for `PolicyDate`, `UtcInstant`, and `DurationValue` that the
specification does not state, and a wrong representative would make a temporal rule look reachable or
unreachable when it is neither.

This design therefore implements the conservative reading: temporal and decimal paths are
`Unsupported`, they contribute presence cells only, and the decision reports inconclusive
completeness. `explicit_domains` remains the specification's own escape hatch at line 2471 for a
caller that needs a complete temporal analysis. Consequence: `RUL253`, the uncovered temporal
boundary, has no production trigger under this reading, exactly as `RUL150` and `RUL151` already
have none. No gate row depends on `RUL253`.

A second, smaller ambiguity: the interaction analyzer classifies a pair with a declared explicit
override as `ExplicitOverride` and reports `RUL204 UNDECLARED_OVERRIDE`, whose name reads as the
opposite of a declared override. The input side is derived honestly from the compiled override rank,
and the classification is left unchanged, because changing it would change existing analyzer
behaviour. The design note records the mismatch instead of hiding it by discarding the override bit.

A third condition is not an ambiguity in the specification but a gap in the implementation of the
vocabulary validator, described under _Vocabulary Validator Interaction_: scalars against primitive
and enum declarations are classified malformed. Fixing it would change `V05` and `V06` behaviour and
belongs to the evaluation contract, not to this design.

## Data Flow

1. `derive_partition_specs` walks each rule condition, collects referenced paths and boundary
   literals, and resolves each path's vocabulary type into a `PartitionSpec`.
2. `build_partition` sorts paths, derives each domain, applies presence states and forbidden values,
   and forms the canonical Cartesian product.
3. `DecisionAnalysisInput::derive` evaluates every cell through the engine under the state budget and
   projects each trace into a `CellEvaluation` with a witness.
4. Rule inputs are derived from the same evaluations, so reachability and overlap claims are
   consequences of the evaluated cells rather than separate approximations.
5. `compute_coverage` and `analyze_interactions` consume those inputs unchanged.
6. The analyzer converts findings into registry-validated diagnostics, assembles the
   `AnalysisReportV1`, and `AnalysisReport::new` canonicalizes section order.

## Hexagonal Boundaries

- **Ports used**: `PolicyEvaluator` for every cell evaluation, `TimeZoneDatabase` for the policy-local
  date and witness identity, and `PolicyAnalyzer` as the report port the application depends on.
- **Core adapters added**: expression inspection in `rulery-ir`, analysis input derivation and report
  assembly in `rulery-analysis`, and decision-table projection in `rulery-emit`.
- **Application adapter**: `ProductionApplication` supplies its existing time-zone database and
  delegates; it derives no analysis semantics.
- **No new port.** The extraction layer adds no capability port, so the v0.1 port list is unchanged.

## Compatibility

- `PartitionDomainKind` gains two variants. The change is additive for the pre-release `0.1.0` API;
  no existing variant changes behaviour, and `inferred_domain` keeps its previous arms.
- `WitnessEvidence`, `ProofEvidence`, `AnalysisLimitEvidence`, and `BehaviorChangeEvidence` gain
  infallible constructors. Their fields stay private and their wire representation is unchanged.
- `ApplicationError::AnalysisUnavailable` is removed. It existed only to report that analysis had no
  production caller, which is exactly the state this design ends, so retaining a variant that can
  never be constructed would document a capability the crate does not have. This is a breaking change
  to the pre-release `0.1.0` API.
- `rulery-analysis` gains `build_case_facts` and `CaseFactsError`, and `ApplicationService::root_facts`
  now delegates to it instead of nesting paths itself, so there is one implementation of the mapping.
- `docs/specification.md` changes only in the status cells of the six gate rows and in the paragraph
  that explains them. No schema tag, diagnostic code, hash vector, or Rust block changes.

## Testing

- Unit tests in `rulery-ir` cover canonical rendering and path collection, including negation,
  disjunction, and presence operators, in `condition_identity_ignores_spans_and_collects_every_path`.
- Unit tests in `rulery-analysis` cover domain derivation for every declaration form and boundary
  collection in `partition_specs_follow_vocabulary_and_authored_boundaries`, witness attachment and
  category assignment in `derived_evaluations_are_witnessed_and_categorized`, and budget exhaustion in
  `an_exhausted_state_budget_reports_inconclusive_without_claims`. `facts.rs` covers path nesting and
  the three collision rejections in `case_facts_nest_authored_paths_and_reject_conflicts`.
- Unit tests in `rulery-emit` cover column ordering, every cell shape the lowering produces, and
  truth-state derivation in `projections_order_columns_and_lower_each_comparison_shape`.
- Unit tests in the root crate drive `ApplicationService::analyze` and `::diff` from a compiled
  package and assert the emitted findings, the coverage ratio, the reachability statuses, and the
  diff classifications. `analyze_derives_reachability_interaction_and_coverage_findings` asserts the
  exact reachability statuses including an unreachable rule with a finite-partition-exhaustion proof,
  the six overlap classifications including a conflict, the coverage categories, and the exact
  diagnostic code set, then repeats the run and asserts byte-identical output.
  `analyze_without_explicit_domains_reports_only_presence_states` pins the consequence of the
  validator interaction described above, and `diff_derives_outcome_changes_between_compiled_packages`
  covers both a classified change and the unchanged self-comparison. They use a fixed instant and a
  fixed time-zone identity, and read neither the system clock nor the system time-zone database.

## Out Of Scope

- Symbolic constraint solving, SMT-backed reachability, and any non-exhaustive partition strategy.
- Finite domains for decimal, date, date-time, and duration values.
- Accepting scalar values against primitive and enum declarations in the vocabulary validator, which
  would change `V05` and `V06` behaviour and is part of the evaluation contract.
- New diagnostic codes, new `UncoveredCategory` values, and any change to `analyze_interactions`,
  `compute_coverage`, or `PolicyDiffer` behaviour.
- Changing the interaction analyzer's `ExplicitOverride` classification.
- Rendering analysis findings as human text or SARIF beyond the existing diagnostic envelope.

## Risk

- [ ] Breaking API changes: yes; `PartitionDomainKind` gains two variants, the diagnostic evidence
      structs gain constructors, and `ApplicationError::AnalysisUnavailable` is removed. All are
      within the pre-release `0.1.0` API.
- [ ] Serialization changes: no; the new variants serialize as `"unbounded"` and `"unsupported"`
      inside a value that is not part of any versioned envelope, and no wire struct changes shape.
- [ ] New external dependency: no.
- [ ] Feature flag required: no.
- [ ] Cross-crate risk: medium; the extraction layer reads engine traces, vocabulary declarations, and
      compiled rules, so a change to any of those three changes derived analysis input.
- [ ] Soundness risk: high; a derivation bug produces wrong coverage numbers rather than an error, so
      every category, completeness, and reachability decision is pinned in this note and asserted by
      a test that drives a real compiled package.
- [ ] Cost risk: medium; an exhaustive product over many referenced paths can be large, so the state
      budget must be enforced during enumeration rather than after it.
