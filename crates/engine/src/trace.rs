//! Strict deterministic decision trace contracts.

use std::collections::BTreeSet;

use rulery_contracts::{
    ContentHash, DecisionId, EvaluationId, FactPath, FactValidationError, HashDomain,
    LanguageVersion, Outcome, PackageId, PolicyTimeZone, QualifiedRuleId, Span,
    TimeZoneDatabaseIdentity, UtcInstant, Value, Version, hash_parts,
};
use rulery_ir::Operator;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{DecisionRelevance, SemanticPrecedenceKey, Truth};

/// Requested trace projection detail.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TraceDetail {
    /// Every logical expression and operand.
    Complete,
    /// Top-level rules and decision-critical detail.
    Compact,
}

/// Reason a descendant was not evaluated.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShortCircuitReason {
    /// False made an `all` decisive.
    DecisiveAnd,
    /// True made an `any` decisive.
    DecisiveOr,
}

/// Reserved clock operand name.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClockOperand {
    /// Policy-local date.
    Today,
    /// Current UTC instant.
    Now,
}

/// Operand value retained by a predicate trace.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EvaluatedOperand {
    /// Valid typed value.
    Value {
        /// Typed value.
        value: Value,
        /// Source fact path for runtime evidence.
        source_path: Option<FactPath>,
    },
    /// Missing runtime fact.
    Missing {
        /// Requested path.
        path: FactPath,
    },
    /// Malformed runtime evidence.
    Invalid {
        /// Runtime path when applicable.
        path: Option<FactPath>,
        /// Structural validation error.
        error: FactValidationError,
    },
    /// Reserved clock value.
    ClockValue {
        /// Exact date or UTC instant.
        value: Value,
        /// Reserved operand name.
        name: ClockOperand,
    },
}

/// Atomic predicate trace.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PredicateTrace {
    /// Predicate result.
    pub result: Truth,
    /// Checked operator.
    pub operator: Operator,
    /// Left operand.
    pub lhs: EvaluatedOperand,
    /// Optional right operand.
    pub rhs: Option<EvaluatedOperand>,
    /// Authored span.
    pub span: Span,
}

/// Recursive expression trace.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExpressionTrace {
    /// Logical conjunction.
    All {
        /// Result.
        result: Truth,
        /// Ordered child traces.
        children: Vec<ExpressionTrace>,
        /// Authored span.
        span: Span,
    },
    /// Logical disjunction.
    Any {
        /// Result.
        result: Truth,
        /// Ordered child traces.
        children: Vec<ExpressionTrace>,
        /// Authored span.
        span: Span,
    },
    /// Logical negation.
    Not {
        /// Result.
        result: Truth,
        /// Child trace.
        child: Box<ExpressionTrace>,
        /// Authored span.
        span: Span,
    },
    /// Atomic predicate.
    Predicate(PredicateTrace),
    /// Constant expression.
    Constant {
        /// Result.
        result: Truth,
        /// Constant value.
        value: bool,
        /// Authored span.
        span: Span,
    },
    /// Explicit short-circuit placeholder.
    NotEvaluated {
        /// Parent-decisive result.
        result: Truth,
        /// Short-circuit reason.
        reason: ShortCircuitReason,
        /// Authored span.
        span: Span,
    },
}

/// Candidate detail retained for selection evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateTrace {
    /// Instantiated outcome.
    pub outcome: Outcome,
    /// Exact semantic key.
    pub semantic_key: SemanticPrecedenceKey,
}

/// Complete top-level rule trace.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleTrace {
    /// Qualified rule identity.
    pub rule: QualifiedRuleId,
    /// Rule truth result.
    pub result: Truth,
    /// Whether this rule determined the outcome.
    pub selected: bool,
    /// Decision relevance.
    pub relevance: DecisionRelevance,
    /// Complete or projected expression trace.
    pub condition: ExpressionTrace,
    /// Instantiated candidate when decisive.
    pub candidate: Option<CandidateTrace>,
    /// Authored rule span.
    pub source_span: Span,
}

/// Why a rule was superseded.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupersessionReason {
    /// Candidate had a lower semantic key.
    LowerSemanticKey,
    /// Equal-key candidate instantiated the same outcome.
    EqualKeyIdenticalOutcome,
    /// Unknown rule could not displace the selected result.
    IrrelevantUnknown,
    /// Invalid rule could not displace the selected result.
    IrrelevantInvalid,
}

/// Superseded rule presentation record.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupersededRuleTrace {
    /// Rule identity.
    pub rule: QualifiedRuleId,
    /// Supersession reason.
    pub reason: SupersessionReason,
}

/// Invalid fact retained by a trace.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvalidFactTrace {
    /// Runtime path when available.
    pub path: Option<FactPath>,
    /// Validation error.
    pub error: FactValidationError,
    /// Source span when available.
    pub span: Option<Span>,
}

/// Strategy family applied during evaluation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategyKind {
    /// Missing-fact strategy.
    Missing,
    /// Invalid-fact strategy.
    Invalid,
}

/// Applied uncertainty strategy trace.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrategyApplicationTrace {
    /// Strategy family.
    pub strategy: StrategyKind,
    /// Sorted decision-relevant paths.
    pub relevant_paths: BTreeSet<FactPath>,
    /// Generated candidate when applicable.
    pub candidate: Option<CandidateTrace>,
}

/// Runtime conflict retained as a successful partial trace.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionConflictTrace {
    /// Greatest shared semantic key.
    pub key: SemanticPrecedenceKey,
    /// Tied rules in presentation order.
    pub rules: Vec<QualifiedRuleId>,
    /// Incompatible tied outcomes.
    pub outcomes: Vec<Outcome>,
}

/// Strict version-one decision trace payload.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionTraceV1 {
    /// Deterministic evaluation identity.
    pub evaluation_id: EvaluationId,
    /// Evaluated package.
    pub package: PackageId,
    /// Evaluated package version.
    pub package_version: Version,
    /// Evaluated decision.
    pub decision: DecisionId,
    /// UTC evaluation instant.
    pub evaluated_at: UtcInstant,
    /// Policy timezone.
    pub timezone: PolicyTimeZone,
    /// Timezone database identity.
    pub timezone_database: TimeZoneDatabaseIdentity,
    /// Compiled package hash.
    pub package_hash: ContentHash,
    /// Case facts hash.
    pub facts_hash: ContentHash,
    /// Compiler identity.
    pub compiler: String,
    /// Language version.
    pub language_version: LanguageVersion,
    /// Selected outcome; explicit JSON null when absent.
    pub outcome: Option<Outcome>,
    /// Determining rules.
    pub determining_rules: Vec<QualifiedRuleId>,
    /// Superseded rules.
    pub superseded_rules: Vec<SupersededRuleTrace>,
    /// Every top-level rule trace.
    pub rule_traces: Vec<RuleTrace>,
    /// Sorted missing facts.
    pub missing_facts: BTreeSet<FactPath>,
    /// Invalid fact details.
    pub invalid_facts: Vec<InvalidFactTrace>,
    /// Applied uncertainty strategies.
    pub strategy_applications: Vec<StrategyApplicationTrace>,
    /// Runtime conflict; explicit JSON null when absent.
    pub conflict: Option<DecisionConflictTrace>,
    /// Hash of the complete logical trace.
    pub trace_hash: ContentHash,
}

/// Trace fields supplied before validation and hashing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecisionTraceDraft {
    /// Payload with a placeholder `trace_hash`.
    pub payload: DecisionTraceV1,
    /// Whether an empty determining set represents an instantiated default.
    pub defaulted: bool,
}

/// Validated decision trace wrapper.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecisionTrace {
    payload: DecisionTraceV1,
}

impl DecisionTrace {
    /// Builds a validated trace and requested projection from complete logical data.
    ///
    /// # Errors
    ///
    /// Returns [`TraceError`] for invalid success/conflict combinations or serialization failure.
    pub fn build(mut draft: DecisionTraceDraft, detail: TraceDetail) -> Result<Self, TraceError> {
        canonicalize(&mut draft.payload)?;
        validate_state(&draft)?;
        draft.payload.evaluation_id = evaluation_id(
            draft.payload.package_hash,
            &draft.payload.decision,
            draft.payload.facts_hash,
            draft.payload.evaluated_at,
            &draft.payload.timezone_database,
        );
        draft.payload.trace_hash = trace_hash(&draft.payload)?;
        if detail == TraceDetail::Compact {
            for rule in &mut draft.payload.rule_traces {
                rule.condition = compact_expression(rule.condition.clone());
            }
        }
        Ok(Self {
            payload: draft.payload,
        })
    }

    /// Returns the strict v1 payload.
    #[must_use]
    pub const fn payload(&self) -> &DecisionTraceV1 {
        &self.payload
    }
}

/// Strict versioned decision trace envelope.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "schema", content = "payload", deny_unknown_fields)]
pub enum DecisionTraceEnvelope {
    /// Decision trace schema version 1.
    #[serde(rename = "rulery.decision-trace/v1")]
    V1(DecisionTraceV1),
}

/// Builds the exact five-part deterministic evaluation identity.
#[must_use]
pub fn evaluation_id(
    package_hash: ContentHash,
    decision: &DecisionId,
    facts_hash: ContentHash,
    evaluated_at: UtcInstant,
    timezone_database: &TimeZoneDatabaseIdentity,
) -> EvaluationId {
    let database = serde_json::to_vec(timezone_database).unwrap_or_default();
    let instant = evaluated_at.as_nanoseconds().to_be_bytes();
    hash_parts(
        HashDomain::EvaluationV1,
        [
            package_hash.as_bytes().as_slice(),
            decision.as_str().as_bytes(),
            facts_hash.as_bytes().as_slice(),
            instant.as_slice(),
            database.as_slice(),
        ],
    )
}

fn validate_state(draft: &DecisionTraceDraft) -> Result<(), TraceError> {
    let payload = &draft.payload;
    let success = payload.outcome.is_some()
        && payload.conflict.is_none()
        && (draft.defaulted || !payload.determining_rules.is_empty());
    let conflict = payload.outcome.is_none()
        && payload.conflict.is_some()
        && payload.determining_rules.is_empty();
    if success || conflict {
        Ok(())
    } else {
        Err(TraceError::InvalidOutcomeConflictState)
    }
}

fn canonicalize(payload: &mut DecisionTraceV1) -> Result<(), TraceError> {
    payload.determining_rules.sort();
    payload.determining_rules.dedup();
    payload
        .superseded_rules
        .sort_by(|left, right| left.rule.cmp(&right.rule));
    payload
        .superseded_rules
        .dedup_by(|left, right| left.rule == right.rule && left.reason == right.reason);
    payload
        .rule_traces
        .sort_by(|left, right| left.rule.cmp(&right.rule));
    payload.invalid_facts.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| format!("{:?}", left.error).cmp(&format!("{:?}", right.error)))
    });
    payload.strategy_applications.sort_by(|left, right| {
        strategy_order(left.strategy)
            .cmp(&strategy_order(right.strategy))
            .then_with(|| left.relevant_paths.cmp(&right.relevant_paths))
    });
    if let Some(conflict) = &mut payload.conflict {
        conflict.rules.sort();
        conflict.rules.dedup();
        let mut keyed = conflict
            .outcomes
            .drain(..)
            .map(|outcome| {
                serde_json::to_vec(&outcome)
                    .map(|key| (key, outcome))
                    .map_err(TraceError::Serialization)
            })
            .collect::<Result<Vec<_>, TraceError>>()?;
        keyed.sort_by(|left, right| left.0.cmp(&right.0));
        conflict.outcomes = keyed.into_iter().map(|(_, outcome)| outcome).collect();
    }
    Ok(())
}

const fn strategy_order(strategy: StrategyKind) -> u8 {
    match strategy {
        StrategyKind::Missing => 0,
        StrategyKind::Invalid => 1,
    }
}

fn trace_hash(payload: &DecisionTraceV1) -> Result<ContentHash, TraceError> {
    let mut value = serde_json::to_value(payload).map_err(TraceError::Serialization)?;
    value
        .as_object_mut()
        .ok_or(TraceError::InvalidOutcomeConflictState)?
        .remove("trace_hash");
    let bytes = serde_json::to_vec(&value).map_err(TraceError::Serialization)?;
    Ok(hash_parts(HashDomain::DecisionTraceV1, [bytes.as_slice()]))
}

fn compact_expression(expression: ExpressionTrace) -> ExpressionTrace {
    match expression {
        ExpressionTrace::All {
            result,
            children,
            span,
        } => ExpressionTrace::All {
            result,
            children: compact_children(children),
            span,
        },
        ExpressionTrace::Any {
            result,
            children,
            span,
        } => ExpressionTrace::Any {
            result,
            children: compact_children(children),
            span,
        },
        ExpressionTrace::Not {
            result,
            child,
            span,
        } => ExpressionTrace::Not {
            result,
            child: Box::new(compact_expression(*child)),
            span,
        },
        leaf => leaf,
    }
}

fn compact_children(children: Vec<ExpressionTrace>) -> Vec<ExpressionTrace> {
    children
        .into_iter()
        .filter(|child| !matches!(child, ExpressionTrace::NotEvaluated { .. }))
        .map(compact_expression)
        .collect()
}

/// Decision trace invariant failure.
#[derive(Debug, Error)]
pub enum TraceError {
    /// Success/conflict fields form an invalid combination.
    #[error("decision trace success/conflict fields are inconsistent")]
    InvalidOutcomeConflictState,
    /// Canonical trace serialization failed.
    #[error("failed to serialize decision trace for hashing: {0}")]
    Serialization(serde_json::Error),
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use std::sync::Arc;

    use rulery_contracts::{
        PackageId, Reason, ReasonCode, Reasons, RuleId, SourceFile, SourceId, SourceKey, SourceMap,
        SourcePath, TimeZoneDatabaseIdentity, TypeId,
    };

    use super::*;

    #[test]
    fn trace_modes_share_complete_logical_hash() {
        let complete =
            DecisionTrace::build(draft(false), TraceDetail::Complete).expect("complete trace");
        let compact =
            DecisionTrace::build(draft(false), TraceDetail::Compact).expect("compact trace");
        assert_eq!(complete.payload.trace_hash, compact.payload.trace_hash);
        assert_eq!(
            complete.payload.rule_traces.len(),
            compact.payload.rule_traces.len()
        );
        assert!(
            expression_count(&complete.payload.rule_traces[0].condition)
                > expression_count(&compact.payload.rule_traces[0].condition)
        );
        assert!(matches!(
            complete.payload.rule_traces[0].condition,
            ExpressionTrace::All { ref children, .. }
                if matches!(children[1], ExpressionTrace::NotEvaluated { .. })
        ));
        assert!(matches!(
            complete.payload.rule_traces[0].candidate,
            Some(CandidateTrace { .. })
        ));
        assert!(!complete.payload.missing_facts.is_empty());
        assert!(!complete.payload.invalid_facts.is_empty());
        assert!(!complete.payload.strategy_applications.is_empty());
        assert!(!complete.payload.superseded_rules.is_empty());
        assert_eq!(complete.payload.determining_rules.len(), 1);

        let encoded = serde_json::to_value(DecisionTraceEnvelope::V1(complete.payload.clone()))
            .expect("serialize envelope");
        assert!(encoded["payload"]["outcome"].is_object());
        assert!(encoded["payload"]["conflict"].is_null());

        let conflict = DecisionTrace::build(draft(true), TraceDetail::Complete)
            .expect("runtime conflict is a valid trace");
        assert!(conflict.payload.outcome.is_none());
        assert!(conflict.payload.conflict.is_some());
        assert!(conflict.payload.determining_rules.is_empty());
        let conflict_json =
            serde_json::to_value(DecisionTraceEnvelope::V1(conflict.payload.clone()))
                .expect("conflict envelope");
        assert!(conflict_json["payload"]["outcome"].is_null());
        assert!(conflict_json["payload"]["conflict"].is_object());

        let mut invalid = draft(false);
        invalid.payload.conflict = Some(conflict.payload.conflict.expect("conflict"));
        assert!(matches!(
            DecisionTrace::build(invalid, TraceDetail::Complete),
            Err(TraceError::InvalidOutcomeConflictState)
        ));
    }

    fn draft(conflict: bool) -> DecisionTraceDraft {
        let map = source_map();
        let span = map.span(SourceKey::new(1), 0, 1).expect("span");
        let rule = QualifiedRuleId::new(
            PackageId::new("pkg.main").expect("package"),
            RuleId::new("rule.main").expect("rule"),
        );
        let reasons = Reasons::new(vec![
            Reason::new(ReasonCode::new("approved").expect("code"), "approved").expect("reason"),
        ])
        .expect("reasons");
        let outcome = Outcome::approve(reasons, Vec::new());
        let key = SemanticPrecedenceKey::PriorityFirst(10, 1_000, 2, 0);
        let condition = ExpressionTrace::All {
            result: Truth::True,
            children: vec![
                ExpressionTrace::Predicate(PredicateTrace {
                    result: Truth::True,
                    operator: Operator::Exists,
                    lhs: EvaluatedOperand::Value {
                        value: Value::Text("active".to_owned()),
                        source_path: Some(FactPath::from_str("member.status").expect("fact path")),
                    },
                    rhs: Some(EvaluatedOperand::ClockValue {
                        value: Value::DateTime(UtcInstant::new(1).expect("instant")),
                        name: ClockOperand::Now,
                    }),
                    span,
                }),
                ExpressionTrace::NotEvaluated {
                    result: Truth::True,
                    reason: ShortCircuitReason::DecisiveOr,
                    span,
                },
            ],
            span,
        };
        let package_hash = ContentHash::from_bytes([1; 32]);
        let facts_hash = ContentHash::from_bytes([2; 32]);
        let evaluated_at = UtcInstant::new(1).expect("instant");
        let timezone_database = TimeZoneDatabaseIdentity::new("jiff", "2026a").expect("database");
        let decision = DecisionId::new("decision.main").expect("decision");
        DecisionTraceDraft {
            payload: DecisionTraceV1 {
                evaluation_id: evaluation_id(
                    package_hash,
                    &decision,
                    facts_hash,
                    evaluated_at,
                    &timezone_database,
                ),
                package: PackageId::new("pkg.main").expect("package"),
                package_version: Version::new("1.0.0").expect("version"),
                decision,
                evaluated_at,
                timezone: PolicyTimeZone::new("America/New_York").expect("timezone"),
                timezone_database,
                package_hash,
                facts_hash,
                compiler: "rulery.compiler/0.1.0".to_owned(),
                language_version: LanguageVersion::V1,
                outcome: (!conflict).then_some(outcome.clone()),
                determining_rules: if conflict {
                    Vec::new()
                } else {
                    vec![rule.clone()]
                },
                superseded_rules: vec![SupersededRuleTrace {
                    rule: QualifiedRuleId::new(
                        PackageId::new("pkg.main").expect("package"),
                        RuleId::new("rule.old").expect("rule"),
                    ),
                    reason: SupersessionReason::LowerSemanticKey,
                }],
                rule_traces: vec![RuleTrace {
                    rule: rule.clone(),
                    result: Truth::True,
                    selected: !conflict,
                    relevance: DecisionRelevance::Decisive,
                    condition,
                    candidate: Some(CandidateTrace {
                        outcome: outcome.clone(),
                        semantic_key: key,
                    }),
                    source_span: span,
                }],
                missing_facts: BTreeSet::from([FactPath::from_str("member.email").expect("path")]),
                invalid_facts: vec![InvalidFactTrace {
                    path: Some(FactPath::from_str("member.age").expect("path")),
                    error: FactValidationError::TypeMismatch {
                        expected: TypeId::new("type.age").expect("type"),
                        actual: rulery_contracts::ValueKind::Text,
                    },
                    span: Some(span),
                }],
                strategy_applications: vec![StrategyApplicationTrace {
                    strategy: StrategyKind::Missing,
                    relevant_paths: BTreeSet::from([
                        FactPath::from_str("member.email").expect("path")
                    ]),
                    candidate: Some(CandidateTrace {
                        outcome: outcome.clone(),
                        semantic_key: key,
                    }),
                }],
                conflict: conflict.then_some(DecisionConflictTrace {
                    key,
                    rules: vec![rule],
                    outcomes: vec![outcome],
                }),
                trace_hash: ContentHash::from_bytes([0; 32]),
            },
            defaulted: false,
        }
    }

    fn expression_count(trace: &ExpressionTrace) -> usize {
        match trace {
            ExpressionTrace::All { children, .. } | ExpressionTrace::Any { children, .. } => {
                1 + children.iter().map(expression_count).sum::<usize>()
            }
            ExpressionTrace::Not { child, .. } => 1 + expression_count(child),
            ExpressionTrace::Predicate(_)
            | ExpressionTrace::Constant { .. }
            | ExpressionTrace::NotEvaluated { .. } => 1,
        }
    }

    fn source_map() -> SourceMap {
        let mut map = SourceMap::new();
        map.insert(
            SourceKey::new(1),
            SourceFile::new(
                SourceId::new("source.main").expect("source"),
                SourcePath::new("rules/main.yaml").expect("path"),
                Arc::<str>::from("x"),
            ),
        )
        .expect("insert");
        map
    }
}
