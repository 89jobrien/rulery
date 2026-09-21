//! Synthetic minimal analysis witnesses.

use std::collections::{BTreeMap, BTreeSet};

use rulery_contracts::{
    ContentHash, FactPath, HashDomain, QualifiedRuleId, TimeZoneDatabaseIdentity, UtcInstant,
    Value, hash_parts,
};
use serde::{Deserialize, Serialize};

/// Claim reproduced by a witness.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "subject", rename_all = "snake_case")]
pub enum WitnessClaim {
    /// One rule is reachable.
    ReachableRule(QualifiedRuleId),
    /// Two rules overlap.
    RuleOverlap(QualifiedRuleId, QualifiedRuleId),
    /// Runtime conflict participants.
    Conflict(Vec<QualifiedRuleId>),
    /// An uncovered partition cell.
    Uncovered,
    /// A semantic behavior change.
    BehaviorChange,
}

/// Synthetic replayable witness case.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WitnessCase {
    /// Display title.
    pub title: String,
    /// Minimal canonical fact assignment.
    pub facts: BTreeMap<FactPath, Value>,
    /// Fixed evaluation instant.
    pub at: UtcInstant,
    /// Fixed timezone database identity.
    pub timezone_database: TimeZoneDatabaseIdentity,
    /// Paths relevant to the claim.
    pub relevant_paths: BTreeSet<FactPath>,
    /// Reproduced claim.
    pub claim: WitnessClaim,
    /// Case-facts domain hash.
    pub hash: ContentHash,
    /// Always true for analyzer-generated witnesses.
    pub synthetic: bool,
}

/// Deletes facts in ascending path order while the claim still replays.
#[must_use]
pub fn minimize_witness(
    title: impl Into<String>,
    mut facts: BTreeMap<FactPath, Value>,
    at: UtcInstant,
    timezone_database: TimeZoneDatabaseIdentity,
    relevant_paths: BTreeSet<FactPath>,
    claim: WitnessClaim,
    replays: impl Fn(&BTreeMap<FactPath, Value>, UtcInstant, &TimeZoneDatabaseIdentity) -> bool,
) -> WitnessCase {
    let paths = facts.keys().cloned().collect::<Vec<_>>();
    for path in paths {
        let mut candidate = facts.clone();
        candidate.remove(&path);
        if replays(&candidate, at, &timezone_database) {
            facts = candidate;
        }
    }
    let bytes = serde_json::to_vec(&facts).unwrap_or_default();
    let hash = hash_parts(HashDomain::CaseFactsV1, [bytes.as_slice()]);
    WitnessCase {
        title: title.into(),
        facts,
        at,
        timezone_database,
        relevant_paths,
        claim,
        hash,
        synthetic: true,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::str::FromStr;

    use rulery_contracts::{PackageId, RuleId};

    use crate::{AnalysisBudget, AnalysisOptions, BudgetResult};

    use super::*;

    #[test]
    fn budgets_stop_before_overrun_and_witnesses_replay() {
        let options = AnalysisOptions::default();
        assert_eq!(options.max_states, 100_000);
        assert_eq!(options.max_witnesses, 1_000);

        let called = Cell::new(0);
        let mut zero = AnalysisBudget::<String, bool>::new(0);
        assert_eq!(
            zero.resolve("decision.a:reachability".to_owned(), || {
                called.set(called.get() + 1);
                true
            }),
            BudgetResult::Inconclusive {
                examined: 0,
                limit: 0
            }
        );
        assert_eq!(called.get(), 0);

        let mut budget = AnalysisBudget::<String, bool>::new(2);
        assert_eq!(
            budget.resolve("decision.a:coverage".to_owned(), || true),
            BudgetResult::Value(true)
        );
        assert_eq!(
            budget.resolve("decision.a:coverage".to_owned(), || false),
            BudgetResult::Value(true)
        );
        assert_eq!(budget.examined(), 1);
        assert_eq!(
            budget.resolve("decision.b:interaction".to_owned(), || false),
            BudgetResult::Value(false)
        );
        assert_eq!(
            budget.resolve("decision.c:coverage".to_owned(), || true),
            BudgetResult::Inconclusive {
                examined: 2,
                limit: 2
            }
        );
        assert_eq!(
            budget.resolve("decision.d:coverage".to_owned(), || true),
            BudgetResult::Inconclusive {
                examined: 2,
                limit: 2
            }
        );

        let required = FactPath::from_str("member.required").expect("path");
        let removable = FactPath::from_str("member.removable").expect("path");
        let facts = BTreeMap::from([
            (removable.clone(), Value::Boolean(true)),
            (required.clone(), Value::Boolean(true)),
        ]);
        let at = UtcInstant::new(42).expect("instant");
        let database = TimeZoneDatabaseIdentity::new("test", "2026a").expect("database");
        let witness = minimize_witness(
            "reachable",
            facts,
            at,
            database.clone(),
            BTreeSet::from([required.clone()]),
            WitnessClaim::ReachableRule(QualifiedRuleId::new(
                PackageId::new("pkg.main").expect("package"),
                RuleId::new("rule.main").expect("rule"),
            )),
            |candidate, replay_at, replay_database| {
                replay_at == at && replay_database == &database && candidate.contains_key(&required)
            },
        );
        assert_eq!(witness.facts.keys().collect::<Vec<_>>(), vec![&required]);
        assert!(witness.synthetic);
        assert_eq!(witness.at, at);
        assert_eq!(witness.timezone_database, database);
        let bytes = serde_json::to_vec(&witness.facts).expect("facts json");
        assert_eq!(
            witness.hash,
            hash_parts(HashDomain::CaseFactsV1, [bytes.as_slice()])
        );
    }
}
