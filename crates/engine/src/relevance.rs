//! Maximum-completion relevance for unresolved rules.

use std::collections::BTreeSet;

use rulery_contracts::FactPath;

use crate::{Candidate, PrecedenceError, PrecedenceModel, PredicateObservation, Truth};

/// Relevance of one evaluated rule to the final decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionRelevance {
    /// Rule produced a decisive candidate.
    Decisive,
    /// An unresolved completion can displace or conflict with the best decisive result.
    RelevantUnresolved,
    /// No unresolved completion can alter the selected result.
    Irrelevant,
}

/// Candidate a currently unresolved rule can instantiate when completed true.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnresolvedCandidate {
    /// Candidate produced by the maximum true completion.
    pub completion: Candidate,
    /// Current unresolved truth state.
    pub truth: Truth,
    /// Paths responsible for unresolved truth.
    pub paths: BTreeSet<FactPath>,
}

/// Observation paired with its rule's decision relevance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassifiedObservation {
    /// Original predicate observation retained for tracing.
    pub observation: PredicateObservation,
    /// Rule-level relevance classification.
    pub relevance: DecisionRelevance,
}

/// Classifies an unresolved candidate by maximum-completion displacement.
///
/// “Maximum completion” treats the unresolved rule as if its condition completed to true, then
/// compares that candidate with the best decisive candidate. A greater key can displace the
/// result; an equal key is relevant only when it would introduce an incompatible outcome.
///
/// # Errors
///
/// Returns [`PrecedenceError`] when explicit rank configuration is invalid.
pub fn classify_unresolved(
    model: &PrecedenceModel,
    best_decisive: Option<&Candidate>,
    unresolved: &UnresolvedCandidate,
) -> Result<DecisionRelevance, PrecedenceError> {
    match unresolved.truth {
        Truth::True => return Ok(DecisionRelevance::Decisive),
        Truth::False => return Ok(DecisionRelevance::Irrelevant),
        Truth::Unknown | Truth::Invalid => {}
    }
    let Some(best_decisive) = best_decisive else {
        return Ok(DecisionRelevance::RelevantUnresolved);
    };
    let completion_key = crate::precedence::semantic_key(model, &unresolved.completion)?;
    let decisive_key = crate::precedence::semantic_key(model, best_decisive)?;
    if completion_key > decisive_key
        || (completion_key == decisive_key
            && unresolved.completion.outcome != best_decisive.outcome)
    {
        Ok(DecisionRelevance::RelevantUnresolved)
    } else {
        Ok(DecisionRelevance::Irrelevant)
    }
}

/// Returns only observations allowed to feed uncertainty strategies.
#[must_use]
pub fn relevant_observations(values: &[ClassifiedObservation]) -> Vec<PredicateObservation> {
    values
        .iter()
        .filter(|value| value.relevance == DecisionRelevance::RelevantUnresolved)
        .map(|value| value.observation.clone())
        .collect()
}

/// Returns whether the declared default is eligible.
#[must_use]
pub const fn should_use_default(
    has_decisive_candidate: bool,
    has_relevant_strategy_candidate: bool,
) -> bool {
    !has_decisive_candidate && !has_relevant_strategy_candidate
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use rulery_contracts::{
        EscalationId, Outcome, PackageId, QualifiedRuleId, Reason, ReasonCode, Reasons, RuleId,
    };

    use crate::{EvidenceClass, InvalidFactStrategy, MissingFactStrategy, apply_strategies};

    use super::*;

    #[test]
    fn unresolved_rules_only_displace_when_completion_can_change_result() {
        let model = PrecedenceModel::PriorityFirst;
        let decisive = escalation_candidate("rule.decisive", "review.a", 10);
        let lower = unresolved("rule.lower", "review.b", 9, Truth::Unknown, "member.lower");
        let greater = unresolved(
            "rule.greater",
            "review.a",
            11,
            Truth::Unknown,
            "member.greater",
        );
        let equal_incompatible =
            unresolved("rule.equal", "review.b", 10, Truth::Invalid, "member.equal");

        assert_eq!(
            classify_unresolved(&model, Some(&decisive), &lower).expect("lower"),
            DecisionRelevance::Irrelevant
        );
        assert_eq!(
            classify_unresolved(&model, Some(&decisive), &greater).expect("greater"),
            DecisionRelevance::RelevantUnresolved
        );
        assert_eq!(
            classify_unresolved(&model, Some(&decisive), &equal_incompatible).expect("equal"),
            DecisionRelevance::RelevantUnresolved
        );

        let classified = vec![
            classified_observation(&lower, DecisionRelevance::Irrelevant),
            classified_observation(&greater, DecisionRelevance::RelevantUnresolved),
            classified_observation(&equal_incompatible, DecisionRelevance::RelevantUnresolved),
        ];
        assert_eq!(classified.len(), 3);
        let relevant = relevant_observations(&classified);
        assert_eq!(
            relevant
                .iter()
                .map(|entry| entry.path.to_string())
                .collect::<Vec<_>>(),
            vec!["member.greater", "member.equal"]
        );
        assert!(matches!(
            apply_strategies(
                &relevant,
                &MissingFactStrategy::PreserveUnknown,
                &InvalidFactStrategy::RejectEvaluation,
            ),
            Err(crate::EvaluationError::InvalidFact { paths }) if paths.iter().map(ToString::to_string).collect::<Vec<_>>() == vec!["member.equal"]
        ));

        assert!(!should_use_default(true, false));
        assert!(!should_use_default(false, true));
        assert!(should_use_default(false, false));
        assert_eq!(
            classify_unresolved(&model, None, &lower).expect("no decisive"),
            DecisionRelevance::RelevantUnresolved
        );
    }

    fn unresolved(
        rule: &str,
        destination: &str,
        priority: i32,
        truth: Truth,
        path: &str,
    ) -> UnresolvedCandidate {
        UnresolvedCandidate {
            completion: escalation_candidate(rule, destination, priority),
            truth,
            paths: BTreeSet::from([FactPath::from_str(path).expect("path")]),
        }
    }

    fn escalation_candidate(rule: &str, destination: &str, priority: i32) -> Candidate {
        let reasons = Reasons::new(vec![
            Reason::new(ReasonCode::new("test").expect("code"), "test").expect("reason"),
        ])
        .expect("reasons");
        Candidate {
            rule: QualifiedRuleId::new(
                PackageId::new("pkg.main").expect("package"),
                RuleId::new(rule).expect("rule"),
            ),
            outcome: Outcome::escalate(
                EscalationId::new(destination).expect("destination"),
                reasons,
                Vec::new(),
            ),
            priority,
            specificity: 4,
            override_rank: 0,
        }
    }

    fn classified_observation(
        unresolved: &UnresolvedCandidate,
        relevance: DecisionRelevance,
    ) -> ClassifiedObservation {
        let path = unresolved.paths.iter().next().expect("path").clone();
        ClassifiedObservation {
            observation: PredicateObservation {
                path,
                truth: unresolved.truth,
                evidence: if unresolved.truth == Truth::Invalid {
                    EvidenceClass::Malformed
                } else {
                    EvidenceClass::Absent
                },
                priority: i64::from(unresolved.completion.priority),
                specificity: unresolved.completion.specificity,
                override_rank: unresolved.completion.override_rank,
            },
            relevance,
        }
    }
}
