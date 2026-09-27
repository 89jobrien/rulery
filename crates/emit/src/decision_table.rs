//! Decision-table lowering with semantic-loss detection.

use std::collections::{BTreeMap, BTreeSet};

use rulery_contracts::{
    ContentHash, DecisionId, FactPath, OutcomeKind, OutcomeTemplate, QualifiedRuleId, RuleId, Span,
    StableId, Value,
};
use rulery_engine::Truth;
use rulery_ir::{
    CompiledPackage, Expr, ExprOperand, Operator, canonical_condition, referenced_fact_paths,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Decision-table column role.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColumnRole {
    /// Condition column.
    Condition,
    /// Outcome column.
    Outcome,
    /// Reason column.
    Reason,
    /// Action column.
    Action,
    /// Priority column.
    Priority,
}

/// Decision-table column.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionColumn {
    /// Stable column identity.
    pub id: StableId,
    /// Display label.
    pub label: String,
    /// Semantic role.
    pub role: ColumnRole,
}

/// Lossless decision cell.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DecisionCell {
    /// Unconstrained cell.
    Any,
    /// Equality cell.
    Equal {
        /// Compared value.
        value: Value,
    },
    /// Inequality cell.
    NotEqual {
        /// Compared value.
        value: Value,
    },
    /// Range cell.
    Range {
        /// Optional minimum.
        minimum: Option<Value>,
        /// Optional maximum.
        maximum: Option<Value>,
        /// Minimum inclusion.
        inclusive_minimum: bool,
        /// Maximum inclusion.
        inclusive_maximum: bool,
    },
    /// Present evidence.
    Present,
    /// Absent evidence.
    Absent,
    /// Exact derived expression.
    Derived {
        /// Exact derived expression.
        expression: String,
    },
}

/// Decision-table row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionRow {
    /// Qualified rule identity.
    pub rule: QualifiedRuleId,
    /// Cells matching column order.
    pub cells: Vec<DecisionCell>,
    /// Exact outcome kind.
    pub outcome: OutcomeKind,
}

/// Strict decision table payload.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionTableV1 {
    /// Compiled package hash.
    pub package_hash: ContentHash,
    /// Single projected decision.
    pub decision: DecisionId,
    /// Ordered columns.
    pub columns: Vec<DecisionColumn>,
    /// Ordered rows.
    pub rows: Vec<DecisionRow>,
    /// Rule source references.
    pub source_references: BTreeMap<RuleId, Vec<Span>>,
}

/// Validated decision table.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecisionTable {
    payload: DecisionTableV1,
}

impl DecisionTable {
    /// Returns the validated payload.
    #[must_use]
    pub const fn payload(&self) -> &DecisionTableV1 {
        &self.payload
    }
}

/// One decision prepared for table lowering.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecisionProjection {
    /// Decision identity.
    pub decision: DecisionId,
    /// Columns.
    pub columns: Vec<DecisionColumn>,
    /// Rules.
    pub rules: Vec<ProjectionRule>,
}

impl DecisionProjection {
    /// Projects one compiled decision into table columns and rows.
    ///
    /// Columns are the fact paths the decision's rules read, in ascending order, and every rule
    /// carries exactly one cell per column. A path no rule constrains is unconstrained, a single
    /// comparison becomes its matching cell shape, and anything a single cell cannot state exactly
    /// becomes a [`DecisionCell::Derived`] cell holding the canonical condition text. Truth states
    /// are the union of the states each condition subtree can produce, so a target without
    /// uncertainty support is reported as lossy by [`validate_projection`].
    ///
    /// Returns [`None`] when the package does not declare the decision.
    #[must_use]
    pub fn from_decision(package: &CompiledPackage, decision: &DecisionId) -> Option<Self> {
        let compiled = package.payload().decisions().get(decision)?;
        let paths = compiled
            .rules()
            .values()
            .flat_map(|rule| referenced_fact_paths(rule.condition()))
            .collect::<BTreeSet<_>>();
        let mut columns = Vec::with_capacity(paths.len());
        for path in &paths {
            columns.push(DecisionColumn {
                id: StableId::new(path.to_string()).ok()?,
                label: path.to_string(),
                role: ColumnRole::Condition,
            });
        }
        let rules = compiled
            .rules()
            .values()
            .map(|rule| ProjectionRule {
                rule: rule.qualified_id().clone(),
                cells: paths
                    .iter()
                    .map(|path| decision_cell(rule.condition(), path))
                    .collect(),
                outcome: rule.outcome().kind(),
                span: rule.span(),
                truth_states: truth_states(rule.condition()),
                has_actions: has_actions(rule.outcome()),
                has_reasons: has_reasons(rule.outcome()),
            })
            .collect();
        Some(Self {
            decision: decision.clone(),
            columns,
            rules,
        })
    }
}

fn has_actions(outcome: &OutcomeTemplate) -> bool {
    match outcome {
        OutcomeTemplate::Approve { actions, .. } | OutcomeTemplate::Deny { actions, .. } => {
            !actions.is_empty()
        }
        OutcomeTemplate::Escalate { .. } | OutcomeTemplate::RequestInformation { .. } => false,
    }
}

fn has_reasons(outcome: &OutcomeTemplate) -> bool {
    match outcome {
        OutcomeTemplate::Approve { reasons, .. }
        | OutcomeTemplate::Deny { reasons, .. }
        | OutcomeTemplate::Escalate { reasons, .. }
        | OutcomeTemplate::RequestInformation { reasons, .. } => reasons.iter().next().is_some(),
    }
}

/// Projects the top-level conjuncts of a condition onto one fact path.
fn decision_cell(condition: &Expr, path: &FactPath) -> DecisionCell {
    let mut conjuncts = Vec::new();
    for child in top_level_conjuncts(condition) {
        if references_path(child, path) {
            conjuncts.push(child);
        }
    }
    match conjuncts.as_slice() {
        [] => DecisionCell::Any,
        [single] => predicate_cell(single, path),
        _ => DecisionCell::Derived {
            expression: conjuncts
                .iter()
                .map(|child| canonical_condition(child))
                .collect::<Vec<_>>()
                .join(" and "),
        },
    }
}

fn top_level_conjuncts(expression: &Expr) -> Vec<&Expr> {
    match expression {
        Expr::All { expressions, .. } => expressions.iter().collect(),
        other => vec![other],
    }
}

fn references_path(expression: &Expr, path: &FactPath) -> bool {
    referenced_fact_paths(expression).contains(path)
}

fn predicate_cell(expression: &Expr, path: &FactPath) -> DecisionCell {
    let Expr::Predicate(predicate) = expression else {
        return DecisionCell::Derived {
            expression: canonical_condition(expression),
        };
    };
    if !matches!(&predicate.left, ExprOperand::Fact(fact) if fact == path) {
        return DecisionCell::Derived {
            expression: canonical_condition(expression),
        };
    }
    let literal = match &predicate.right {
        Some(ExprOperand::Literal(value)) => value.clone(),
        _ => {
            return match predicate.operator {
                Operator::Exists => DecisionCell::Present,
                Operator::Missing => DecisionCell::Absent,
                _ => DecisionCell::Derived {
                    expression: canonical_condition(expression),
                },
            };
        }
    };
    match predicate.operator {
        Operator::Equals => DecisionCell::Equal { value: literal },
        Operator::NotEquals => DecisionCell::NotEqual { value: literal },
        Operator::LessThan | Operator::Before => DecisionCell::Range {
            minimum: None,
            maximum: Some(literal),
            inclusive_minimum: false,
            inclusive_maximum: false,
        },
        Operator::LessOrEqual | Operator::OnOrBefore => DecisionCell::Range {
            minimum: None,
            maximum: Some(literal),
            inclusive_minimum: false,
            inclusive_maximum: true,
        },
        Operator::GreaterThan | Operator::After => DecisionCell::Range {
            minimum: Some(literal),
            maximum: None,
            inclusive_minimum: false,
            inclusive_maximum: false,
        },
        Operator::GreaterOrEqual | Operator::OnOrAfter => DecisionCell::Range {
            minimum: Some(literal),
            maximum: None,
            inclusive_minimum: true,
            inclusive_maximum: false,
        },
        _ => DecisionCell::Derived {
            expression: canonical_condition(expression),
        },
    }
}

/// Truth states a condition subtree can produce.
///
/// A constant produces only its own state, and a presence predicate never produces `Unknown` or
/// `Invalid`. Every other predicate can read an absent fact, which is `Unknown`, or malformed
/// evidence, which is `Invalid`, so all four states are representable.
fn truth_states(expression: &Expr) -> Vec<Truth> {
    let mut states = Vec::new();
    collect_truth_states(expression, &mut states);
    let mut ordered = Vec::with_capacity(states.len());
    for state in [Truth::False, Truth::True, Truth::Unknown, Truth::Invalid] {
        if states.contains(&state) {
            ordered.push(state);
        }
    }
    ordered
}

fn collect_truth_states(expression: &Expr, states: &mut Vec<Truth>) {
    let mut insert = |state: Truth| {
        if !states.contains(&state) {
            states.push(state);
        }
    };
    match expression {
        Expr::Constant { value, .. } => insert(if *value { Truth::True } else { Truth::False }),
        Expr::Predicate(predicate) => {
            if matches!(predicate.operator, Operator::Exists | Operator::Missing) {
                insert(Truth::False);
                insert(Truth::True);
            } else {
                for state in [Truth::False, Truth::True, Truth::Unknown, Truth::Invalid] {
                    insert(state);
                }
            }
        }
        Expr::All { expressions, .. } | Expr::Any { expressions, .. } => {
            for child in expressions {
                collect_truth_states(child, states);
            }
        }
        Expr::Not { expression, .. } => collect_truth_states(expression, states),
    }
}

/// Rule projection with semantic feature inventory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectionRule {
    /// Rule identity.
    pub rule: QualifiedRuleId,
    /// Table cells.
    pub cells: Vec<DecisionCell>,
    /// Exact outcome.
    pub outcome: OutcomeKind,
    /// Authored source span.
    pub span: Span,
    /// Truth states requiring representation.
    pub truth_states: Vec<Truth>,
    /// Whether actions are present.
    pub has_actions: bool,
    /// Whether reasons are present.
    pub has_reasons: bool,
}

/// Target semantic capabilities.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(clippy::struct_excessive_bools)]
pub struct ProjectionCapabilities {
    /// Supports escalation.
    pub escalation: bool,
    /// Supports information requests.
    pub information_request: bool,
    /// Supports Unknown and Invalid.
    pub uncertainty: bool,
    /// Supports actions.
    pub actions: bool,
    /// Supports reasons.
    pub reasons: bool,
}

/// Feature that cannot be represented.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ProjectionFeature {
    /// Escalation outcome.
    Escalation,
    /// Information-request outcome.
    InformationRequest,
    /// Unknown truth.
    Unknown,
    /// Invalid truth.
    Invalid,
    /// Declarative action.
    Action,
    /// Outcome reason.
    Reason,
}

/// Semantic-loss finding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectionLoss {
    /// `RUL400` or `RUL401`.
    pub code: &'static str,
    /// Unsupported feature.
    pub feature: ProjectionFeature,
    /// Rule evidence.
    pub rule: QualifiedRuleId,
}

/// Decision-table construction error.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum DecisionTableError {
    /// Exactly one decision is required.
    #[error("decision-table export requires exactly one decision")]
    DecisionCount,
}

/// Lowers exactly one decision to a strict table.
///
/// # Errors
///
/// Returns [`DecisionTableError`] unless exactly one decision is supplied.
pub fn build_decision_table(
    package_hash: ContentHash,
    mut decisions: Vec<DecisionProjection>,
) -> Result<DecisionTable, DecisionTableError> {
    if decisions.len() != 1 {
        return Err(DecisionTableError::DecisionCount);
    }
    let projection = decisions.remove(0);
    let mut columns = projection.columns;
    columns.sort_by(|left, right| left.id.cmp(&right.id));
    let mut rules = projection.rules;
    rules.sort_by(|left, right| left.rule.cmp(&right.rule));
    let rows = rules
        .iter()
        .map(|rule| DecisionRow {
            rule: rule.rule.clone(),
            cells: rule.cells.clone(),
            outcome: rule.outcome,
        })
        .collect();
    let mut source_references = BTreeMap::new();
    for rule in rules {
        source_references
            .entry(rule.rule.rule().clone())
            .or_insert_with(Vec::new)
            .push(rule.span);
    }
    Ok(DecisionTable {
        payload: DecisionTableV1 {
            package_hash,
            decision: projection.decision,
            columns,
            rows,
            source_references,
        },
    })
}

/// Reports every feature a target cannot preserve without reinterpretation.
#[must_use]
pub fn validate_projection(
    rules: &[ProjectionRule],
    capabilities: ProjectionCapabilities,
) -> Vec<ProjectionLoss> {
    let mut losses = Vec::new();
    for rule in rules {
        if rule.outcome == OutcomeKind::Escalate && !capabilities.escalation {
            losses.push(loss(rule, "RUL401", ProjectionFeature::Escalation));
        }
        if rule.outcome == OutcomeKind::RequestInformation && !capabilities.information_request {
            losses.push(loss(rule, "RUL401", ProjectionFeature::InformationRequest));
        }
        if !capabilities.uncertainty && rule.truth_states.contains(&Truth::Unknown) {
            losses.push(loss(rule, "RUL400", ProjectionFeature::Unknown));
        }
        if !capabilities.uncertainty && rule.truth_states.contains(&Truth::Invalid) {
            losses.push(loss(rule, "RUL400", ProjectionFeature::Invalid));
        }
        if rule.has_actions && !capabilities.actions {
            losses.push(loss(rule, "RUL400", ProjectionFeature::Action));
        }
        if rule.has_reasons && !capabilities.reasons {
            losses.push(loss(rule, "RUL400", ProjectionFeature::Reason));
        }
    }
    losses
}

fn loss(rule: &ProjectionRule, code: &'static str, feature: ProjectionFeature) -> ProjectionLoss {
    ProjectionLoss {
        code,
        feature,
        rule: rule.rule.clone(),
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use std::sync::Arc;

    use rulery_contracts::{
        ContentHash, FactPath, LanguageVersion, Outcome, PackageId, PolicyTimeZone,
        QualifiedRuleId, Reason, ReasonCode, Reasons, RuleId, SourceFile, SourceId, SourceKey,
        SourceMap, SourcePath, Span, StableId, TypeId, Value, Version,
    };
    use rulery_ir::{
        CompilationInput, CompiledDecision, CompiledEffect, CompiledPackage, CompiledPackageDraft,
        CompiledRule, DecisionPrecedence, DecisionSemantics, ExpiryPolicy, FieldDeclaration,
        FieldPresence, InvalidFactStrategy, MissingFactStrategy, Operator, PackageIntegritySet,
        Predicate, ResolvedRoot, ResolvedType, ResolvedVocabulary, TypeDeclaration,
    };

    use super::*;

    const DECISION: &str = "decision.access";

    #[test]
    fn projections_order_columns_and_lower_each_comparison_shape() {
        let (source_map, span) = sample_source_map();
        let package = package(&[access_decision(span)], source_map);
        let decision = DecisionId::new(DECISION).expect("decision");
        let projection =
            DecisionProjection::from_decision(&package, &decision).expect("projection");

        assert_eq!(
            projection
                .columns
                .iter()
                .map(|column| (column.label.as_str(), column.role))
                .collect::<Vec<_>>(),
            vec![
                ("member.age", ColumnRole::Condition),
                ("member.label", ColumnRole::Condition),
                ("member.tags", ColumnRole::Condition),
            ]
        );
        assert_eq!(
            projection
                .rules
                .iter()
                .map(|entry| (entry.rule.to_string(), entry.cells.clone()))
                .collect::<Vec<_>>(),
            vec![
                (
                    "pkg.main::rule.senior".to_owned(),
                    vec![
                        DecisionCell::Range {
                            minimum: Some(Value::Integer(65)),
                            maximum: None,
                            inclusive_minimum: true,
                            inclusive_maximum: false,
                        },
                        DecisionCell::Any,
                        DecisionCell::Any,
                    ]
                ),
                (
                    "pkg.main::rule.tagged".to_owned(),
                    vec![
                        DecisionCell::Any,
                        DecisionCell::Equal {
                            value: Value::Text("blue".to_owned())
                        },
                        DecisionCell::Absent,
                    ]
                ),
            ]
        );
        assert_eq!(
            projection
                .rules
                .iter()
                .map(|entry| entry.truth_states.clone())
                .collect::<Vec<_>>(),
            vec![
                vec![Truth::False, Truth::True, Truth::Unknown, Truth::Invalid],
                vec![Truth::False, Truth::True, Truth::Unknown, Truth::Invalid],
            ]
        );
        assert!(projection.rules.iter().all(|entry| entry.has_reasons));
        assert!(!projection.rules.iter().any(|entry| entry.has_actions));
        assert!(
            DecisionProjection::from_decision(
                &package,
                &DecisionId::new("decision.absent").expect("id")
            )
            .is_none()
        );
    }

    fn sample_source_map() -> (SourceMap, Span) {
        let mut map = SourceMap::new();
        map.insert(
            SourceKey::new(1),
            SourceFile::new(
                SourceId::new("source.main").expect("source"),
                SourcePath::new("rules/access.yaml").expect("path"),
                Arc::<str>::from("access rules"),
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

    fn access_decision(span: Span) -> CompiledDecision {
        let rules = vec![
            rule(
                "rule.senior",
                predicate(
                    Operator::GreaterOrEqual,
                    "member.age",
                    Some(Value::Integer(65)),
                    span,
                ),
                Outcome::approve(reasons("senior"), Vec::new()),
                span,
            ),
            rule(
                "rule.tagged",
                Expr::All {
                    expressions: vec![
                        predicate(Operator::Missing, "member.tags", None, span),
                        predicate(
                            Operator::Equals,
                            "member.label",
                            Some(Value::Text("blue".to_owned())),
                            span,
                        ),
                    ],
                    span,
                },
                Outcome::deny(reasons("tagged"), Vec::new()),
                span,
            ),
        ];
        CompiledDecision::new(
            DecisionId::new(DECISION).expect("decision"),
            DecisionSemantics::new(
                MissingFactStrategy::PreserveUnknown,
                InvalidFactStrategy::PreserveInvalid,
                DecisionPrecedence::PriorityFirst,
                PolicyTimeZone::new("UTC").expect("timezone"),
                ExpiryPolicy::Inclusive,
            )
            .expect("semantics"),
            CompiledEffect::new(Outcome::deny(reasons("default-deny"), Vec::new())),
            rules,
            span,
        )
        .expect("decision")
    }

    fn rule(id: &str, condition: Expr, outcome: Outcome, span: Span) -> CompiledRule {
        let rule = RuleId::new(id).expect("rule");
        CompiledRule::new(
            rule.clone(),
            QualifiedRuleId::new(PackageId::new("pkg.main").expect("package"), rule),
            None,
            50,
            condition,
            CompiledEffect::new(outcome),
            None,
            span,
            1,
        )
    }

    fn predicate(operator: Operator, path: &str, right: Option<Value>, span: Span) -> Expr {
        Expr::Predicate(Predicate {
            operator,
            left: ExprOperand::Fact(FactPath::from_str(path).expect("path")),
            right: right.map(ExprOperand::Literal),
            span,
        })
    }

    fn reasons(code: &str) -> Reasons {
        Reasons::new(vec![
            Reason::new(ReasonCode::new(code).expect("code"), code).expect("reason"),
        ])
        .expect("reasons")
    }

    fn vocabulary() -> ResolvedVocabulary {
        let member = TypeId::new("type.member").expect("type");
        let text = TypeId::new("text").expect("type");
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
                                field("age", TypeId::new("int").expect("type")),
                                field("label", text.clone()),
                                field("tags", text.clone()),
                            ]),
                            closed: true,
                        },
                    },
                ),
                (
                    text.clone(),
                    ResolvedType {
                        id: text,
                        declaration: TypeDeclaration::Primitive,
                    },
                ),
            ]),
            terms: BTreeMap::new(),
        }
    }
}
