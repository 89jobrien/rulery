//! Package-level policy evaluation.

use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

use rulery_contracts::{
    CaseFacts, ContentHash, DecisionId, FactPath, FactValidationError, HashDomain, Outcome, Reason,
    ReasonCode, Reasons, RequiredFacts, RuleId, TimeZoneDatabaseIdentity, UtcInstant, Value,
    hash_parts,
};
use rulery_ir::{
    CompiledPackage, DecisionPrecedence, ExpiryPolicy, Expr, ExprOperand, InvalidFactStrategy,
    MissingFactStrategy, Operator, PrecedenceDimension, Predicate, ReservedOperand,
};
use rulery_vocabulary::{FactLookupState, validate_case_facts};
use thiserror::Error;

use crate::{
    Candidate, CandidateTrace, ClockOperand, DecisionConflictTrace, DecisionRelevance,
    DecisionTrace, DecisionTraceDraft, DecisionTraceV1, EvaluatedOperand, ExpressionTrace,
    InvalidFactTrace, PrecedenceModel, PredicateObservation, PredicateTrace, RuleTrace,
    SemanticPrecedenceKey, StrategyApplicationTrace, StrategyKind, SupersededRuleTrace,
    SupersessionReason, TimeZoneDatabase, TimeZoneError, TraceDetail, Truth, classify_unresolved,
    evaluate_binary, evaluate_presence, select_candidates, should_use_default,
};

/// Typed port for deterministic package-level decision evaluation.
pub trait PolicyEvaluator {
    /// Evaluates one compiled decision at an explicitly supplied instant.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown decision, relevant rejected invalid facts, time-zone
    /// conversion failures, or an invalid trace construction. Runtime candidate conflicts are
    /// returned in the successful trace as evidence.
    fn evaluate(
        &self,
        package: &CompiledPackage,
        decision: &DecisionId,
        facts: &CaseFacts,
        evaluated_at: UtcInstant,
        detail: TraceDetail,
    ) -> Result<DecisionTrace, PolicyEvaluationError>;
}

/// Production evaluator backed by an explicitly provided time-zone database.
pub struct ProductionPolicyEvaluator<'a> {
    time_zones: &'a dyn TimeZoneDatabase,
}

impl<'a> ProductionPolicyEvaluator<'a> {
    /// Creates an evaluator using the supplied versioned time-zone database.
    #[must_use]
    pub const fn new(time_zones: &'a dyn TimeZoneDatabase) -> Self {
        Self { time_zones }
    }
}

/// Package-level evaluation failure.
#[derive(Debug, Error)]
pub enum PolicyEvaluationError {
    /// The requested decision is not in the compiled package.
    #[error("unknown decision `{0}`")]
    UnknownDecision(DecisionId),
    /// Relevant malformed facts were rejected by compiled semantics.
    #[error("decision-relevant facts are invalid")]
    InvalidFact(BTreeSet<FactPath>),
    /// Policy-local date conversion failed.
    #[error(transparent)]
    TimeZone(#[from] TimeZoneError),
    /// Precedence configuration was unexpectedly invalid.
    #[error(transparent)]
    Precedence(#[from] crate::PrecedenceError),
    /// A valid evaluator result could not satisfy trace invariants.
    #[error(transparent)]
    Trace(#[from] crate::TraceError),
    /// Case facts could not be canonically serialized for trace identity.
    #[error("failed to serialize case facts: {0}")]
    FactsSerialization(serde_json::Error),
}

impl PolicyEvaluator for ProductionPolicyEvaluator<'_> {
    #[allow(clippy::too_many_lines)]
    fn evaluate(
        &self,
        package: &CompiledPackage,
        decision_id: &DecisionId,
        facts: &CaseFacts,
        evaluated_at: UtcInstant,
        detail: TraceDetail,
    ) -> Result<DecisionTrace, PolicyEvaluationError> {
        let decision = package
            .payload()
            .decisions()
            .get(decision_id)
            .ok_or_else(|| PolicyEvaluationError::UnknownDecision(decision_id.clone()))?;
        let semantics = decision.semantics();
        let today = self
            .time_zones
            .local_date(evaluated_at, semantics.timezone())?;
        let precedence = precedence_model(semantics.precedence())?;
        let validated = validate_case_facts(package.vocabulary(), facts);
        let facts_hash = facts_hash(facts)?;
        let timezone_database = timezone_identity(self.time_zones.identity());

        let mut evaluated = Vec::new();
        for rule in decision.rules().values() {
            let mut observations = Vec::new();
            let (truth, condition) = evaluate_expression(
                rule.condition(),
                &validated.states,
                evaluated_at,
                &today,
                semantics.expiry(),
                &mut observations,
                rule.priority(),
                rule.specificity(),
                rule.override_rank(),
            );
            let candidate = (truth == Truth::True).then(|| Candidate {
                rule: rule.qualified_id().clone(),
                outcome: rule.outcome().clone(),
                priority: rule.priority(),
                specificity: rule.specificity(),
                override_rank: rule.override_rank(),
            });
            evaluated.push(EvaluatedRule {
                truth,
                condition,
                observations,
                candidate,
                rule,
            });
        }

        let decisive = evaluated
            .iter()
            .filter_map(|entry| entry.candidate.clone())
            .collect::<Vec<_>>();
        let initial = select_candidates(&precedence, decisive.clone())?;
        let best = initial
            .determining_rules
            .first()
            .and_then(|id| decisive.iter().find(|candidate| &candidate.rule == id));
        let mut relevant = Vec::new();
        for entry in &evaluated {
            let relevance = if let Some(candidate) = &entry.candidate {
                let _ = candidate;
                DecisionRelevance::Decisive
            } else {
                classify_unresolved(
                    &precedence,
                    best,
                    &crate::UnresolvedCandidate {
                        completion: Candidate {
                            rule: entry.rule.qualified_id().clone(),
                            outcome: entry.rule.outcome().clone(),
                            priority: entry.rule.priority(),
                            specificity: entry.rule.specificity(),
                            override_rank: entry.rule.override_rank(),
                        },
                        truth: entry.truth,
                        paths: entry
                            .observations
                            .iter()
                            .filter(|observation| {
                                observation.truth != Truth::True
                                    && observation.truth != Truth::False
                            })
                            .map(|observation| observation.path.clone())
                            .collect(),
                    },
                )?
            };
            if relevance == DecisionRelevance::RelevantUnresolved {
                relevant.extend(entry.observations.clone());
            }
        }

        let missing = relevant
            .iter()
            .filter(|observation| observation.evidence == crate::EvidenceClass::Absent)
            .map(|observation| observation.path.clone())
            .collect::<BTreeSet<_>>();
        let invalid = relevant
            .iter()
            .filter(|observation| observation.evidence == crate::EvidenceClass::Malformed)
            .map(|observation| observation.path.clone())
            .collect::<BTreeSet<_>>();
        if !invalid.is_empty()
            && matches!(
                semantics.invalid_facts(),
                InvalidFactStrategy::RejectEvaluation
            )
        {
            return Err(PolicyEvaluationError::InvalidFact(invalid));
        }

        let application = crate::apply_strategies(
            &relevant,
            &missing_strategy(semantics.missing_facts()),
            &invalid_strategy(semantics.invalid_facts()),
        )
        .map_err(|error| match error {
            crate::EvaluationError::InvalidFact { paths } => {
                PolicyEvaluationError::InvalidFact(paths)
            }
        })?;
        let strategy_candidates = application
            .candidates
            .iter()
            .enumerate()
            .map(|(index, candidate)| strategy_candidate(package, candidate, index))
            .collect::<Vec<_>>();
        let strategy_applications =
            strategy_traces(&missing, &invalid, &strategy_candidates, &precedence);
        let selection = select_candidates(
            &precedence,
            decisive
                .iter()
                .cloned()
                .chain(strategy_candidates.iter().cloned())
                .collect(),
        )?;
        let defaulted = should_use_default(!decisive.is_empty(), !strategy_candidates.is_empty());
        let outcome = if defaulted {
            Some(decision.default().outcome().clone())
        } else {
            selection.outcome.clone()
        };
        let conflict = selection
            .conflict
            .as_ref()
            .map(|conflict| DecisionConflictTrace {
                key: conflict.key,
                rules: conflict
                    .participants
                    .iter()
                    .map(|candidate| candidate.rule.clone())
                    .collect(),
                outcomes: conflict
                    .participants
                    .iter()
                    .map(|candidate| candidate.outcome.clone())
                    .collect(),
            });
        let rule_traces = evaluated
            .into_iter()
            .map(|entry| {
                let relevance = if entry.candidate.is_some() {
                    DecisionRelevance::Decisive
                } else if entry.truth == Truth::Unknown || entry.truth == Truth::Invalid {
                    DecisionRelevance::RelevantUnresolved
                } else {
                    DecisionRelevance::Irrelevant
                };
                RuleTrace {
                    rule: entry.rule.qualified_id().clone(),
                    result: entry.truth,
                    selected: selection
                        .determining_rules
                        .contains(entry.rule.qualified_id()),
                    relevance,
                    condition: entry.condition,
                    candidate: entry.candidate.as_ref().and_then(|candidate| {
                        semantic_key(&precedence, candidate)
                            .ok()
                            .map(|semantic_key| CandidateTrace {
                                outcome: candidate.outcome.clone(),
                                semantic_key,
                            })
                    }),
                    source_span: entry.rule.span(),
                }
            })
            .collect();
        let superseded_rules = selection
            .superseded
            .into_iter()
            .map(|entry| SupersededRuleTrace {
                rule: entry.rule,
                reason: SupersessionReason::LowerSemanticKey,
            })
            .collect();
        let trace_hash = ContentHash::from_bytes([0; 32]);
        DecisionTrace::build(
            DecisionTraceDraft {
                defaulted,
                payload: DecisionTraceV1 {
                    evaluation_id: ContentHash::from_bytes([0; 32]),
                    package: package.payload().package_id().clone(),
                    package_version: package.payload().package_version().clone(),
                    decision: decision_id.clone(),
                    evaluated_at,
                    timezone: semantics.timezone().clone(),
                    timezone_database,
                    package_hash: package.payload().package_hash(),
                    facts_hash,
                    compiler: package.payload().compiler_identity().to_owned(),
                    language_version: package.payload().language_version(),
                    outcome,
                    determining_rules: selection.determining_rules,
                    superseded_rules,
                    rule_traces,
                    missing_facts: missing,
                    invalid_facts: invalid_fact_traces(&validated.states),
                    strategy_applications,
                    conflict,
                    trace_hash,
                },
            },
            detail,
        )
        .map_err(Into::into)
    }
}

struct EvaluatedRule<'a> {
    truth: Truth,
    condition: ExpressionTrace,
    observations: Vec<PredicateObservation>,
    candidate: Option<Candidate>,
    rule: &'a rulery_ir::CompiledRule,
}

fn facts_hash(facts: &CaseFacts) -> Result<ContentHash, PolicyEvaluationError> {
    let bytes = serde_json::to_vec(facts).map_err(PolicyEvaluationError::FactsSerialization)?;
    Ok(hash_parts(HashDomain::CaseFactsV1, [bytes.as_slice()]))
}

fn timezone_identity(value: &str) -> TimeZoneDatabaseIdentity {
    let (implementation, version) = value.split_once('/').unwrap_or((value, "unspecified"));
    TimeZoneDatabaseIdentity::new(implementation, version).unwrap_or_else(|_| {
        TimeZoneDatabaseIdentity::new("unknown", "unspecified").expect("static identity")
    })
}

fn precedence_model(value: &DecisionPrecedence) -> Result<PrecedenceModel, PolicyEvaluationError> {
    Ok(match value {
        DecisionPrecedence::SafetyFirst => PrecedenceModel::SafetyFirst,
        DecisionPrecedence::PriorityFirst => PrecedenceModel::PriorityFirst,
        DecisionPrecedence::Explicit {
            primary,
            outcome_ranks,
        } => {
            let ranks = crate::ExplicitRanks::new(
                outcome_ranks
                    .iter()
                    .map(|(kind, rank)| (*kind, *rank))
                    .collect(),
            )?;
            match primary {
                PrecedenceDimension::Outcome => PrecedenceModel::ExplicitOutcome(ranks),
                PrecedenceDimension::Priority => PrecedenceModel::ExplicitPriority(ranks),
            }
        }
    })
}

fn semantic_key(
    model: &PrecedenceModel,
    candidate: &Candidate,
) -> Result<SemanticPrecedenceKey, crate::PrecedenceError> {
    // Selecting one candidate returns its exact semantic key without exposing precedence internals.
    crate::precedence::semantic_key(model, candidate)
}

fn missing_strategy(value: &MissingFactStrategy) -> crate::MissingFactStrategy {
    match value {
        MissingFactStrategy::PreserveUnknown => crate::MissingFactStrategy::PreserveUnknown,
        MissingFactStrategy::ClosedWorldFalse => crate::MissingFactStrategy::ClosedWorldFalse,
        MissingFactStrategy::RequestInformation => crate::MissingFactStrategy::RequestInformation,
        MissingFactStrategy::Escalate { destination } => crate::MissingFactStrategy::Escalate {
            destination: destination.clone(),
        },
    }
}

fn invalid_strategy(value: &InvalidFactStrategy) -> crate::InvalidFactStrategy {
    match value {
        InvalidFactStrategy::RejectEvaluation => crate::InvalidFactStrategy::RejectEvaluation,
        InvalidFactStrategy::PreserveInvalid => crate::InvalidFactStrategy::PreserveInvalid,
        InvalidFactStrategy::Escalate { destination } => crate::InvalidFactStrategy::Escalate {
            destination: destination.clone(),
        },
    }
}

fn strategy_candidate(
    package: &CompiledPackage,
    candidate: &crate::StrategyCandidate,
    index: usize,
) -> Candidate {
    let reasons = Reasons::new(
        candidate
            .reasons
            .iter()
            .map(|reason| {
                Reason::new(
                    ReasonCode::new(reason.code).expect("fixed strategy reason code"),
                    reason.message,
                )
                .expect("fixed strategy reason")
            })
            .collect(),
    )
    .expect("strategy candidates always have a reason");
    let outcome = match &candidate.outcome {
        crate::StrategyOutcome::RequestInformation => Outcome::request_information(
            RequiredFacts::new(candidate.required_facts.clone())
                .expect("request strategy candidates always have facts"),
            reasons,
            Vec::new(),
        ),
        crate::StrategyOutcome::Escalate(destination) => {
            Outcome::escalate(destination.clone(), reasons, Vec::new())
        }
    };
    let rule = RuleId::new(format!("strategy.generated.{index}"))
        .expect("fixed generated strategy rule id");
    Candidate {
        rule: rulery_contracts::QualifiedRuleId::new(package.payload().package_id().clone(), rule),
        outcome,
        priority: i32::try_from(candidate.priority).unwrap_or(
            if candidate.priority.is_negative() {
                i32::MIN
            } else {
                i32::MAX
            },
        ),
        specificity: candidate.specificity,
        override_rank: candidate.override_rank,
    }
}

fn strategy_traces(
    missing: &BTreeSet<FactPath>,
    invalid: &BTreeSet<FactPath>,
    candidates: &[Candidate],
    precedence: &PrecedenceModel,
) -> Vec<StrategyApplicationTrace> {
    let mut values = Vec::new();
    if !missing.is_empty() {
        values.push(StrategyApplicationTrace {
            strategy: StrategyKind::Missing,
            relevant_paths: missing.clone(),
            candidate: candidates
                .iter()
                .find(|candidate| {
                    candidate.outcome.kind() == rulery_contracts::OutcomeKind::RequestInformation
                })
                .and_then(|candidate| {
                    semantic_key(precedence, candidate)
                        .ok()
                        .map(|semantic_key| CandidateTrace {
                            outcome: candidate.outcome.clone(),
                            semantic_key,
                        })
                }),
        });
    }
    if !invalid.is_empty() {
        values.push(StrategyApplicationTrace {
            strategy: StrategyKind::Invalid,
            relevant_paths: invalid.clone(),
            candidate: candidates
                .iter()
                .find(|candidate| {
                    candidate.outcome.kind() == rulery_contracts::OutcomeKind::Escalate
                })
                .and_then(|candidate| {
                    semantic_key(precedence, candidate)
                        .ok()
                        .map(|semantic_key| CandidateTrace {
                            outcome: candidate.outcome.clone(),
                            semantic_key,
                        })
                }),
        });
    }
    values
}

fn invalid_fact_traces(states: &BTreeMap<String, FactLookupState<'_>>) -> Vec<InvalidFactTrace> {
    states
        .iter()
        .filter_map(|(path, state)| match state {
            FactLookupState::Malformed(error) => {
                FactPath::from_str(path).ok().map(|path| InvalidFactTrace {
                    path: Some(path),
                    error: error.clone(),
                    span: None,
                })
            }
            _ => None,
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn evaluate_expression(
    expression: &Expr,
    facts: &BTreeMap<String, FactLookupState<'_>>,
    now: UtcInstant,
    today: &rulery_contracts::PolicyDate,
    expiry: ExpiryPolicy,
    observations: &mut Vec<PredicateObservation>,
    priority: i32,
    specificity: u32,
    override_rank: u8,
) -> (Truth, ExpressionTrace) {
    match expression {
        Expr::Constant { value, span } => (
            Truth::from(*value),
            ExpressionTrace::Constant {
                result: Truth::from(*value),
                value: *value,
                span: *span,
            },
        ),
        Expr::Predicate(predicate) => evaluate_predicate(
            predicate,
            facts,
            now,
            today,
            expiry,
            observations,
            priority,
            specificity,
            override_rank,
        ),
        Expr::Not { expression, span } => {
            let (truth, child) = evaluate_expression(
                expression,
                facts,
                now,
                today,
                expiry,
                observations,
                priority,
                specificity,
                override_rank,
            );
            (
                truth.not(),
                ExpressionTrace::Not {
                    result: truth.not(),
                    child: Box::new(child),
                    span: *span,
                },
            )
        }
        Expr::All { expressions, span } => {
            let children = expressions
                .iter()
                .map(|child| {
                    evaluate_expression(
                        child,
                        facts,
                        now,
                        today,
                        expiry,
                        observations,
                        priority,
                        specificity,
                        override_rank,
                    )
                })
                .collect::<Vec<_>>();
            let result = children
                .iter()
                .fold(Truth::True, |value, (truth, _)| value.and(*truth));
            (
                result,
                ExpressionTrace::All {
                    result,
                    children: children.into_iter().map(|(_, trace)| trace).collect(),
                    span: *span,
                },
            )
        }
        Expr::Any { expressions, span } => {
            let children = expressions
                .iter()
                .map(|child| {
                    evaluate_expression(
                        child,
                        facts,
                        now,
                        today,
                        expiry,
                        observations,
                        priority,
                        specificity,
                        override_rank,
                    )
                })
                .collect::<Vec<_>>();
            let result = children
                .iter()
                .fold(Truth::False, |value, (truth, _)| value.or(*truth));
            (
                result,
                ExpressionTrace::Any {
                    result,
                    children: children.into_iter().map(|(_, trace)| trace).collect(),
                    span: *span,
                },
            )
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn evaluate_predicate(
    predicate: &Predicate,
    facts: &BTreeMap<String, FactLookupState<'_>>,
    now: UtcInstant,
    today: &rulery_contracts::PolicyDate,
    _expiry: ExpiryPolicy,
    observations: &mut Vec<PredicateObservation>,
    priority: i32,
    specificity: u32,
    override_rank: u8,
) -> (Truth, ExpressionTrace) {
    let (left, left_trace, path) = operand(&predicate.left, facts, now, today);
    let (right, right_trace, _) = predicate.right.as_ref().map_or(
        (RuntimeOperand::Owned(Value::Null), None, None),
        |expression_operand| {
            let (value, trace, path) = operand(expression_operand, facts, now, today);
            (value, Some(trace), path)
        },
    );
    let left_state = left.state();
    let right_state = right.state();
    let truth = match predicate.operator {
        Operator::Exists => evaluate_presence(crate::PresencePredicate::IsPresent, left_state),
        Operator::Missing => evaluate_presence(crate::PresencePredicate::IsAbsent, left_state),
        Operator::Equals => {
            evaluate_binary(crate::BinaryPredicate::Equals, left_state, right_state)
        }
        Operator::NotEquals => {
            evaluate_binary(crate::BinaryPredicate::NotEquals, left_state, right_state)
        }
        Operator::LessThan | Operator::Before => {
            evaluate_binary(crate::BinaryPredicate::LessThan, left_state, right_state)
        }
        Operator::LessOrEqual | Operator::OnOrBefore => {
            evaluate_binary(crate::BinaryPredicate::LessOrEqual, left_state, right_state)
        }
        Operator::GreaterThan | Operator::After => {
            evaluate_binary(crate::BinaryPredicate::GreaterThan, left_state, right_state)
        }
        Operator::GreaterOrEqual | Operator::OnOrAfter => evaluate_binary(
            crate::BinaryPredicate::GreaterOrEqual,
            left_state,
            right_state,
        ),
        Operator::Contains => {
            evaluate_binary(crate::BinaryPredicate::Contains, left_state, right_state)
        }
        Operator::StartsWith => {
            evaluate_binary(crate::BinaryPredicate::StartsWith, left_state, right_state)
        }
        Operator::EndsWith => {
            evaluate_binary(crate::BinaryPredicate::EndsWith, left_state, right_state)
        }
        Operator::IsOneOf => {
            evaluate_binary(crate::BinaryPredicate::IsOneOf, left_state, right_state)
        }
        Operator::IsTrue => evaluate_binary(
            crate::BinaryPredicate::Equals,
            left_state,
            crate::OperandState::Valid(&Value::Boolean(true)),
        ),
        Operator::IsFalse => evaluate_binary(
            crate::BinaryPredicate::Equals,
            left_state,
            crate::OperandState::Valid(&Value::Boolean(false)),
        ),
        Operator::Matches | Operator::Between => Truth::Invalid,
    };
    if let Some(path) = path {
        observations.push(PredicateObservation {
            path,
            truth,
            evidence: match left_state {
                crate::OperandState::Absent => crate::EvidenceClass::Absent,
                crate::OperandState::Malformed(_) => crate::EvidenceClass::Malformed,
                crate::OperandState::Null | crate::OperandState::Valid(_) => {
                    crate::EvidenceClass::Supplied
                }
            },
            priority: i64::from(priority),
            specificity,
            override_rank,
        });
    }
    (
        truth,
        ExpressionTrace::Predicate(PredicateTrace {
            result: truth,
            operator: predicate.operator,
            lhs: left_trace,
            rhs: right_trace,
            span: predicate.span,
        }),
    )
}

enum RuntimeOperand<'a> {
    Absent,
    Null,
    Malformed(&'a FactValidationError),
    Borrowed(&'a Value),
    Owned(Value),
}

impl RuntimeOperand<'_> {
    const fn state(&self) -> crate::OperandState<'_> {
        match self {
            Self::Absent => crate::OperandState::Absent,
            Self::Null => crate::OperandState::Null,
            Self::Malformed(error) => crate::OperandState::Malformed(error),
            Self::Borrowed(value) => crate::OperandState::Valid(value),
            Self::Owned(value) => crate::OperandState::Valid(value),
        }
    }
}

fn operand<'a>(
    operand: &'a ExprOperand,
    facts: &'a BTreeMap<String, FactLookupState<'a>>,
    now: UtcInstant,
    today: &'a rulery_contracts::PolicyDate,
) -> (RuntimeOperand<'a>, EvaluatedOperand, Option<FactPath>) {
    match operand {
        ExprOperand::Fact(path) => match facts.get(&path.to_string()) {
            None | Some(FactLookupState::Absent) => (
                RuntimeOperand::Absent,
                EvaluatedOperand::Missing { path: path.clone() },
                Some(path.clone()),
            ),
            Some(FactLookupState::Null) => (
                RuntimeOperand::Null,
                EvaluatedOperand::Value {
                    value: Value::Null,
                    source_path: Some(path.clone()),
                },
                Some(path.clone()),
            ),
            Some(FactLookupState::Valid(value)) => (
                RuntimeOperand::Borrowed(value),
                EvaluatedOperand::Value {
                    value: (*value).clone(),
                    source_path: Some(path.clone()),
                },
                Some(path.clone()),
            ),
            Some(FactLookupState::Malformed(error)) => (
                RuntimeOperand::Malformed(error),
                EvaluatedOperand::Invalid {
                    path: Some(path.clone()),
                    error: error.clone(),
                },
                Some(path.clone()),
            ),
        },
        ExprOperand::Literal(value) => (
            RuntimeOperand::Borrowed(value),
            EvaluatedOperand::Value {
                value: value.clone(),
                source_path: None,
            },
            None,
        ),
        ExprOperand::Reserved(ReservedOperand::Now) => {
            let value = Value::DateTime(now);
            (
                RuntimeOperand::Owned(value.clone()),
                EvaluatedOperand::ClockValue {
                    value,
                    name: ClockOperand::Now,
                },
                None,
            )
        }
        ExprOperand::Reserved(ReservedOperand::Today) => {
            let value = Value::Date(today.clone());
            (
                RuntimeOperand::Owned(value.clone()),
                EvaluatedOperand::ClockValue {
                    value,
                    name: ClockOperand::Today,
                },
                None,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rulery_contracts::{
        EscalationId, FactRootId, LanguageVersion, OutcomeKind, PackageId, PolicyDate,
        PolicyTimeZone, QualifiedRuleId, SourceFile, SourceId, SourceKey, SourceMap, SourcePath,
        Span, StableId, TypeId, ValueKind, Version,
    };
    use rulery_ir::{
        CompilationInput, CompiledDecision, CompiledEffect, CompiledPackageDraft, CompiledRule,
        DecisionSemantics, FieldDeclaration, FieldPresence, PackageIntegritySet, ResolvedRoot,
        ResolvedType, ResolvedVocabulary, TypeDeclaration,
    };

    use super::*;

    #[allow(clippy::too_many_lines)]
    #[test]
    fn evaluate_records_decisive_rule_and_missing_evidence() {
        let database = FixedTimeZoneDatabase::default();
        let evaluator = ProductionPolicyEvaluator::new(&database);
        let package = compiled_package();
        let facts = member_facts(&[(
            "status",
            Value::List(vec![Value::Text("active".to_owned())]),
        )]);

        let trace = evaluate(&evaluator, &package, "decision.access", &facts)
            .payload()
            .clone();

        assert_eq!(
            trace.outcome.as_ref().map(Outcome::kind),
            Some(OutcomeKind::Approve)
        );
        assert_eq!(outcome_codes(&trace), vec!["member-active"]);
        assert_eq!(trace.determining_rules, vec![qualified("rule.allow")]);
        assert!(trace.superseded_rules.is_empty());
        assert!(trace.conflict.is_none());
        assert_eq!(trace.evaluated_at, evaluated_at());
        assert_eq!(
            trace.timezone_database,
            TimeZoneDatabaseIdentity::new("fixture", "fixed-tzdb-2026a").expect("database")
        );
        assert_eq!(trace.timezone_database.implementation(), "fixture");
        assert_eq!(
            trace.missing_facts,
            BTreeSet::from([fact_path("member.age")])
        );
        assert!(trace.invalid_facts.is_empty());
        assert_eq!(
            trace.strategy_applications,
            vec![StrategyApplicationTrace {
                strategy: StrategyKind::Missing,
                relevant_paths: BTreeSet::from([fact_path("member.age")]),
                candidate: None,
            }]
        );

        assert_eq!(trace.rule_traces.len(), 3);
        let allow = rule_trace(&trace, "rule.allow");
        assert_eq!(allow.result, Truth::True);
        assert!(allow.selected);
        assert_eq!(allow.relevance, DecisionRelevance::Decisive);
        assert_eq!(
            allow
                .candidate
                .as_ref()
                .map(|candidate| candidate.semantic_key),
            Some(SemanticPrecedenceKey::PriorityFirst(50, 1_000, 3, 0))
        );
        assert_eq!(
            allow
                .candidate
                .as_ref()
                .map(|candidate| candidate.outcome.kind()),
            Some(OutcomeKind::Approve)
        );

        let block = rule_trace(&trace, "rule.block");
        assert_eq!(block.result, Truth::False);
        assert!(!block.selected);
        assert_eq!(block.relevance, DecisionRelevance::Irrelevant);
        assert!(block.candidate.is_none());

        let review = rule_trace(&trace, "rule.review");
        assert_eq!(review.result, Truth::Unknown);
        assert!(!review.selected);
        assert_eq!(review.relevance, DecisionRelevance::RelevantUnresolved);

        assert_eq!(
            today_operand(&trace, "rule.allow"),
            Some(Value::Date(policy_date("2026-03-08")))
        );
    }

    #[test]
    fn evaluate_falls_back_to_default_when_no_rule_determines() {
        let database = FixedTimeZoneDatabase::default();
        let evaluator = ProductionPolicyEvaluator::new(&database);
        let package = compiled_package();
        let facts = member_facts(&[(
            "status",
            Value::List(vec![Value::Text("unlisted".to_owned())]),
        )]);

        let trace = evaluate(&evaluator, &package, "decision.access", &facts)
            .payload()
            .clone();

        assert_eq!(
            trace.outcome.as_ref().map(Outcome::kind),
            Some(OutcomeKind::Deny)
        );
        assert_eq!(outcome_codes(&trace), vec!["no-matching-rule"]);
        assert!(trace.determining_rules.is_empty());
        assert!(trace.rule_traces.iter().all(|entry| !entry.selected));
        assert_eq!(rule_trace(&trace, "rule.allow").result, Truth::False);
        assert_eq!(rule_trace(&trace, "rule.block").result, Truth::False);
        assert_eq!(rule_trace(&trace, "rule.review").result, Truth::Unknown);
        assert_eq!(
            trace.missing_facts,
            BTreeSet::from([fact_path("member.age")])
        );
    }

    #[test]
    fn evaluate_rejects_unknown_decision_identity() {
        let database = FixedTimeZoneDatabase::default();
        let evaluator = ProductionPolicyEvaluator::new(&database);
        let package = compiled_package();
        let facts = member_facts(&[]);

        let error = evaluator
            .evaluate(
                &package,
                &DecisionId::new("decision.absent").expect("decision"),
                &facts,
                evaluated_at(),
                TraceDetail::Complete,
            )
            .expect_err("unknown decision");
        assert!(
            matches!(error, PolicyEvaluationError::UnknownDecision(ref id) if id.as_str() == "decision.absent"),
            "{error}"
        );
        assert!(error.to_string().contains("unknown decision"), "{error}");
    }

    #[allow(clippy::too_many_lines)]
    #[test]
    fn evaluate_applies_compiled_invalid_fact_strategy() {
        let database = FixedTimeZoneDatabase::default();
        let evaluator = ProductionPolicyEvaluator::new(&database);
        let package = compiled_package();
        let facts = member_facts(&[
            (
                "status",
                Value::List(vec![Value::Text("active".to_owned())]),
            ),
            ("age", Value::Text("sixty".to_owned())),
        ]);

        let error = evaluator
            .evaluate(
                &package,
                &DecisionId::new("decision.strict").expect("decision"),
                &facts,
                evaluated_at(),
                TraceDetail::Complete,
            )
            .expect_err("rejected facts");
        assert!(
            matches!(error, PolicyEvaluationError::InvalidFact(ref paths) if paths == &BTreeSet::from([fact_path("member.age")])),
            "{error}"
        );

        let trace = evaluate(&evaluator, &package, "decision.access", &facts)
            .payload()
            .clone();
        assert_eq!(
            trace.outcome.as_ref().map(Outcome::kind),
            Some(OutcomeKind::Approve)
        );
        assert_eq!(trace.determining_rules, vec![qualified("rule.allow")]);
        assert!(trace.missing_facts.is_empty());
        assert_eq!(
            trace.invalid_facts,
            vec![InvalidFactTrace {
                path: Some(fact_path("member.age")),
                error: FactValidationError::TypeMismatch {
                    expected: TypeId::new("type.age").expect("type"),
                    actual: ValueKind::Text,
                },
                span: None,
            }]
        );
        assert_eq!(
            trace.strategy_applications,
            vec![StrategyApplicationTrace {
                strategy: StrategyKind::Invalid,
                relevant_paths: BTreeSet::from([fact_path("member.age")]),
                candidate: None,
            }]
        );
        let review = rule_trace(&trace, "rule.review");
        assert_eq!(review.result, Truth::Invalid);
        assert_eq!(review.relevance, DecisionRelevance::RelevantUnresolved);
    }

    #[test]
    fn evaluate_is_reproducible_for_one_instant_and_database_identity() {
        let database = FixedTimeZoneDatabase::default();
        let evaluator = ProductionPolicyEvaluator::new(&database);
        let package = compiled_package();
        let facts = member_facts(&[(
            "status",
            Value::List(vec![Value::Text("active".to_owned())]),
        )]);

        let first = evaluate(&evaluator, &package, "decision.access", &facts);
        let second = evaluate(&evaluator, &package, "decision.access", &facts);
        assert_eq!(first, second);
        assert_eq!(
            first.payload().evaluation_id,
            second.payload().evaluation_id
        );
        assert_eq!(first.payload().trace_hash, second.payload().trace_hash);

        let other = member_facts(&[(
            "status",
            Value::List(vec![Value::Text("unlisted".to_owned())]),
        )]);
        let changed = evaluate(&evaluator, &package, "decision.access", &other);
        assert_ne!(first.payload().facts_hash, changed.payload().facts_hash);
        assert_ne!(
            first.payload().evaluation_id,
            changed.payload().evaluation_id
        );
    }

    fn evaluate(
        evaluator: &ProductionPolicyEvaluator<'_>,
        package: &CompiledPackage,
        decision: &str,
        facts: &CaseFacts,
    ) -> DecisionTrace {
        evaluator
            .evaluate(
                package,
                &DecisionId::new(decision).expect("decision"),
                facts,
                evaluated_at(),
                TraceDetail::Complete,
            )
            .expect("decision trace")
    }

    fn compiled_package() -> CompiledPackage {
        let (source_map, span) = fixture_source_map();
        CompiledPackage::new(
            CompiledPackageDraft {
                package_id: PackageId::new("pkg.main").expect("package"),
                package_version: Version::new("1.0.0").expect("version"),
                language_version: LanguageVersion::V1,
                compiler_identity: "compiler".to_owned(),
                decisions: vec![access_decision(span), strict_decision(span)],
                actions: Vec::new(),
                integrity: PackageIntegritySet::new(CompilationInput::new(
                    ContentHash::from_bytes([1; 32]),
                    ContentHash::from_bytes([2; 32]),
                    None,
                )),
            },
            source_map,
            fixture_vocabulary(),
        )
        .expect("package")
    }

    #[allow(clippy::too_many_lines)]
    fn access_decision(span: Span) -> CompiledDecision {
        CompiledDecision::new(
            DecisionId::new("decision.access").expect("decision"),
            semantics(
                MissingFactStrategy::PreserveUnknown,
                InvalidFactStrategy::PreserveInvalid,
            ),
            deny_effect("no-matching-rule", "No access rule matched this member."),
            vec![
                compiled_rule(
                    "rule.block",
                    100,
                    status_is("blocked", span),
                    deny_effect("member-blocked", "A blocked member is denied."),
                    3,
                    span,
                ),
                compiled_rule(
                    "rule.review",
                    75,
                    age_is(70, span),
                    escalate_effect("age-review", "The exact age 70 needs a human review."),
                    2,
                    span,
                ),
                compiled_rule(
                    "rule.allow",
                    50,
                    Expr::All {
                        expressions: vec![
                            status_is("active", span),
                            Expr::Predicate(Predicate {
                                operator: Operator::Equals,
                                left: ExprOperand::Reserved(ReservedOperand::Today),
                                right: Some(ExprOperand::Literal(Value::Date(policy_date(
                                    "2026-03-08",
                                )))),
                                span,
                            }),
                        ],
                        span,
                    },
                    approve_effect("member-active", "An active member is allowed."),
                    3,
                    span,
                ),
            ],
            span,
        )
        .expect("decision")
    }

    fn strict_decision(span: Span) -> CompiledDecision {
        CompiledDecision::new(
            DecisionId::new("decision.strict").expect("decision"),
            semantics(
                MissingFactStrategy::PreserveUnknown,
                InvalidFactStrategy::RejectEvaluation,
            ),
            deny_effect("no-strict-match", "No strict rule matched this member."),
            vec![compiled_rule(
                "rule.guard",
                200,
                age_is(70, span),
                escalate_effect(
                    "strict-age-review",
                    "An exact age of 70 needs a strict review.",
                ),
                1,
                span,
            )],
            span,
        )
        .expect("decision")
    }

    fn semantics(missing: MissingFactStrategy, invalid: InvalidFactStrategy) -> DecisionSemantics {
        DecisionSemantics::new(
            missing,
            invalid,
            DecisionPrecedence::PriorityFirst,
            PolicyTimeZone::new("UTC").expect("timezone"),
            ExpiryPolicy::Inclusive,
        )
        .expect("semantics")
    }

    #[allow(clippy::too_many_arguments)]
    fn compiled_rule(
        id: &str,
        priority: i32,
        condition: Expr,
        effect: CompiledEffect,
        specificity: u32,
        span: Span,
    ) -> CompiledRule {
        let rule = RuleId::new(id).expect("rule");
        CompiledRule::new(
            rule.clone(),
            QualifiedRuleId::new(PackageId::new("pkg.main").expect("package"), rule),
            None,
            priority,
            condition,
            effect,
            None,
            span,
            specificity,
        )
    }

    fn status_is(expected: &str, span: Span) -> Expr {
        Expr::Predicate(Predicate {
            operator: Operator::Contains,
            left: ExprOperand::Fact(fact_path("member.status")),
            right: Some(ExprOperand::Literal(Value::Text(expected.to_owned()))),
            span,
        })
    }

    fn age_is(expected: i64, span: Span) -> Expr {
        Expr::Predicate(Predicate {
            operator: Operator::Contains,
            left: ExprOperand::Fact(fact_path("member.age")),
            right: Some(ExprOperand::Literal(Value::Integer(expected))),
            span,
        })
    }

    fn approve_effect(code: &str, message: &str) -> CompiledEffect {
        CompiledEffect::new(Outcome::approve(reasons(code, message), Vec::new()))
    }

    fn deny_effect(code: &str, message: &str) -> CompiledEffect {
        CompiledEffect::new(Outcome::deny(reasons(code, message), Vec::new()))
    }

    fn escalate_effect(code: &str, message: &str) -> CompiledEffect {
        CompiledEffect::new(Outcome::escalate(
            EscalationId::new("review.safety").expect("destination"),
            reasons(code, message),
            Vec::new(),
        ))
    }

    fn reasons(code: &str, message: &str) -> Reasons {
        Reasons::new(vec![
            Reason::new(ReasonCode::new(code).expect("code"), message).expect("reason"),
        ])
        .expect("reasons")
    }

    #[allow(clippy::too_many_lines)]
    fn fixture_vocabulary() -> ResolvedVocabulary {
        let member_type = TypeId::new("type.member").expect("type");
        let status_type = TypeId::new("type.status").expect("type");
        let age_type = TypeId::new("type.age").expect("type");
        let text_type = TypeId::new("type.text").expect("type");
        let integer_type = TypeId::new("type.integer").expect("type");
        ResolvedVocabulary {
            roots: BTreeMap::from([(
                fact_path("member"),
                ResolvedRoot {
                    path: fact_path("member"),
                    type_id: member_type.clone(),
                },
            )]),
            types: BTreeMap::from([
                (
                    member_type.clone(),
                    ResolvedType {
                        id: member_type.clone(),
                        declaration: TypeDeclaration::Record {
                            id: member_type.clone(),
                            fields: BTreeMap::from([
                                (
                                    StableId::new("status").expect("field"),
                                    FieldDeclaration {
                                        type_id: status_type.clone(),
                                        presence: FieldPresence::Optional,
                                        derived: false,
                                    },
                                ),
                                (
                                    StableId::new("age").expect("field"),
                                    FieldDeclaration {
                                        type_id: age_type.clone(),
                                        presence: FieldPresence::Optional,
                                        derived: false,
                                    },
                                ),
                            ]),
                            closed: true,
                        },
                    },
                ),
                (
                    status_type.clone(),
                    ResolvedType {
                        id: status_type.clone(),
                        declaration: TypeDeclaration::List {
                            id: status_type.clone(),
                            element: text_type.clone(),
                            min_items: None,
                            max_items: None,
                        },
                    },
                ),
                (
                    age_type.clone(),
                    ResolvedType {
                        id: age_type.clone(),
                        declaration: TypeDeclaration::List {
                            id: age_type.clone(),
                            element: integer_type.clone(),
                            min_items: None,
                            max_items: None,
                        },
                    },
                ),
                (
                    text_type.clone(),
                    ResolvedType {
                        id: text_type,
                        declaration: TypeDeclaration::Primitive,
                    },
                ),
                (
                    integer_type.clone(),
                    ResolvedType {
                        id: integer_type,
                        declaration: TypeDeclaration::Primitive,
                    },
                ),
            ]),
            terms: BTreeMap::new(),
        }
    }

    fn fixture_source_map() -> (SourceMap, Span) {
        let mut map = SourceMap::new();
        map.insert(
            SourceKey::new(1),
            SourceFile::new(
                SourceId::new("source.main").expect("source"),
                SourcePath::new("rules/access.yaml").expect("path"),
                Arc::<str>::from("member access rules"),
            ),
        )
        .expect("source map");
        let span = map.span(SourceKey::new(1), 0, 6).expect("span");
        (map, span)
    }

    fn member_facts(fields: &[(&str, Value)]) -> CaseFacts {
        let record = fields
            .iter()
            .map(|(name, value)| (StableId::new(*name).expect("field"), value.clone()))
            .collect();
        CaseFacts::new(BTreeMap::from([(
            FactRootId::new("member").expect("root"),
            Value::Record(record),
        )]))
    }

    fn rule_trace(trace: &DecisionTraceV1, rule: &str) -> RuleTrace {
        trace
            .rule_traces
            .iter()
            .find(|entry| entry.rule == qualified(rule))
            .unwrap_or_else(|| panic!("missing rule trace for {rule}"))
            .clone()
    }

    fn today_operand(trace: &DecisionTraceV1, rule: &str) -> Option<Value> {
        let ExpressionTrace::All { children, .. } = &rule_trace(trace, rule).condition else {
            return None;
        };
        children.iter().find_map(|child| {
            let ExpressionTrace::Predicate(PredicateTrace { lhs, .. }) = child else {
                return None;
            };
            match lhs {
                EvaluatedOperand::ClockValue { value, name } if *name == ClockOperand::Today => {
                    Some(value.clone())
                }
                _ => None,
            }
        })
    }

    fn outcome_codes(trace: &DecisionTraceV1) -> Vec<String> {
        match trace.outcome.as_ref() {
            Some(
                Outcome::Approve { reasons, .. }
                | Outcome::Deny { reasons, .. }
                | Outcome::Escalate { reasons, .. }
                | Outcome::RequestInformation { reasons, .. },
            ) => reasons
                .iter()
                .map(|reason| reason.code().as_str().to_owned())
                .collect(),
            None => Vec::new(),
        }
    }

    fn qualified(rule: &str) -> QualifiedRuleId {
        QualifiedRuleId::new(
            PackageId::new("pkg.main").expect("package"),
            RuleId::new(rule).expect("rule"),
        )
    }

    fn fact_path(value: &str) -> FactPath {
        FactPath::from_str(value).expect("fact path")
    }

    fn policy_date(value: &str) -> PolicyDate {
        PolicyDate::parse(value).expect("date")
    }

    fn evaluated_at() -> UtcInstant {
        UtcInstant::new(1_772_955_000_000_000_000).expect("instant")
    }

    #[derive(Clone, Debug)]
    struct FixedTimeZoneDatabase {
        today: PolicyDate,
    }

    impl Default for FixedTimeZoneDatabase {
        fn default() -> Self {
            Self {
                today: policy_date("2026-03-08"),
            }
        }
    }

    impl TimeZoneDatabase for FixedTimeZoneDatabase {
        fn identity(&self) -> &'static str {
            "fixture/fixed-tzdb-2026a"
        }

        fn local_date(
            &self,
            _instant: UtcInstant,
            _zone: &PolicyTimeZone,
        ) -> Result<PolicyDate, TimeZoneError> {
            Ok(self.today.clone())
        }
    }
}
