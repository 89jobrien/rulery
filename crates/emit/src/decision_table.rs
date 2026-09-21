//! Decision-table lowering with semantic-loss detection.

use std::collections::BTreeMap;

use rulery_contracts::{
    ContentHash, DecisionId, OutcomeKind, QualifiedRuleId, RuleId, Span, StableId, Value,
};
use rulery_engine::Truth;
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
