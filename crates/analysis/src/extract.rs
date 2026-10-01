//! Derivation of every static-analysis input from a compiled package.
//!
//! The analyzer functions in this crate consume typed inputs, and nothing before this module
//! produced them from a [`CompiledPackage`]. Derivation is deliberately the only new logic here: a
//! wrong partition yields wrong coverage, reachability, and diff numbers without failing, so every
//! rule below is narrow, over-approximate where the claim is universal, and inconclusive where the
//! partition contract cannot represent a domain.
//!
//! Every cell is evaluated through the engine at a fixed instant and time-zone identity, so
//! reachability, overlap, conflict, and coverage claims are consequences of the same evaluated
//! cells rather than separate approximations of them.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use rulery_contracts::{
    ContentHash, DecisionId, EnumValue, FactPath, LanguageVersion, Outcome, OutcomeKind,
    QualifiedRuleId, StableId, TimeZoneDatabaseIdentity, TypeId, UtcInstant, Value,
};
use rulery_diagnostics::{
    AnalysisLimitEvidence, AnalysisPhase, BehaviorChangeEvidence, Diagnostic, DiagnosticCode,
    DiagnosticEvidence, DiagnosticReport, DiagnosticReportV1, FindingConfidence, ProofEvidence,
    WitnessEvidence,
};
use rulery_engine::{
    Candidate, DecisionTraceV1, ExplicitRanks, PolicyEvaluator, PrecedenceModel,
    ProductionPolicyEvaluator, StrategyKind, TimeZoneDatabase, TraceDetail, Truth,
};
use rulery_ir::{
    CompiledDecision, CompiledPackage, DecisionPrecedence, Expr, ExprOperand, Operator,
    PrecedenceDimension, ResolvedType, TypeDeclaration, canonical_condition, referenced_fact_paths,
};
use rulery_vocabulary::ResolvedVocabulary;

use crate::{
    AnalysisBudget, AnalysisCompleteness, AnalysisFinding, AnalysisOptions, AnalysisReport,
    AnalysisReportV1 as ReportV1, BudgetResult, CellEvaluation, CoverageReport, DecisionCoverage,
    DecisionPartition, DiffCell, FactPartitionValue, InteractionAnalysis, OutcomeChangeKind,
    PartitionCell, PartitionDomainKind, PartitionSpec, PolicyAnalyzer, PolicyDiff, PolicyDiffer,
    RuleAnalysisInput, StructuralChange, UncoveredCategory, ValueUsage, WitnessCase, WitnessClaim,
    analyze_interactions, build_case_facts, build_partition, compute_coverage, minimize_witness,
};

/// Nanoseconds of the fixed analysis instant.
///
/// The report port carries no clock parameter, so analysis uses a constant rather than reading a
/// clock. The Unix epoch is representable by the supported civil calendar in every time zone.
const ANALYSIS_INSTANT_NANOSECONDS: i128 = 0;

/// Producer identity recorded on analysis diagnostics.
const ANALYSIS_PRODUCER: &str = "rulery-analysis";

/// Maximum alias hops followed before a type chain is treated as unresolvable.
const MAX_ALIAS_HOPS: usize = 16;

/// Built-in type names a resolved vocabulary references without declaring them.
const BUILT_INS: &[&str] = &[
    "bool",
    "boolean",
    "int",
    "integer",
    "string",
    "text",
    "decimal",
    "date",
    "datetime",
    "date-time",
    "duration",
];

/// Returns the fixed instant used for every analysis evaluation.
///
/// The value is a constant rather than a clock read, so two reports of the same package are
/// byte-identical regardless of when they run.
#[must_use]
pub fn analysis_instant() -> UtcInstant {
    utc_instant(ANALYSIS_INSTANT_NANOSECONDS)
}

/// Builds the analysis instant, asserting the one invariant that makes it infallible.
///
/// `ANALYSIS_INSTANT_NANOSECONDS` is the Unix epoch, which is the origin of the supported civil
/// calendar, so it is inside the representable range by construction rather than by configuration.
fn utc_instant(nanoseconds: i128) -> UtcInstant {
    UtcInstant::new(nanoseconds).expect("analysis instant is inside the civil calendar range")
}

/// Fixed evaluation inputs shared by every derived analysis evaluation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnalysisContext {
    /// Fixed evaluation instant.
    pub at: UtcInstant,
    /// Time-zone database identity recorded in every witness.
    pub time_zones: TimeZoneDatabaseIdentity,
}

impl AnalysisContext {
    /// Builds a context from a time-zone port and the fixed instant.
    #[must_use]
    pub fn new(time_zones: &dyn TimeZoneDatabase, at: UtcInstant) -> Self {
        Self {
            at,
            time_zones: database_identity(time_zones),
        }
    }
}

/// Splits a port identity into the witness identity form the engine records.
fn database_identity(time_zones: &dyn TimeZoneDatabase) -> TimeZoneDatabaseIdentity {
    let identity = time_zones.identity();
    let (implementation, version) = identity
        .split_once('/')
        .unwrap_or((identity, "unspecified"));
    TimeZoneDatabaseIdentity::new(implementation, version).unwrap_or_else(|_| {
        TimeZoneDatabaseIdentity::new("unknown", "unspecified").expect("static identity")
    })
}

/// One evaluated partition cell, before it is projected into report input types.
#[derive(Clone, Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct CellOutcome {
    /// Facts supplied to the engine.
    pub facts: BTreeMap<FactPath, Value>,
    /// Assigned paths in ascending order.
    pub paths: BTreeSet<FactPath>,
    /// Rules whose condition evaluated to `True`.
    pub reached: BTreeSet<QualifiedRuleId>,
    /// Rules that determined the outcome.
    pub determining: BTreeSet<QualifiedRuleId>,
    /// Whether decision-relevant missing evidence remained.
    pub missing: bool,
    /// Whether decision-relevant invalid evidence remained.
    pub invalid: bool,
    /// Whether evaluation produced a runtime conflict.
    pub conflict: bool,
    /// Whether the declared default resolved the cell.
    pub defaulted: bool,
    /// Time-zone identity the engine recorded for this cell.
    pub time_zones: TimeZoneDatabaseIdentity,
}

impl CellOutcome {
    /// Whether decision-relevant unknown or invalid evidence remained.
    const fn uncertain(&self) -> bool {
        self.missing || self.invalid
    }

    /// Whether a non-default rule resolved the cell without residual uncertainty or conflict.
    fn covered(&self) -> bool {
        !self.determining.is_empty() && !self.uncertain() && !self.conflict
    }
}

/// Complete derived analysis input for one decision.
#[derive(Clone, Debug)]
pub struct DecisionAnalysisInput {
    /// Decision identity.
    pub decision: DecisionId,
    /// Finite partition for the decision.
    pub partition: DecisionPartition,
    /// One evaluation per enumerated partition cell.
    pub evaluations: Vec<CellEvaluation>,
    /// One input per compiled rule, in ascending rule order.
    pub rules: Vec<RuleAnalysisInput>,
    /// Witnesses referenced by the evaluations and rule inputs, in canonical hash order.
    pub witnesses: Vec<WitnessCase>,
}

impl DecisionAnalysisInput {
    /// Derives the partition, cell evaluations, and rule inputs for one decision.
    ///
    /// Cell evaluation and witness search are charged against `budget`, so an exhausted budget or
    /// an evaluation that cannot produce a trace leaves the partition incomplete, which suppresses
    /// the coverage percentage and rules out any unreachability claim. `witnesses` bounds witness
    /// generation for the whole command.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn derive(
        package: &CompiledPackage,
        decision: &CompiledDecision,
        evaluator: &dyn PolicyEvaluator,
        context: &AnalysisContext,
        options: &AnalysisOptions,
        budget: &mut AnalysisBudget<String, Option<CellOutcome>>,
        witnesses: &mut u32,
    ) -> Self {
        let shared = RefCell::new(budget);
        let partition = build_partition(
            decision.id().clone(),
            derive_partition_specs(package, decision),
            options,
        );

        let mut complete = partition.completeness == AnalysisCompleteness::Complete;
        let mut outcomes: Vec<(PartitionCell, CellOutcome)> = Vec::new();
        for cell in &partition.cells {
            let facts = cell_facts(cell, package.vocabulary());
            let paths = cell.assignments.keys().cloned().collect();
            let key = state_key(decision.id(), &facts);
            let BudgetResult::Value(outcome) = shared.borrow_mut().resolve(key, || {
                evaluate_facts(&facts, &paths, package, decision, evaluator, context)
            }) else {
                complete = false;
                break;
            };
            let Some(outcome) = outcome else {
                complete = false;
                break;
            };
            outcomes.push((cell.clone(), outcome));
        }

        let partition = if complete {
            partition
        } else {
            DecisionPartition {
                completeness: AnalysisCompleteness::Inconclusive {
                    examined: shared.borrow().examined(),
                    limit: options.max_states,
                },
                ..partition
            }
        };

        let evaluations: Vec<CellEvaluation> = outcomes
            .iter()
            .map(|(cell, outcome)| CellEvaluation {
                cell: cell.clone(),
                satisfiable: true,
                reached_rules: outcome.reached.clone(),
                determining_rules: outcome.determining.clone(),
                relevant_uncertainty: outcome.uncertain(),
                conflict: outcome.conflict,
                category: uncovered_category(cell, outcome, decision),
                witness: uncovered_witness(
                    outcome, package, decision, evaluator, context, &shared, witnesses,
                ),
            })
            .collect();

        let rules = if options.include_reachability || options.include_interactions {
            derive_rule_inputs(
                package, decision, &partition, &outcomes, evaluator, context, &shared, witnesses,
            )
        } else {
            Vec::new()
        };

        let mut collected = evaluations
            .iter()
            .map(|evaluation| evaluation.witness.clone())
            .chain(
                rules
                    .iter()
                    .filter(|rule| rule.satisfiable == Some(true))
                    .map(|rule| rule.witness.clone()),
            )
            .collect::<Vec<_>>();
        collected.sort_by_key(|witness| witness.hash);
        collected.dedup_by_key(|witness| witness.hash);
        Self {
            decision: decision.id().clone(),
            partition,
            evaluations,
            rules,
            witnesses: collected,
        }
    }
}

/// Derives one partition specification per referenced fact path of a decision.
#[must_use]
pub fn derive_partition_specs(
    package: &CompiledPackage,
    decision: &CompiledDecision,
) -> Vec<PartitionSpec> {
    let mut boundaries: BTreeMap<FactPath, Boundaries> = decision
        .rules()
        .values()
        .flat_map(|rule| referenced_fact_paths(rule.condition()))
        .map(|path| (path, Boundaries::default()))
        .collect();
    for rule in decision.rules().values() {
        collect_boundaries(rule.condition(), &mut boundaries);
    }
    boundaries
        .into_iter()
        .map(|(path, collected)| spec_for(path, &collected, package.vocabulary()))
        .collect()
}

/// Boundary literals collected for one referenced path.
#[derive(Clone, Debug, Default)]
struct Boundaries {
    /// Authored integer literals compared against the path.
    integers: Vec<i64>,
    /// Authored text literals compared against the path.
    texts: Vec<String>,
    /// How the decision's conditions use the path's value.
    usage: ValueUsage,
}

fn collect_boundaries(expression: &Expr, boundaries: &mut BTreeMap<FactPath, Boundaries>) {
    match expression {
        Expr::Constant { .. } => {}
        Expr::Predicate(predicate) => {
            let usage = usage_of(predicate.operator);
            if let ExprOperand::Fact(path) = &predicate.left
                && let Some(collected) = boundaries.get_mut(path)
            {
                collected.usage = collected.usage.max(usage);
                if let Some(literal) = &predicate.right {
                    collect_literal(literal, collected);
                }
            }
            if let (Some(ExprOperand::Fact(path)), ExprOperand::Literal(literal)) =
                (&predicate.right, &predicate.left)
                && let Some(collected) = boundaries.get_mut(path)
            {
                collected.usage = collected.usage.max(usage);
                collect_literal(&ExprOperand::Literal(literal.clone()), collected);
            }
        }
        Expr::All { expressions, .. } | Expr::Any { expressions, .. } => {
            for child in expressions {
                collect_boundaries(child, boundaries);
            }
        }
        Expr::Not { expression, .. } => collect_boundaries(expression, boundaries),
    }
}

/// Classifies what an operator does to a fact path's value.
///
/// Presence predicates leave the value uninspected. Equality against an authored literal bounds the
/// domain to that literal set. A partial or ordered match can distinguish values the domain omits,
/// which is the only case where a text domain stays partial.
fn usage_of(operator: Operator) -> ValueUsage {
    match operator {
        Operator::Exists | Operator::Missing => ValueUsage::PresenceOnly,
        Operator::Equals
        | Operator::NotEquals
        | Operator::IsOneOf
        | Operator::IsTrue
        | Operator::IsFalse => ValueUsage::BoundedByLiterals,
        Operator::Contains
        | Operator::StartsWith
        | Operator::EndsWith
        | Operator::Matches
        | Operator::LessThan
        | Operator::LessOrEqual
        | Operator::GreaterThan
        | Operator::GreaterOrEqual
        | Operator::Before
        | Operator::After
        | Operator::Between
        | Operator::OnOrBefore
        | Operator::OnOrAfter => ValueUsage::UnrepresentableValues,
    }
}

fn collect_literal(operand: &ExprOperand, boundaries: &mut Boundaries) {
    let ExprOperand::Literal(value) = operand else {
        return;
    };
    match value {
        Value::Integer(inner) => boundaries.integers.push(*inner),
        Value::Text(inner) => boundaries.texts.push(inner.clone()),
        Value::List(items) => {
            for item in items {
                collect_literal(&ExprOperand::Literal(item.clone()), boundaries);
            }
        }
        _ => {}
    }
}

fn spec_for(
    path: FactPath,
    boundaries: &Boundaries,
    vocabulary: &ResolvedVocabulary,
) -> PartitionSpec {
    let (kind, malformed) = domain_kind(&path, boundaries, vocabulary);
    // A record's only absence representation is an absent assignment: the vocabulary expresses
    // optionality as presence, and a null record cannot be represented as case facts alongside a
    // valid value on one of its own fields, because nesting a child under a null parent is a
    // path conflict. Assigning null here used to void the whole decision.
    let record = matches!(kind, PartitionDomainKind::Record);
    PartitionSpec {
        path,
        kind,
        allow_absent: true,
        allow_null: !record,
        allow_malformed: malformed.is_some(),
        forbidden: Vec::new(),
    }
}

fn domain_kind(
    path: &FactPath,
    boundaries: &Boundaries,
    vocabulary: &ResolvedVocabulary,
) -> (PartitionDomainKind, Option<u64>) {
    match resolve_path_type(path, vocabulary) {
        Some(PathType::Enum { id, variants }) => (
            PartitionDomainKind::Enum(
                variants
                    .into_iter()
                    .map(|variant| Value::Enum(EnumValue::new(id.clone(), variant)))
                    .collect(),
            ),
            None,
        ),
        Some(PathType::Primitive(name)) => (primitive_kind(name, boundaries), None),
        Some(PathType::List { malformed_length }) => (PartitionDomainKind::List, malformed_length),
        Some(PathType::Record) => (PartitionDomainKind::Record, None),
        None => (PartitionDomainKind::Unsupported, None),
    }
}

fn primitive_kind(name: &str, boundaries: &Boundaries) -> PartitionDomainKind {
    match name {
        "bool" | "boolean" => PartitionDomainKind::Boolean,
        "int" | "integer" if boundaries.integers.is_empty() => PartitionDomainKind::Unbounded,
        "int" | "integer" => PartitionDomainKind::Integer(boundaries.integers.clone()),
        "string" | "text" => PartitionDomainKind::Text {
            values: boundaries.texts.clone(),
            usage: boundaries.usage,
        },
        _ => PartitionDomainKind::Unsupported,
    }
}

/// Vocabulary shape of one referenced fact path.
#[derive(Clone, Debug)]
enum PathType {
    /// Declared enum type identity and variant symbols.
    Enum {
        /// Declared enum type identity.
        id: TypeId,
        /// Declared variant symbols.
        variants: Vec<StableId>,
    },
    /// Built-in primitive identified by its declared name.
    Primitive(&'static str),
    /// List type and the item count that makes supplied evidence out of range.
    List {
        /// Item count that makes supplied evidence malformed.
        malformed_length: Option<u64>,
    },
    /// Record type.
    Record,
}

/// Resolves one fact path to the shape its declared type gives it.
///
/// Relies on an invariant this crate does not itself establish: **at most one declared root can
/// prefix any given path**. That holds because `resolve_vocabulary` rejects a root nested under
/// another, and two prefixes of the same path are always prefix-ordered with respect to each other,
/// so the `find` below can never have two candidates to choose between. Taking the first match is
/// therefore not a precedence rule — it is the only match.
///
/// If that check is ever relaxed, this becomes silently order-dependent: a shorter root sorts first
/// in the `BTreeMap` and would resolve every shared path, leaving the nested root's declaration
/// inert. Preferring the longest match would then be the fix, and it would be a no-op today.
fn resolve_path_type(path: &FactPath, vocabulary: &ResolvedVocabulary) -> Option<PathType> {
    let (root_path, root) = vocabulary
        .roots
        .iter()
        .find(|(root_path, _)| path.segments().starts_with(root_path.segments()))?;
    let mut type_id = root.type_id.clone();
    for segment in &path.segments()[root_path.len()..] {
        let TypeDeclaration::Record { fields, .. } =
            &resolve_type(&type_id, vocabulary)?.declaration
        else {
            return None;
        };
        type_id = fields
            .iter()
            .find(|(name, _)| name.as_str() == segment.as_str())?
            .1
            .type_id
            .clone();
    }
    classify(&type_id, vocabulary)
}

fn classify(type_id: &TypeId, vocabulary: &ResolvedVocabulary) -> Option<PathType> {
    match resolve_type(type_id, vocabulary) {
        Some(resolved) => match &resolved.declaration {
            TypeDeclaration::Enum { id, variants } => Some(PathType::Enum {
                id: id.clone(),
                variants: variants
                    .iter()
                    .map(|variant| variant.symbol.clone())
                    .collect(),
            }),
            TypeDeclaration::Record { .. } => Some(PathType::Record),
            TypeDeclaration::List {
                min_items,
                max_items,
                ..
            } => Some(PathType::List {
                malformed_length: malformed_length(*min_items, *max_items),
            }),
            TypeDeclaration::Primitive => BUILT_INS
                .iter()
                .find(|candidate| **candidate == resolved.id.as_str())
                .map(|name| PathType::Primitive(name)),
            TypeDeclaration::Alias { .. } => None,
        },
        None => BUILT_INS
            .iter()
            .find(|candidate| **candidate == type_id.as_str())
            .map(|name| PathType::Primitive(name)),
    }
}

/// Item count whose supply violates a declared list length bound.
///
/// A declared maximum is violated by one extra item; otherwise a declared positive minimum is
/// violated by an empty list. A bound of zero items cannot be violated at all.
fn malformed_length(min_items: Option<u64>, max_items: Option<u64>) -> Option<u64> {
    if let Some(maximum) = max_items {
        return maximum.checked_add(1);
    }
    min_items.filter(|minimum| *minimum > 0)
}

fn resolve_type(type_id: &TypeId, vocabulary: &ResolvedVocabulary) -> Option<ResolvedType> {
    let mut current = type_id.clone();
    for _ in 0..MAX_ALIAS_HOPS {
        let resolved = vocabulary.types.get(&current)?.clone();
        match &resolved.declaration {
            TypeDeclaration::Alias { target, .. } => current = target.clone(),
            _ => return Some(resolved),
        }
    }
    None
}

fn state_key(decision: &DecisionId, assignments: &BTreeMap<FactPath, Value>) -> String {
    let bytes = serde_json::to_vec(assignments).unwrap_or_default();
    format!("{decision}|{}", String::from_utf8_lossy(&bytes))
}

fn reached_rules(trace: &DecisionTraceV1) -> BTreeSet<QualifiedRuleId> {
    trace
        .rule_traces
        .iter()
        .filter(|rule| rule.result == Truth::True)
        .map(|rule| rule.rule.clone())
        .collect()
}

/// Projects one partition cell into the facts an evaluator receives.
///
/// Absent assignments are omitted, null assignments become explicit nulls, and a malformed
/// assignment becomes the out-of-range evidence a declared list length bound produces. The
/// partition only declares a malformed cell when such evidence exists.
fn cell_facts(cell: &PartitionCell, vocabulary: &ResolvedVocabulary) -> BTreeMap<FactPath, Value> {
    cell.assignments
        .iter()
        .filter_map(|(path, value)| match value {
            FactPartitionValue::Absent => None,
            FactPartitionValue::Null => Some((path.clone(), Value::Null)),
            FactPartitionValue::Valid(value) => Some((path.clone(), value.clone())),
            FactPartitionValue::Malformed(_) => {
                resolve_path_type(path, vocabulary).and_then(|resolved| match resolved {
                    PathType::List {
                        malformed_length: Some(length),
                    } => Some((path.clone(), malformed_list(length))),
                    _ => None,
                })
            }
        })
        .collect()
}

fn malformed_list(length: u64) -> Value {
    let length = usize::try_from(length).unwrap_or(usize::MAX);
    Value::List(vec![Value::Null; length])
}

fn uncovered_category(
    cell: &PartitionCell,
    outcome: &CellOutcome,
    decision: &CompiledDecision,
) -> Option<UncoveredCategory> {
    if outcome.covered() {
        return None;
    }
    if outcome.invalid {
        return Some(UncoveredCategory::InvalidFact);
    }
    if outcome.missing {
        return Some(UncoveredCategory::MissingFact);
    }
    if has_unhandled_variant(cell, decision) {
        return Some(UncoveredCategory::UnhandledEnumVariant);
    }
    if outcome.defaulted {
        return Some(UncoveredCategory::DefaultOnly);
    }
    Some(UncoveredCategory::NoMatchingRule)
}

/// Whether a cell assigns a declared enum variant that no rule of the decision references.
fn has_unhandled_variant(cell: &PartitionCell, decision: &CompiledDecision) -> bool {
    cell.assignments.iter().any(|(path, value)| {
        let FactPartitionValue::Valid(Value::Enum(assigned)) = value else {
            return false;
        };
        !decision
            .rules()
            .values()
            .any(|rule| condition_mentions(rule.condition(), path, assigned.variant()))
    })
}

fn condition_mentions(expression: &Expr, path: &FactPath, variant: &StableId) -> bool {
    match expression {
        Expr::Constant { .. } => false,
        Expr::Predicate(predicate) => {
            matches!(&predicate.left, ExprOperand::Fact(fact) if fact == path)
                && predicate
                    .right
                    .as_ref()
                    .is_some_and(|operand| operand_mentions(operand, variant))
        }
        Expr::All { expressions, .. } | Expr::Any { expressions, .. } => expressions
            .iter()
            .any(|child| condition_mentions(child, path, variant)),
        Expr::Not { expression, .. } => condition_mentions(expression, path, variant),
    }
}

fn operand_mentions(operand: &ExprOperand, variant: &StableId) -> bool {
    match operand {
        ExprOperand::Literal(Value::Enum(value)) => value.variant() == variant,
        ExprOperand::Literal(Value::List(items)) => items
            .iter()
            .any(|item| matches!(item, Value::Enum(value) if value.variant() == variant)),
        _ => false,
    }
}

/// Builds the minimized witness for one uncovered cell.
#[allow(clippy::too_many_arguments)]
fn uncovered_witness(
    outcome: &CellOutcome,
    package: &CompiledPackage,
    decision: &CompiledDecision,
    evaluator: &dyn PolicyEvaluator,
    context: &AnalysisContext,
    budget: &RefCell<&mut AnalysisBudget<String, Option<CellOutcome>>>,
    witnesses: &mut u32,
) -> WitnessCase {
    minimize(
        format!("uncovered `{}`", decision.id()),
        outcome.facts.clone(),
        outcome.paths.clone(),
        outcome.time_zones.clone(),
        WitnessClaim::Uncovered,
        context,
        witnesses,
        |facts| {
            replay_cell(facts, package, decision, evaluator, context, budget)
                .is_some_and(|replay| !replay.covered())
        },
    )
}

/// Builds the minimized witness for one reachable rule.
#[allow(clippy::too_many_arguments)]
fn rule_witness(
    rule: &QualifiedRuleId,
    outcome: &CellOutcome,
    package: &CompiledPackage,
    decision: &CompiledDecision,
    evaluator: &dyn PolicyEvaluator,
    context: &AnalysisContext,
    budget: &RefCell<&mut AnalysisBudget<String, Option<CellOutcome>>>,
    witnesses: &mut u32,
) -> WitnessCase {
    minimize(
        format!("reachable `{rule}`"),
        outcome.facts.clone(),
        outcome.paths.clone(),
        outcome.time_zones.clone(),
        WitnessClaim::ReachableRule(rule.clone()),
        context,
        witnesses,
        |facts| {
            replay_cell(facts, package, decision, evaluator, context, budget)
                .is_some_and(|replay| replay.reached.contains(rule))
        },
    )
}

/// Minimizes a witness while the witness budget lasts, then keeps the unminimized assignment.
#[allow(clippy::too_many_arguments)]
fn minimize(
    title: String,
    facts: BTreeMap<FactPath, Value>,
    paths: BTreeSet<FactPath>,
    time_zones: TimeZoneDatabaseIdentity,
    claim: WitnessClaim,
    context: &AnalysisContext,
    witnesses: &mut u32,
    replays: impl Fn(&BTreeMap<FactPath, Value>) -> bool,
) -> WitnessCase {
    if *witnesses == 0 {
        return minimize_witness(
            title,
            facts,
            context.at,
            time_zones,
            paths,
            claim,
            |_, _, _| true,
        );
    }
    *witnesses -= 1;
    minimize_witness(
        title,
        facts,
        context.at,
        time_zones,
        paths,
        claim,
        |facts, _, _| replays(facts),
    )
}

/// Re-evaluates one candidate fact assignment under the shared state budget.
#[allow(clippy::too_many_arguments)]
fn replay_cell(
    facts: &BTreeMap<FactPath, Value>,
    package: &CompiledPackage,
    decision: &CompiledDecision,
    evaluator: &dyn PolicyEvaluator,
    context: &AnalysisContext,
    budget: &RefCell<&mut AnalysisBudget<String, Option<CellOutcome>>>,
) -> Option<CellOutcome> {
    let paths = facts.keys().cloned().collect();
    let key = state_key(decision.id(), facts);
    let BudgetResult::Value(outcome) = budget.borrow_mut().resolve(key, || {
        evaluate_facts(facts, &paths, package, decision, evaluator, context)
    }) else {
        return None;
    };
    outcome
}

fn evaluate_facts(
    facts: &BTreeMap<FactPath, Value>,
    paths: &BTreeSet<FactPath>,
    package: &CompiledPackage,
    decision: &CompiledDecision,
    evaluator: &dyn PolicyEvaluator,
    context: &AnalysisContext,
) -> Option<CellOutcome> {
    let case_facts = build_case_facts(facts).ok()?;
    let trace = evaluator
        .evaluate(
            package,
            decision.id(),
            &case_facts,
            context.at,
            TraceDetail::Complete,
        )
        .ok()?;
    let payload = trace.payload();
    Some(CellOutcome {
        paths: paths.clone(),
        reached: reached_rules(payload),
        determining: payload.determining_rules.iter().cloned().collect(),
        missing: !payload.missing_facts.is_empty(),
        invalid: payload.strategy_applications.iter().any(|application| {
            application.strategy == StrategyKind::Invalid && !application.relevant_paths.is_empty()
        }),
        conflict: payload.conflict.is_some(),
        defaulted: payload.outcome.is_some() && payload.determining_rules.is_empty(),
        time_zones: payload.timezone_database.clone(),
        facts: facts.clone(),
    })
}

#[allow(clippy::too_many_arguments)]
fn derive_rule_inputs(
    package: &CompiledPackage,
    decision: &CompiledDecision,
    partition: &DecisionPartition,
    outcomes: &[(PartitionCell, CellOutcome)],
    evaluator: &dyn PolicyEvaluator,
    context: &AnalysisContext,
    budget: &RefCell<&mut AnalysisBudget<String, Option<CellOutcome>>>,
    witnesses: &mut u32,
) -> Vec<RuleAnalysisInput> {
    let complete = partition.completeness == AnalysisCompleteness::Complete;
    let identities = decision
        .rules()
        .values()
        .map(|rule| rule.qualified_id().clone())
        .collect::<Vec<_>>();
    let fallback = match outcomes.first() {
        Some((_, outcome)) => plain_witness(outcome.facts.clone(), outcome.paths.clone(), context),
        None => plain_witness(BTreeMap::new(), BTreeSet::new(), context),
    };

    let mut inputs = Vec::new();
    for (index, rule) in decision.rules().values().enumerate() {
        let rule_id = rule.qualified_id().clone();
        let reached = outcomes
            .iter()
            .filter(|(_, outcome)| outcome.reached.contains(&rule_id))
            .collect::<Vec<_>>();
        let satisfiable = if !reached.is_empty() {
            Some(true)
        } else if complete {
            Some(false)
        } else {
            None
        };
        let witness = match reached.first() {
            Some((_, outcome)) => rule_witness(
                &rule_id, outcome, package, decision, evaluator, context, budget, witnesses,
            ),
            None => fallback.clone(),
        };
        let mut overlaps = BTreeSet::new();
        for other in identities.iter().skip(index + 1) {
            if outcomes.iter().any(|(_, outcome)| {
                outcome.reached.contains(&rule_id) && outcome.reached.contains(other)
            }) {
                overlaps.insert(other.clone());
            }
        }
        inputs.push(RuleAnalysisInput {
            candidate: Candidate {
                rule: rule_id,
                outcome: rule.outcome().clone(),
                priority: rule.priority(),
                specificity: rule.specificity(),
                override_rank: rule.override_rank(),
            },
            satisfiable,
            condition_hash: ContentHash::digest(canonical_condition(rule.condition()).as_bytes()),
            overlaps,
            explicit_override: rule.override_rank() == 1,
            undeclared_override: false,
            witness,
        });
    }
    inputs
}

fn plain_witness(
    facts: BTreeMap<FactPath, Value>,
    paths: BTreeSet<FactPath>,
    context: &AnalysisContext,
) -> WitnessCase {
    minimize_witness(
        "unexamined",
        facts,
        context.at,
        context.time_zones.clone(),
        paths,
        WitnessClaim::Uncovered,
        |_, _, _| true,
    )
}

fn precedence_model(value: &DecisionPrecedence) -> PrecedenceModel {
    match value {
        DecisionPrecedence::SafetyFirst => PrecedenceModel::SafetyFirst,
        DecisionPrecedence::PriorityFirst => PrecedenceModel::PriorityFirst,
        DecisionPrecedence::Explicit {
            primary,
            outcome_ranks,
        } => {
            let ranks = ExplicitRanks::new(
                outcome_ranks
                    .iter()
                    .map(|(kind, rank)| (*kind, *rank))
                    .collect(),
            )
            .expect("compiled decision semantics validate every explicit outcome rank");
            match primary {
                PrecedenceDimension::Outcome => PrecedenceModel::ExplicitOutcome(ranks),
                PrecedenceDimension::Priority => PrecedenceModel::ExplicitPriority(ranks),
            }
        }
    }
}

/// Package analyzer bound to one time-zone database and instant.
pub struct PackageAnalyzer<'a> {
    evaluator: ProductionPolicyEvaluator<'a>,
    context: AnalysisContext,
}

impl core::fmt::Debug for PackageAnalyzer<'_> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("PackageAnalyzer")
            .field("at", &self.context.at)
            .field("time_zones", &self.context.time_zones)
            .finish()
    }
}

impl<'a> PackageAnalyzer<'a> {
    /// Creates an analyzer that evaluates every cell at `at` through `time_zones`.
    #[must_use]
    pub fn new(time_zones: &'a dyn TimeZoneDatabase, at: UtcInstant) -> Self {
        Self {
            evaluator: ProductionPolicyEvaluator::new(time_zones),
            context: AnalysisContext::new(time_zones, at),
        }
    }

    /// Compares two compiled packages over the union of their affected partitions.
    ///
    /// A decision the diff cannot pair cell by cell is reported as inconclusive rather than
    /// unchanged: either it exists on one side only, or a side resolved a cell to a runtime
    /// conflict, which has no outcome to pair.
    #[must_use]
    pub fn diff(
        &self,
        before: &CompiledPackage,
        after: &CompiledPackage,
        decision: Option<&DecisionId>,
        options: &AnalysisOptions,
    ) -> AnalysisReport {
        let mut diffs = Vec::new();
        let mut diagnostics = Vec::new();
        let mut complete = true;
        let limit = usize::try_from(options.max_states).unwrap_or(usize::MAX);
        let mut witnesses = options.max_witnesses;

        for id in decision_ids(before, after, decision) {
            let (Some(left), Some(right)) = (
                before.payload().decisions().get(&id),
                after.payload().decisions().get(&id),
            ) else {
                complete = false;
                diagnostics.push(inconclusive(
                    &id,
                    AnalysisPhase::DiffEnumeration,
                    options,
                    0,
                    Vec::new(),
                ));
                continue;
            };
            let partition = build_partition(
                id.clone(),
                union_specs(
                    derive_partition_specs(before, left),
                    derive_partition_specs(after, right),
                ),
                options,
            );
            let structural = structural_changes(before, after, left, right);
            let mut cells = Vec::new();
            let mut pairable = true;
            for cell in partition.cells.iter().take(limit.saturating_add(1)) {
                let Some(pair) = pair_outcomes(
                    &self.evaluator,
                    before,
                    left,
                    after,
                    right,
                    cell,
                    &self.context,
                ) else {
                    pairable = false;
                    break;
                };
                let paired = pair_structural(&pair, structural.clone());
                let witness = behavior_witness(
                    &pair.0.facts,
                    before,
                    left,
                    after,
                    right,
                    &self.evaluator,
                    &self.context,
                    &mut witnesses,
                );
                cells.push(DiffCell {
                    decision: id.clone(),
                    before: pair.0.outcome,
                    after: pair.1.outcome,
                    structural: paired,
                    witness,
                });
            }
            if !pairable {
                complete = false;
                diagnostics.push(inconclusive(
                    &id,
                    AnalysisPhase::DiffEnumeration,
                    options,
                    cells.len() as u64,
                    partition.paths,
                ));
                continue;
            }
            let report = PolicyDiffer.diff(id.clone(), cells, options.max_states);
            complete &= report.completeness == AnalysisCompleteness::Complete;
            diagnostics.extend(change_diagnostics(&report));
            diffs.push(report);
        }

        AnalysisReport::new(ReportV1 {
            package_hash: after.payload().package_hash(),
            options: options.clone(),
            completeness: completeness(options, 0, complete),
            diagnostics: diagnostic_report(&diagnostics, after.payload().language_version()),
            reachability: Vec::new(),
            overlaps: Vec::new(),
            coverage: Vec::new(),
            witnesses: Vec::new(),
            semantic_diffs: diffs,
        })
    }

    /// Runs the whole static analysis pass over every decision of one package.
    fn report(&self, package: &CompiledPackage, options: &AnalysisOptions) -> AnalysisReport {
        let mut budget = AnalysisBudget::new(options.max_states);
        let mut witnesses = options.max_witnesses;
        let mut complete = true;
        let mut reachability = Vec::new();
        let mut overlaps = Vec::new();
        let mut coverage = Vec::new();
        let mut collected = Vec::new();
        let mut diagnostics = Vec::new();

        for decision in package.payload().decisions().values() {
            let input = DecisionAnalysisInput::derive(
                package,
                decision,
                &self.evaluator,
                &self.context,
                options,
                &mut budget,
                &mut witnesses,
            );
            let partition_complete = input.partition.completeness == AnalysisCompleteness::Complete;
            if !partition_complete {
                diagnostics.push(inconclusive(
                    decision.id(),
                    AnalysisPhase::CoverageEnumeration,
                    options,
                    budget.examined(),
                    input.partition.paths.clone(),
                ));
            }
            if options.include_coverage {
                let report = compute_coverage(&input.partition, input.evaluations.clone());
                complete &= report.completeness == AnalysisCompleteness::Complete;
                diagnostics.extend(coverage_diagnostics(decision.id(), &report));
                coverage.push(DecisionCoverage {
                    decision: decision.id().clone(),
                    report,
                });
            }
            if options.include_reachability || options.include_interactions {
                let interaction = analyze_interactions(
                    &precedence_model(decision.semantics().precedence()),
                    input.rules.clone(),
                );
                diagnostics.extend(interaction_diagnostics(decision.id(), &interaction));
                if options.include_reachability {
                    reachability.extend(interaction.reachability.iter().cloned());
                }
                if options.include_interactions {
                    overlaps.extend(interaction.overlaps.iter().cloned());
                }
            }
            complete &= partition_complete;
            collected.extend(input.witnesses.iter().cloned());
        }

        AnalysisReport::new(ReportV1 {
            package_hash: package.payload().package_hash(),
            options: options.clone(),
            completeness: completeness(options, budget.examined(), complete),
            diagnostics: diagnostic_report(&diagnostics, package.payload().language_version()),
            reachability,
            overlaps,
            coverage,
            witnesses: collected,
            semantic_diffs: Vec::new(),
        })
    }
}

impl PolicyAnalyzer for PackageAnalyzer<'_> {
    fn analyze(&self, package: &CompiledPackage, options: &AnalysisOptions) -> AnalysisReport {
        self.report(package, options)
    }
}

fn completeness(options: &AnalysisOptions, examined: u64, complete: bool) -> AnalysisCompleteness {
    if complete {
        AnalysisCompleteness::Complete
    } else {
        AnalysisCompleteness::Inconclusive {
            examined,
            limit: options.max_states,
        }
    }
}

fn diagnostic_report(diagnostics: &[Diagnostic], language: LanguageVersion) -> DiagnosticReportV1 {
    DiagnosticReport::new(diagnostics.to_vec(), ANALYSIS_PRODUCER, language)
        .expect("analysis producer identity is not empty")
        .payload()
        .clone()
}

fn decision_ids(
    before: &CompiledPackage,
    after: &CompiledPackage,
    selected: Option<&DecisionId>,
) -> Vec<DecisionId> {
    before
        .payload()
        .decisions()
        .keys()
        .chain(after.payload().decisions().keys())
        .filter(|id| selected.is_none_or(|wanted| wanted == *id))
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn union_specs(before: Vec<PartitionSpec>, after: Vec<PartitionSpec>) -> Vec<PartitionSpec> {
    let mut union: BTreeMap<FactPath, PartitionSpec> = BTreeMap::new();
    for spec in before.into_iter().chain(after) {
        match union.get_mut(&spec.path) {
            Some(existing) => {
                existing.kind = merge_kinds(&existing.kind, &spec.kind);
                existing.allow_absent |= spec.allow_absent;
                existing.allow_null |= spec.allow_null;
                existing.allow_malformed |= spec.allow_malformed;
                existing.forbidden.extend(spec.forbidden.iter().cloned());
            }
            None => {
                union.insert(spec.path.clone(), spec);
            }
        }
    }
    union.into_values().collect()
}

fn merge_kinds(left: &PartitionDomainKind, right: &PartitionDomainKind) -> PartitionDomainKind {
    match (left, right) {
        (PartitionDomainKind::Boolean, PartitionDomainKind::Boolean) => {
            PartitionDomainKind::Boolean
        }
        (PartitionDomainKind::Enum(left), PartitionDomainKind::Enum(right)) => {
            let mut variants = left.clone();
            variants.extend(right.iter().cloned());
            PartitionDomainKind::Enum(variants)
        }
        (PartitionDomainKind::Integer(left), PartitionDomainKind::Integer(right)) => {
            let mut boundaries = left.clone();
            boundaries.extend(right.iter().copied());
            PartitionDomainKind::Integer(boundaries)
        }
        (
            PartitionDomainKind::Text {
                values: left,
                usage: left_usage,
            },
            PartitionDomainKind::Text {
                values: right,
                usage: right_usage,
            },
        ) => {
            let mut boundaries = left.clone();
            boundaries.extend(right.iter().cloned());
            PartitionDomainKind::Text {
                values: boundaries,
                usage: (*left_usage).max(*right_usage),
            }
        }
        (PartitionDomainKind::Unbounded, PartitionDomainKind::Unbounded) => {
            PartitionDomainKind::Unbounded
        }
        _ => PartitionDomainKind::Unsupported,
    }
}

/// One paired union-cell outcome.
type Paired = (PairedOutcome, PairedOutcome);

/// Outcome of one side of a union-cell evaluation.
struct PairedOutcome {
    /// Resolved outcome.
    outcome: Outcome,
    /// Facts the cell supplied.
    facts: BTreeMap<FactPath, Value>,
}

#[allow(clippy::too_many_arguments)]
fn pair_outcomes(
    evaluator: &dyn PolicyEvaluator,
    before: &CompiledPackage,
    left: &CompiledDecision,
    after: &CompiledPackage,
    right: &CompiledDecision,
    cell: &PartitionCell,
    context: &AnalysisContext,
) -> Option<Paired> {
    let paired = |package: &CompiledPackage, decision: &CompiledDecision| {
        let facts = cell_facts(cell, package.vocabulary());
        let case_facts = build_case_facts(&facts).ok()?;
        let outcome = evaluator
            .evaluate(
                package,
                decision.id(),
                &case_facts,
                context.at,
                TraceDetail::Complete,
            )
            .ok()?
            .payload()
            .outcome
            .clone()?;
        Some(PairedOutcome { outcome, facts })
    };
    Some((paired(before, left)?, paired(after, right)?))
}

fn pair_structural(
    pair: &Paired,
    mut structural: BTreeSet<StructuralChange>,
) -> BTreeSet<StructuralChange> {
    if pair.0.outcome.kind() != pair.1.outcome.kind() {
        return structural;
    }
    if reason_codes(&pair.0.outcome) != reason_codes(&pair.1.outcome) {
        structural.insert(StructuralChange::Reasons);
    }
    if required_paths(&pair.0.outcome) != required_paths(&pair.1.outcome) {
        structural.insert(StructuralChange::RequiredFacts);
    }
    // An approval or denial outcome carries nothing beyond its reasons and its action list, so an
    // inequality there is exactly an action change. An escalation destination change is left
    // unclassified rather than being reported as an action change.
    if structural.is_empty() && has_actions(&pair.0.outcome) && pair.0.outcome != pair.1.outcome {
        structural.insert(StructuralChange::Actions);
    }
    structural
}

#[allow(clippy::too_many_arguments)]
fn behavior_witness(
    facts: &BTreeMap<FactPath, Value>,
    before: &CompiledPackage,
    left: &CompiledDecision,
    after: &CompiledPackage,
    right: &CompiledDecision,
    evaluator: &dyn PolicyEvaluator,
    context: &AnalysisContext,
    witnesses: &mut u32,
) -> WitnessCase {
    let paths = facts.keys().cloned().collect();
    let time_zones = context.time_zones.clone();
    if *witnesses == 0 {
        return minimize_witness(
            "behavior change",
            facts.clone(),
            context.at,
            time_zones,
            paths,
            WitnessClaim::BehaviorChange,
            |_, _, _| true,
        );
    }
    *witnesses -= 1;
    minimize_witness(
        "behavior change",
        facts.clone(),
        context.at,
        time_zones,
        paths,
        WitnessClaim::BehaviorChange,
        |candidate, _, _| {
            let left_outcome = paired_outcome(candidate, before, left, evaluator, context);
            let right_outcome = paired_outcome(candidate, after, right, evaluator, context);
            matches!((left_outcome, right_outcome), (Some(left), Some(right)) if left != right)
        },
    )
}

fn paired_outcome(
    facts: &BTreeMap<FactPath, Value>,
    package: &CompiledPackage,
    decision: &CompiledDecision,
    evaluator: &dyn PolicyEvaluator,
    context: &AnalysisContext,
) -> Option<Outcome> {
    let case_facts = build_case_facts(facts).ok()?;
    evaluator
        .evaluate(
            package,
            decision.id(),
            &case_facts,
            context.at,
            TraceDetail::Complete,
        )
        .ok()?
        .payload()
        .outcome
        .clone()
}

fn reason_codes(outcome: &Outcome) -> Vec<String> {
    match outcome {
        Outcome::Approve { reasons, .. }
        | Outcome::Deny { reasons, .. }
        | Outcome::Escalate { reasons, .. }
        | Outcome::RequestInformation { reasons, .. } => reasons
            .iter()
            .map(|reason| reason.code().as_str().to_owned())
            .collect(),
    }
}

fn required_paths(outcome: &Outcome) -> BTreeSet<FactPath> {
    match outcome {
        Outcome::RequestInformation { required_facts, .. } => required_facts.paths().clone(),
        _ => BTreeSet::new(),
    }
}

fn has_actions(outcome: &Outcome) -> bool {
    matches!(outcome, Outcome::Approve { .. } | Outcome::Deny { .. })
}

const fn outcome_name(kind: OutcomeKind) -> &'static str {
    match kind {
        OutcomeKind::Approve => "approve",
        OutcomeKind::Deny => "deny",
        OutcomeKind::Escalate => "escalate",
        OutcomeKind::RequestInformation => "request_information",
    }
}

fn structural_changes(
    before: &CompiledPackage,
    after: &CompiledPackage,
    left: &CompiledDecision,
    right: &CompiledDecision,
) -> BTreeSet<StructuralChange> {
    let mut changes = BTreeSet::new();
    if left.default() != right.default() {
        changes.insert(StructuralChange::Default);
    }
    if left.semantics().precedence() != right.semantics().precedence() {
        changes.insert(StructuralChange::Precedence);
    }
    if left.semantics().missing_facts() != right.semantics().missing_facts() {
        changes.insert(StructuralChange::MissingStrategy);
    }
    if left.semantics().invalid_facts() != right.semantics().invalid_facts() {
        changes.insert(StructuralChange::InvalidStrategy);
    }
    if left.semantics().timezone() != right.semantics().timezone() {
        changes.insert(StructuralChange::Timezone);
    }
    if left.semantics().expiry() != right.semantics().expiry() {
        changes.insert(StructuralChange::Expiry);
    }
    if before.vocabulary() != after.vocabulary() {
        changes.insert(StructuralChange::Vocabulary);
    }
    if before.payload().integrity() != after.payload().integrity() {
        changes.insert(StructuralChange::ImportIntegrity);
    }
    changes
}

fn inconclusive(
    decision: &DecisionId,
    phase: AnalysisPhase,
    options: &AnalysisOptions,
    observed: u64,
    paths: Vec<FactPath>,
) -> Diagnostic {
    Diagnostic::builder(
        DiagnosticCode::new(DiagnosticCode::ANALYSIS_INCONCLUSIVE).expect("known code"),
        format!("analysis of `{decision}` is inconclusive"),
    )
    .expect("known code")
    .confidence(FindingConfidence::Inconclusive)
    .push_evidence(DiagnosticEvidence::AnalysisLimit(
        AnalysisLimitEvidence::new(phase, "max_states", options.max_states, observed, paths),
    ))
    .build()
    .expect("registry evidence")
}

/// Emits one diagnostic per uncovered category, anchored on its canonically first case.
///
/// `InvalidFact` and `UnsupportedDomain` are omitted because their registry code requires
/// analysis-limit evidence, which the decision-level `RUL254` already carries; the cases themselves
/// remain in the structured coverage section with their witnesses.
fn coverage_diagnostics(decision: &DecisionId, report: &CoverageReport) -> Vec<Diagnostic> {
    let mut seen: BTreeSet<&'static str> = BTreeSet::new();
    let mut diagnostics = Vec::new();
    for case in &report.uncovered {
        if matches!(
            case.category,
            UncoveredCategory::InvalidFact | UncoveredCategory::UnsupportedDomain
        ) || !seen.insert(category_code(case.category))
        {
            continue;
        }
        let count = report
            .uncovered
            .iter()
            .filter(|other| other.category == case.category)
            .count();
        let Some(diagnostic) = Diagnostic::builder(
            DiagnosticCode::new(category_code(case.category)).expect("known code"),
            format!(
                "decision `{decision}` leaves {count} case(s) uncovered as {}",
                category_name(case.category)
            ),
        )
        .ok()
        .map(|builder| {
            builder
                .confidence(FindingConfidence::Witnessed)
                .push_evidence(DiagnosticEvidence::Witness(witness_evidence(
                    &case.witness,
                    decision,
                )))
                .build()
        }) else {
            continue;
        };
        if let Ok(diagnostic) = diagnostic {
            diagnostics.push(diagnostic);
        }
    }
    diagnostics
}

fn category_code(category: UncoveredCategory) -> &'static str {
    match category {
        UncoveredCategory::DefaultOnly | UncoveredCategory::NoMatchingRule => "RUL250",
        UncoveredCategory::UnhandledEnumVariant => "RUL251",
        UncoveredCategory::MissingFact => "RUL252",
        UncoveredCategory::TemporalBoundary => "RUL253",
        UncoveredCategory::InvalidFact | UncoveredCategory::UnsupportedDomain => "RUL254",
    }
}

fn category_name(category: UncoveredCategory) -> &'static str {
    match category {
        UncoveredCategory::DefaultOnly => "default_only",
        UncoveredCategory::UnhandledEnumVariant => "unhandled_enum_variant",
        UncoveredCategory::MissingFact => "missing_fact",
        UncoveredCategory::TemporalBoundary => "temporal_boundary",
        UncoveredCategory::NoMatchingRule => "no_matching_rule",
        UncoveredCategory::InvalidFact => "invalid_fact",
        UncoveredCategory::UnsupportedDomain => "unsupported_domain",
    }
}

fn interaction_diagnostics(
    decision: &DecisionId,
    analysis: &InteractionAnalysis,
) -> Vec<Diagnostic> {
    analysis
        .findings
        .iter()
        .filter_map(|finding| finding_diagnostic(decision, finding, analysis))
        .collect()
}

fn finding_diagnostic(
    decision: &DecisionId,
    finding: &AnalysisFinding,
    analysis: &InteractionAnalysis,
) -> Option<Diagnostic> {
    let code = DiagnosticCode::new(finding.code).ok()?;
    let confidence = if finding.code == DiagnosticCode::UNREACHABLE_RULE {
        FindingConfidence::Proven
    } else {
        FindingConfidence::Witnessed
    };
    let builder = Diagnostic::builder(
        code,
        format!(
            "rules `{}` in `{decision}` need justification",
            finding
                .rules
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    )
    .ok()?
    .confidence(confidence);
    if finding.code == DiagnosticCode::UNREACHABLE_RULE {
        let proof = analysis
            .reachability
            .iter()
            .find(|entry| entry.rule == finding.rules[0])
            .and_then(|entry| entry.proof.as_ref())?;
        return builder
            .push_evidence(DiagnosticEvidence::Proof(ProofEvidence::new(
                proof.method.clone(),
                proof.constraints_hash,
            )))
            .build()
            .ok();
    }
    let witness = analysis
        .overlaps
        .iter()
        .find(|overlap| overlap.left == finding.rules[0])
        .map(|overlap| overlap.witness.clone())
        .or_else(|| {
            analysis
                .reachability
                .iter()
                .find(|entry| entry.rule == finding.rules[0])
                .and_then(|entry| entry.witness.clone())
        })?;
    builder
        .push_evidence(DiagnosticEvidence::Witness(witness_evidence(
            &witness, decision,
        )))
        .build()
        .ok()
}

fn witness_evidence(witness: &WitnessCase, decision: &DecisionId) -> WitnessEvidence {
    WitnessEvidence::new(
        StableId::new("analysis.witness").expect("stable id"),
        witness.title.clone(),
        serde_json::to_string(&witness.facts).unwrap_or_default(),
        witness.hash,
        decision.clone(),
        match &witness.claim {
            WitnessClaim::ReachableRule(rule) => vec![rule.clone()],
            WitnessClaim::RuleOverlap(left, right) => vec![left.clone(), right.clone()],
            WitnessClaim::Conflict(rules) => rules.clone(),
            WitnessClaim::Uncovered | WitnessClaim::BehaviorChange => Vec::new(),
        },
        Vec::new(),
    )
}

fn change_diagnostics(report: &PolicyDiff) -> Vec<Diagnostic> {
    report
        .changes
        .iter()
        .filter_map(|change| {
            Diagnostic::builder(
                DiagnosticCode::new(change.diagnostic_code.as_str()).ok()?,
                format!(
                    "decision `{}` changes from `{}` to `{}` as {}",
                    report.decision,
                    outcome_name(change.before.kind()),
                    outcome_name(change.after.kind()),
                    change_name(change.classification)
                ),
            )
            .ok()?
            .confidence(FindingConfidence::Witnessed)
            .push_evidence(DiagnosticEvidence::BehaviorChange(
                BehaviorChangeEvidence::new(
                    change.before.kind(),
                    change.after.kind(),
                    Vec::new(),
                    Vec::new(),
                    change.witness.hash,
                ),
            ))
            .build()
            .ok()
        })
        .collect()
}

const fn change_name(classification: OutcomeChangeKind) -> &'static str {
    match classification {
        OutcomeChangeKind::MorePermissive => "more_permissive",
        OutcomeChangeKind::MoreRestrictive => "more_restrictive",
        OutcomeChangeKind::IntroducesEscalation => "introduces_escalation",
        OutcomeChangeKind::IntroducesInformationRequest => "introduces_information_request",
        OutcomeChangeKind::PrecedenceOnly => "precedence_only",
        OutcomeChangeKind::ReasonOnly => "reason_only",
        OutcomeChangeKind::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use std::sync::Arc;

    use rulery_contracts::{
        FactPath, LanguageVersion, Outcome, PackageId, PolicyDate, PolicyTimeZone, Reason,
        ReasonCode, Reasons, RuleId, SourceFile, SourceId, SourceKey, SourceMap, SourcePath, Span,
        Version,
    };
    use rulery_engine::JiffTimeZoneDatabase;
    use rulery_ir::{
        CompilationInput, CompiledDecision, CompiledEffect, CompiledPackageDraft, CompiledRule,
        DecisionSemantics, ExpiryPolicy, InvalidFactStrategy, MissingFactStrategy, Operator,
        PackageIntegritySet, Predicate,
    };
    use rulery_vocabulary::{EnumVariant, FieldDeclaration, FieldPresence, ResolvedRoot};

    use crate::FiniteDomain;

    use super::*;

    #[test]
    fn partition_specs_follow_vocabulary_and_authored_boundaries() {
        let (source_map, span) = sample_source_map();
        let package = package(&[domains_decision(span)], source_map);
        let specs = derive_partition_specs(&package, decision_of(&package, DOMAINS_DECISION));
        assert_eq!(
            specs
                .iter()
                .map(|spec| (spec.path.to_string(), &spec.kind, spec.allow_malformed))
                .collect::<Vec<_>>(),
            vec![
                (
                    "member.age".to_owned(),
                    &PartitionDomainKind::Integer(vec![18]),
                    false
                ),
                (
                    "member.bounded".to_owned(),
                    &PartitionDomainKind::List,
                    true
                ),
                (
                    "member.label".to_owned(),
                    &PartitionDomainKind::Text {
                        values: vec!["blue".to_owned()],
                        usage: ValueUsage::UnrepresentableValues,
                    },
                    false
                ),
                (
                    "member.member".to_owned(),
                    &PartitionDomainKind::Record,
                    false
                ),
                (
                    "member.promoted".to_owned(),
                    &PartitionDomainKind::Enum(vec![enum_value("gold"), enum_value("silver")]),
                    false
                ),
                (
                    "member.promoted-at".to_owned(),
                    &PartitionDomainKind::Unsupported,
                    false
                ),
                (
                    "member.undeclared".to_owned(),
                    &PartitionDomainKind::Unbounded,
                    false
                ),
            ]
        );
        for spec in &specs {
            assert!(spec.allow_absent, "{}", spec.path);
            // Every path may be absent. A path may be null only when its domain can hold a
            // null; a record's absence is expressed by the absent assignment alone.
            let is_record = matches!(spec.kind, PartitionDomainKind::Record);
            assert_eq!(spec.allow_null, !is_record, "{}", spec.path);
            assert!(spec.forbidden.is_empty(), "{}", spec.path);
        }
        assert_eq!(analysis_instant().as_nanoseconds(), 0);
    }

    #[test]
    fn derived_evaluations_are_witnessed_and_categorized() {
        let (source_map, span) = sample_source_map();
        let package = package(&[analysis_decision(span)], source_map);
        let database = JiffTimeZoneDatabase::new("test/fixed");
        let analyzer = PackageAnalyzer::new(&database, analysis_instant());
        let context = AnalysisContext::new(&database, analysis_instant());
        let options = tag_domain();
        let mut budget = AnalysisBudget::new(options.max_states);
        let mut witnesses = options.max_witnesses;

        let input = DecisionAnalysisInput::derive(
            &package,
            decision_of(&package, ANALYSIS_DECISION),
            &analyzer.evaluator,
            &context,
            &options,
            &mut budget,
            &mut witnesses,
        );
        assert_eq!(input.decision.as_str(), ANALYSIS_DECISION);
        assert_eq!(input.partition.completeness, AnalysisCompleteness::Complete);
        assert_eq!(input.evaluations.len(), 3);
        assert!(
            input
                .evaluations
                .iter()
                .all(|evaluation| evaluation.satisfiable)
        );
        assert!(
            input
                .evaluations
                .iter()
                .all(|evaluation| evaluation.witness.timezone_database
                    == TimeZoneDatabaseIdentity::new("test", "fixed").expect("identity"))
        );
        assert!(
            input
                .rules
                .iter()
                .all(|rule| rule.satisfiable == Some(true))
        );
        assert!(!input.witnesses.is_empty());
        assert!(input.witnesses.iter().all(|witness| witness.synthetic));
        assert!(budget.examined() > 0);

        let payload = analyzer.analyze(&package, &options).payload().clone();
        assert_eq!(payload.package_hash, package.payload().package_hash());
        assert_eq!(payload.completeness, AnalysisCompleteness::Complete);
        assert_eq!(payload.coverage.len(), 1);
        assert_eq!(payload.coverage[0].report.denominator, 3);
        assert_eq!(payload.coverage[0].report.numerator, 2);
        assert_eq!(payload.coverage[0].report.percent_basis_points, Some(6_666));
        assert_eq!(
            payload.coverage[0]
                .report
                .uncovered
                .iter()
                .map(|case| case.category)
                .collect::<Vec<_>>(),
            vec![UncoveredCategory::DefaultOnly]
        );
        assert_eq!(
            payload.coverage[0].report.diagnostics,
            BTreeSet::from(["RUL250".to_owned()])
        );
        assert_eq!(payload.reachability.len(), 2);
        assert!(
            payload
                .reachability
                .iter()
                .all(|entry| entry.status == crate::ReachabilityStatus::Reachable)
        );
        assert!(!payload.witnesses.is_empty());
    }

    #[test]
    fn an_exhausted_state_budget_reports_inconclusive_without_claims() {
        let (source_map, span) = sample_source_map();
        let package = package(&[analysis_decision(span)], source_map);
        let database = JiffTimeZoneDatabase::new("test/fixed");
        let analyzer = PackageAnalyzer::new(&database, analysis_instant());
        let options = AnalysisOptions {
            max_states: 0,
            max_witnesses: 0,
            ..tag_domain()
        };

        let payload = analyzer.analyze(&package, &options).payload().clone();
        assert_eq!(
            payload.completeness,
            AnalysisCompleteness::Inconclusive {
                examined: 0,
                limit: 0
            }
        );
        assert_eq!(payload.coverage.len(), 1);
        assert_eq!(payload.coverage[0].report.denominator, 0);
        assert!(payload.coverage[0].report.uncovered.is_empty());
        assert!(
            payload
                .reachability
                .iter()
                .all(|entry| entry.status == crate::ReachabilityStatus::Inconclusive)
        );
        assert!(payload.overlaps.is_empty());
        assert!(payload.witnesses.is_empty());
    }

    /// Supplies the finite domain a list path cannot infer, which the specification designates as
    /// the way to make an unsupported or presence-only path complete.
    fn tag_domain() -> AnalysisOptions {
        let mut options = AnalysisOptions::default();
        options.explicit_domains.insert(
            FactPath::from_str("member.tags").expect("path"),
            FiniteDomain::new(vec![FactPartitionValue::Valid(Value::List(vec![
                Value::Text("kept".to_owned()),
            ]))]),
        );
        options
    }

    const ANALYSIS_DECISION: &str = "decision.analysis";
    const DOMAINS_DECISION: &str = "decision.domains";

    fn sample_source_map() -> (SourceMap, Span) {
        let mut map = SourceMap::new();
        map.insert(
            SourceKey::new(1),
            SourceFile::new(
                SourceId::new("source.main").expect("source"),
                SourcePath::new("rules/analysis.yaml").expect("path"),
                Arc::<str>::from("analysis rules"),
            ),
        )
        .expect("source map");
        let span = map.span(SourceKey::new(1), 0, 8).expect("span");
        (map, span)
    }

    fn package(decisions: &[CompiledDecision], source_map: SourceMap) -> CompiledPackage {
        CompiledPackage::new(
            CompiledPackageDraft {
                package_id: PackageId::new("pkg.main").expect("package"),
                package_version: Version::new("1.0.0").expect("version"),
                language_version: LanguageVersion::V1,
                compiler_identity: "compiler".to_owned(),
                decisions: decisions.to_vec(),
                actions: Vec::new(),
                integrity: PackageIntegritySet::new(CompilationInput::new(
                    ContentHash::from_bytes([1; 32]),
                    ContentHash::from_bytes([2; 32]),
                    None,
                )),
            },
            source_map,
            vocabulary(),
        )
        .expect("package")
    }

    fn decision_of<'a>(package: &'a CompiledPackage, id: &str) -> &'a CompiledDecision {
        let id = DecisionId::new(id).expect("decision");
        package
            .payload()
            .decisions()
            .get(&id)
            .expect("compiled decision")
    }

    fn analysis_decision(span: Span) -> CompiledDecision {
        decision(
            ANALYSIS_DECISION,
            vec![
                rule("rule.tagged", tagged(span), approve("tagged"), span),
                rule("rule.untagged", untagged(span), deny("untagged"), span),
            ],
            span,
        )
    }

    fn domains_decision(span: Span) -> CompiledDecision {
        decision(
            DOMAINS_DECISION,
            vec![
                rule("rule.age", age_at_least(18, span), deny("too-young"), span),
                rule(
                    "rule.every-shape",
                    every_shape(span),
                    approve("shaped"),
                    span,
                ),
            ],
            span,
        )
    }

    fn decision(id: &str, rules: Vec<CompiledRule>, span: Span) -> CompiledDecision {
        CompiledDecision::new(
            DecisionId::new(id).expect("decision"),
            DecisionSemantics::new(
                MissingFactStrategy::PreserveUnknown,
                InvalidFactStrategy::PreserveInvalid,
                DecisionPrecedence::PriorityFirst,
                PolicyTimeZone::new("UTC").expect("timezone"),
                ExpiryPolicy::Inclusive,
            )
            .expect("semantics"),
            deny("default-deny"),
            rules,
            span,
        )
        .expect("decision")
    }

    fn rule(id: &str, condition: Expr, effect: CompiledEffect, span: Span) -> CompiledRule {
        let rule = RuleId::new(id).expect("rule");
        CompiledRule::new(
            rule.clone(),
            QualifiedRuleId::new(PackageId::new("pkg.main").expect("package"), rule),
            None,
            50,
            condition,
            effect,
            None,
            span,
            1,
        )
    }

    fn age_at_least(boundary: i64, span: Span) -> Expr {
        predicate(
            Operator::GreaterOrEqual,
            "member.age",
            Some(Value::Integer(boundary)),
            span,
        )
    }

    fn tagged(span: Span) -> Expr {
        predicate(
            Operator::Contains,
            "member.tags",
            Some(Value::Text("kept".to_owned())),
            span,
        )
    }

    fn untagged(span: Span) -> Expr {
        predicate(Operator::Missing, "member.tags", None, span)
    }

    fn every_shape(span: Span) -> Expr {
        Expr::All {
            expressions: vec![
                predicate(
                    Operator::StartsWith,
                    "member.label",
                    Some(Value::Text("blue".to_owned())),
                    span,
                ),
                predicate(
                    Operator::Equals,
                    "member.promoted",
                    Some(enum_value("gold")),
                    span,
                ),
                predicate(Operator::Missing, "member.label", None, span),
                predicate(Operator::Exists, "member.member", None, span),
                predicate(
                    Operator::LessThan,
                    "member.promoted-at",
                    Some(Value::Date(PolicyDate::parse("2026-01-01").expect("date"))),
                    span,
                ),
                predicate(Operator::IsTrue, "member.undeclared", None, span),
                predicate(
                    Operator::Contains,
                    "member.bounded",
                    Some(Value::Text("kept".to_owned())),
                    span,
                ),
            ],
            span,
        }
    }

    fn predicate(operator: Operator, path: &str, right: Option<Value>, span: Span) -> Expr {
        Expr::Predicate(Predicate {
            operator,
            left: ExprOperand::Fact(FactPath::from_str(path).expect("path")),
            right: right.map(ExprOperand::Literal),
            span,
        })
    }

    fn enum_value(variant: &str) -> Value {
        Value::Enum(EnumValue::new(
            TypeId::new("type.tier").expect("type"),
            StableId::new(variant).expect("variant"),
        ))
    }

    fn approve(code: &str) -> CompiledEffect {
        CompiledEffect::new(Outcome::approve(reasons(code), Vec::new()))
    }

    fn deny(code: &str) -> CompiledEffect {
        CompiledEffect::new(Outcome::deny(reasons(code), Vec::new()))
    }

    fn reasons(code: &str) -> Reasons {
        Reasons::new(vec![
            Reason::new(ReasonCode::new(code).expect("code"), code).expect("reason"),
        ])
        .expect("reasons")
    }

    #[allow(clippy::too_many_lines)]
    fn vocabulary() -> ResolvedVocabulary {
        let member = TypeId::new("type.member").expect("type");
        let tier = TypeId::new("type.tier").expect("type");
        let text = TypeId::new("text").expect("type");
        let bounded = TypeId::new("type.bounded").expect("type");
        let tags = TypeId::new("type.tags").expect("type");
        let nested = TypeId::new("type.nested").expect("type");
        let field = |name: &str, type_id: TypeId| {
            (
                StableId::new(name).expect("field"),
                FieldDeclaration {
                    type_id,
                    presence: FieldPresence::Optional,
                    derived: false,
                },
            )
        };
        ResolvedVocabulary {
            roots: BTreeMap::from([(
                FactPath::from_str("member").expect("path"),
                ResolvedRoot {
                    path: FactPath::from_str("member").expect("path"),
                    type_id: member.clone(),
                },
            )]),
            types: BTreeMap::from([
                (
                    member.clone(),
                    ResolvedType {
                        id: member.clone(),
                        declaration: TypeDeclaration::Record {
                            id: member,
                            fields: BTreeMap::from([
                                field("member", nested.clone()),
                                field("age", TypeId::new("int").expect("type")),
                                field("label", text.clone()),
                                field("promoted", tier.clone()),
                                field("promoted-at", TypeId::new("date").expect("type")),
                                field("undeclared", TypeId::new("int").expect("type")),
                                field("bounded", bounded.clone()),
                                field("tags", tags.clone()),
                            ]),
                            closed: true,
                        },
                    },
                ),
                (
                    nested.clone(),
                    ResolvedType {
                        id: nested.clone(),
                        declaration: TypeDeclaration::Record {
                            id: nested,
                            fields: BTreeMap::from([field("label", text.clone())]),
                            closed: true,
                        },
                    },
                ),
                (
                    tier.clone(),
                    ResolvedType {
                        id: tier.clone(),
                        declaration: TypeDeclaration::Enum {
                            id: tier,
                            variants: vec![
                                EnumVariant {
                                    symbol: StableId::new("gold").expect("variant"),
                                },
                                EnumVariant {
                                    symbol: StableId::new("silver").expect("variant"),
                                },
                            ],
                        },
                    },
                ),
                (
                    text.clone(),
                    ResolvedType {
                        id: text.clone(),
                        declaration: TypeDeclaration::Primitive,
                    },
                ),
                (
                    bounded.clone(),
                    ResolvedType {
                        id: bounded.clone(),
                        declaration: TypeDeclaration::List {
                            id: bounded,
                            element: text.clone(),
                            min_items: Some(1),
                            max_items: Some(2),
                        },
                    },
                ),
                (
                    tags.clone(),
                    ResolvedType {
                        id: tags.clone(),
                        declaration: TypeDeclaration::List {
                            id: tags,
                            element: text,
                            min_items: None,
                            max_items: None,
                        },
                    },
                ),
            ]),
            terms: BTreeMap::new(),
        }
    }

    #[test]
    fn record_typed_paths_are_never_assigned_null() {
        let vocabulary = vocabulary();
        let record = FactPath::from_str("member.member").expect("path");
        let child = FactPath::from_str("member.member.label").expect("path");

        let record_spec = spec_for(record.clone(), &Boundaries::default(), &vocabulary);
        assert_eq!(record_spec.kind, PartitionDomainKind::Record);
        assert!(
            !record_spec.allow_null,
            "a record is never null; the vocabulary expresses absence as optional presence, and a \
             null record combined with a valid child path cannot be represented as case facts"
        );

        let child_spec = spec_for(child, &Boundaries::default(), &vocabulary);
        assert!(matches!(child_spec.kind, PartitionDomainKind::Text { .. }));
        assert!(
            child_spec.allow_null,
            "a scalar field may still be explicitly null"
        );
    }
}
