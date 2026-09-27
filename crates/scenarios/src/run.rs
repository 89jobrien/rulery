//! Exact scenario execution and comparison.

use std::collections::BTreeSet;

use rulery_contracts::{
    ContentHash, FactPath, OutcomeKind, QualifiedRuleId, ReasonCode, UtcInstant,
};
use rulery_engine::DecisionTraceV1;

use crate::{
    CompiledScenario, ScenarioExpectationField, ScenarioFailure, ScenarioResult, ScenarioResultV1,
    ScenarioStatus,
};

/// Actual decision projection compared by scenario assertions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActualDecision {
    /// Actual outcome kind.
    pub outcome: OutcomeKind,
    /// Actual determining rules.
    pub determining_rules: BTreeSet<QualifiedRuleId>,
    /// Actual required facts.
    pub required_facts: BTreeSet<FactPath>,
    /// Actual reason codes.
    pub reason_codes: BTreeSet<ReasonCode>,
    /// Complete decision trace.
    pub trace: DecisionTraceV1,
}

/// Typed evaluator response for scenario status mapping.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EvaluationResult {
    /// Evaluation produced a decision.
    Decision(Box<ActualDecision>),
    /// Scenario facts were invalid.
    Invalid,
}

/// Evaluation port used by the scenario runner.
pub trait ScenarioEvaluator {
    /// Evaluates a compiled scenario at its fixed instant.
    ///
    /// # Errors
    ///
    /// Returns an error string for operational evaluation failures.
    fn evaluate(
        &self,
        scenario: &CompiledScenario,
        at: UtcInstant,
    ) -> Result<EvaluationResult, String>;
}

/// Exact scenario runner.
#[derive(Clone, Debug)]
pub struct ScenarioRunner<E> {
    evaluator: E,
}

impl<E> ScenarioRunner<E> {
    /// Creates a runner around an evaluator port.
    #[must_use]
    pub const fn new(evaluator: E) -> Self {
        Self { evaluator }
    }
}

impl<E: ScenarioEvaluator> ScenarioRunner<E> {
    /// Runs one scenario and compares all expectation fields exactly.
    #[must_use]
    pub fn run(&self, package_hash: ContentHash, scenario: &CompiledScenario) -> ScenarioResult {
        let (status, failures, trace) = match self.evaluator.evaluate(scenario, scenario.at) {
            Ok(EvaluationResult::Decision(actual)) => {
                let failures = compare(scenario, &actual);
                let status = if failures.is_empty() {
                    ScenarioStatus::Passed
                } else {
                    ScenarioStatus::Failed
                };
                (status, failures, Some(actual.trace))
            }
            Ok(EvaluationResult::Invalid) => (ScenarioStatus::Invalid, Vec::new(), None),
            Err(_) => (ScenarioStatus::Error, Vec::new(), None),
        };
        ScenarioResult::from_validated(ScenarioResultV1 {
            package_hash,
            scenario: scenario.id.clone(),
            status,
            failures,
            trace,
        })
    }
}

fn compare(scenario: &CompiledScenario, actual: &ActualDecision) -> Vec<ScenarioFailure> {
    let expected = &scenario.expect;
    let mut failures = Vec::new();
    push_mismatch(
        &mut failures,
        ScenarioExpectationField::Outcome,
        &expected.outcome,
        &actual.outcome,
    );
    push_mismatch(
        &mut failures,
        ScenarioExpectationField::DeterminingRules,
        &expected.determining_rules,
        &actual.determining_rules,
    );
    push_mismatch(
        &mut failures,
        ScenarioExpectationField::RequiredFacts,
        &expected.required_facts,
        &actual.required_facts,
    );
    push_mismatch(
        &mut failures,
        ScenarioExpectationField::ReasonCodes,
        &expected.reason_codes,
        &actual.reason_codes,
    );
    failures
}

fn push_mismatch<T: serde::Serialize + PartialEq>(
    failures: &mut Vec<ScenarioFailure>,
    field: ScenarioExpectationField,
    expected: &T,
    actual: &T,
) {
    if expected != actual {
        failures.push(ScenarioFailure {
            field,
            expected: serde_json::to_value(expected).unwrap_or(serde_json::Value::Null),
            actual: serde_json::to_value(actual).unwrap_or(serde_json::Value::Null),
        });
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::str::FromStr;
    use std::sync::{Arc, Mutex};

    use crate::{ExpectedDecision, ScenarioResultEnvelope};
    use rulery_contracts::{
        DecisionId, LanguageVersion, PackageId, PolicyTimeZone, ReasonCode, RuleId, ScenarioId,
        SourceFile, SourceId, SourceKey, SourceMap, SourcePath, Span, StableId,
        TimeZoneDatabaseIdentity, Version,
    };

    use super::*;

    #[test]
    fn scenario_runner_reports_all_exact_mismatches() {
        let seen_at = Arc::new(Mutex::new(Vec::new()));
        let actual = actual_decision();
        let runner = ScenarioRunner::new(FakeEvaluator {
            result: Ok(EvaluationResult::Decision(Box::new(actual))),
            seen_at: seen_at.clone(),
        });
        let scenario = scenario();
        let package_hash = ContentHash::from_bytes([1; 32]);
        let result = runner.run(package_hash, &scenario);
        assert_eq!(result.payload().status, ScenarioStatus::Failed);
        assert_eq!(result.payload().failures.len(), 4);
        assert_eq!(
            result
                .payload()
                .failures
                .iter()
                .map(|failure| failure.field)
                .collect::<Vec<_>>(),
            vec![
                ScenarioExpectationField::Outcome,
                ScenarioExpectationField::DeterminingRules,
                ScenarioExpectationField::RequiredFacts,
                ScenarioExpectationField::ReasonCodes,
            ]
        );
        assert_eq!(
            result.payload().failures[0].expected,
            serde_json::json!("deny")
        );
        assert_eq!(
            result.payload().failures[0].actual,
            serde_json::json!("approve")
        );
        assert_eq!(seen_at.lock().expect("seen at").as_slice(), &[scenario.at]);
        let encoded = serde_json::to_value(ScenarioResultEnvelope::V1(result.payload().clone()))
            .expect("envelope");
        assert_eq!(encoded["schema"], "rulery.scenario-result/v1");

        let passed_scenario = matching_scenario();
        let passed = ScenarioRunner::new(FakeEvaluator {
            result: Ok(EvaluationResult::Decision(Box::new(actual_decision()))),
            seen_at: Arc::new(Mutex::new(Vec::new())),
        })
        .run(package_hash, &passed_scenario);
        assert_eq!(passed.payload().status, ScenarioStatus::Passed);

        let invalid = ScenarioRunner::new(FakeEvaluator {
            result: Ok(EvaluationResult::Invalid),
            seen_at: Arc::new(Mutex::new(Vec::new())),
        })
        .run(package_hash, &scenario);
        assert_eq!(invalid.payload().status, ScenarioStatus::Invalid);
        let error = ScenarioRunner::new(FakeEvaluator {
            result: Err("boom".to_owned()),
            seen_at: Arc::new(Mutex::new(Vec::new())),
        })
        .run(package_hash, &scenario);
        assert_eq!(error.payload().status, ScenarioStatus::Error);
    }

    #[derive(Clone)]
    struct FakeEvaluator {
        result: Result<EvaluationResult, String>,
        seen_at: Arc<Mutex<Vec<UtcInstant>>>,
    }

    impl ScenarioEvaluator for FakeEvaluator {
        fn evaluate(
            &self,
            _scenario: &CompiledScenario,
            at: UtcInstant,
        ) -> Result<EvaluationResult, String> {
            self.seen_at.lock().expect("seen at").push(at);
            self.result.clone()
        }
    }

    fn scenario() -> CompiledScenario {
        let mut scenario = matching_scenario();
        scenario.expect.outcome = OutcomeKind::Deny;
        scenario.expect.determining_rules = BTreeSet::from([qualified("rule.expected")]);
        scenario.expect.required_facts =
            BTreeSet::from([FactPath::from_str("member.expected").expect("path")]);
        scenario.expect.reason_codes =
            BTreeSet::from([ReasonCode::new("expected").expect("reason")]);
        scenario
    }

    fn matching_scenario() -> CompiledScenario {
        CompiledScenario {
            id: ScenarioId::new("scenario.main").expect("scenario"),
            title: "main".to_owned(),
            description: None,
            decision: DecisionId::new("decision.main").expect("decision"),
            at: UtcInstant::new(55).expect("instant"),
            given: BTreeMap::new(),
            expect: ExpectedDecision {
                outcome: OutcomeKind::Approve,
                determining_rules: BTreeSet::from([qualified("rule.actual")]),
                required_facts: BTreeSet::from(
                    [FactPath::from_str("member.actual").expect("path")],
                ),
                reason_codes: BTreeSet::from([ReasonCode::new("actual").expect("reason")]),
            },
            tags: BTreeSet::from([StableId::new("test").expect("tag")]),
            span: span(),
        }
    }

    fn actual_decision() -> ActualDecision {
        let trace = DecisionTraceV1 {
            evaluation_id: ContentHash::from_bytes([9; 32]),
            package: PackageId::new("pkg.main").expect("package"),
            package_version: Version::new("1.0.0").expect("version"),
            decision: DecisionId::new("decision.main").expect("decision"),
            evaluated_at: UtcInstant::new(55).expect("instant"),
            timezone: PolicyTimeZone::new("UTC").expect("timezone"),
            timezone_database: TimeZoneDatabaseIdentity::new("test", "1").expect("database"),
            package_hash: ContentHash::from_bytes([1; 32]),
            facts_hash: ContentHash::from_bytes([2; 32]),
            compiler: "compiler".to_owned(),
            language_version: LanguageVersion::V1,
            outcome: None,
            determining_rules: Vec::new(),
            superseded_rules: Vec::new(),
            rule_traces: Vec::new(),
            missing_facts: BTreeSet::new(),
            invalid_facts: Vec::new(),
            strategy_applications: Vec::new(),
            conflict: None,
            trace_hash: ContentHash::from_bytes([3; 32]),
        };
        ActualDecision {
            outcome: OutcomeKind::Approve,
            determining_rules: BTreeSet::from([qualified("rule.actual")]),
            required_facts: BTreeSet::from([FactPath::from_str("member.actual").expect("path")]),
            reason_codes: BTreeSet::from([ReasonCode::new("actual").expect("reason")]),
            trace,
        }
    }

    fn qualified(rule: &str) -> QualifiedRuleId {
        QualifiedRuleId::new(
            PackageId::new("pkg.main").expect("package"),
            RuleId::new(rule).expect("rule"),
        )
    }

    fn span() -> Span {
        let mut map = SourceMap::new();
        map.insert(
            SourceKey::new(1),
            SourceFile::new(
                SourceId::new("source.main").expect("source"),
                SourcePath::new("scenario.yaml").expect("path"),
                Arc::<str>::from("x"),
            ),
        )
        .expect("source");
        map.span(SourceKey::new(1), 0, 1).expect("span")
    }
}
