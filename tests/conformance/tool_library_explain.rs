//! End-to-end reproducibility checks for the tool-library explain workflow.

use std::fs;

use rulery::contracts::{
    ContentHash, DecisionId, LanguageVersion, Outcome, OutcomeKind, PackagePath,
    TimeZoneDatabaseIdentity, UtcInstant,
};
use rulery::emit::{ArtifactRenderer, JsonRenderer};
use rulery::engine::{DecisionTraceEnvelope, Truth, evaluation_id};
use rulery::{ApplicationService, LockMode, ProductionApplication};

const DETERMINING_RULE: &str = "community-tool-library::deny-expired-training";

#[test]
fn tool_library_explain_is_reproducible_end_to_end() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/tool-library");
    let at = UtcInstant::new(1_789_574_400_000_000_000).expect("instant");
    let first = rulery::tool_library_explain(&root, at).expect("first explain");
    let second = rulery::tool_library_explain(&root, at).expect("second explain");
    assert_eq!(first.json, second.json);
    assert_eq!(first.trace, second.trace);
    assert_eq!(first.trace_hash, second.trace_hash);
    assert_eq!(first.human, second.human);

    let trace = &first.trace;
    assert_eq!(first.outcome, OutcomeKind::Deny);
    assert_eq!(first.policy_date.to_string(), "2026-09-16");
    assert_eq!(first.determining_rule.to_string(), DETERMINING_RULE);
    assert_eq!(
        trace.outcome.as_ref().map(rulery::contracts::Outcome::kind),
        Some(OutcomeKind::Deny)
    );

    // The trace records the compiled package and the evaluated inputs, not authored literals.
    assert_eq!(trace.package.to_string(), "community-tool-library");
    assert_eq!(trace.package_version.to_string(), "0.1.0");
    assert_eq!(trace.compiler, "rulery-compiler/0.1.0");
    assert_eq!(trace.language_version, LanguageVersion::V1);
    assert_eq!(trace.evaluated_at, at);
    assert_eq!(trace.timezone.as_str(), "America/New_York");
    assert_eq!(trace.package_hash, first.package_hash);
    assert_eq!(trace.facts_hash, first.facts_hash);
    assert_eq!(trace.trace_hash, first.trace_hash);
    assert_eq!(trace.timezone_database, first.timezone_database);
    assert_eq!(
        trace.evaluation_id,
        evaluation_id(
            trace.package_hash,
            &DecisionId::new("checkout").expect("decision"),
            trace.facts_hash,
            at,
            &TimeZoneDatabaseIdentity::new("fixture", "pinned-policy-calendar-2026a")
                .expect("database"),
        )
    );

    // Every authored rule of the decision was evaluated, and only the expired-training rule
    // determined the outcome.
    assert_eq!(trace.rule_traces.len(), 4);
    assert_eq!(
        trace.determining_rules,
        vec![first.determining_rule.clone()]
    );
    let selected = trace
        .rule_traces
        .iter()
        .filter(|entry| entry.selected)
        .map(|entry| (entry.rule.to_string(), entry.result))
        .collect::<Vec<_>>();
    assert_eq!(selected, vec![(DETERMINING_RULE.to_owned(), Truth::True)]);
    assert!(
        trace
            .rule_traces
            .iter()
            .filter(|entry| !entry.selected)
            .all(|entry| entry.result == Truth::False)
    );
    assert!(
        trace
            .rule_traces
            .iter()
            .filter(|entry| entry.selected)
            .all(|entry| entry.candidate.is_some())
    );
    assert!(trace.missing_facts.is_empty());
    assert!(trace.invalid_facts.is_empty());
    assert!(trace.strategy_applications.is_empty());
    assert!(trace.superseded_rules.is_empty());
    assert!(trace.conflict.is_none());

    // The fixture calendar is pinned, so the workflow never reads host timezone data.
    assert_eq!(trace.timezone_database.implementation(), "fixture");
    assert_eq!(
        trace.timezone_database.version(),
        "pinned-policy-calendar-2026a"
    );

    let json = String::from_utf8(first.json.clone()).expect("json");
    assert!(json.contains("expired-training"));
    assert!(json.contains(&first.trace_hash.to_string()));
    assert!(json.contains(&first.facts_hash.to_string()));
    assert_eq!(
        first.json,
        JsonRenderer
            .render(&DecisionTraceEnvelope::V1(trace.clone()))
            .expect("rendered envelope")
    );
    for hash in [first.package_hash, first.facts_hash, first.trace_hash] {
        assert_ne!(hash, ContentHash::from_bytes([0; 32]));
    }

    assert_eq!(
        first.human,
        fs::read_to_string(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/tool-library-explain.txt")
        )
        .expect("human fixture")
    );
}

#[test]
fn canonical_scenario_compiles_and_passes_with_record_facts() {
    // The authored scenario supplies `member` and `tool` as nested mappings, and its `at` as RFC
    // 3339. Both only work if the authored literal keeps its structure to the vocabulary-guided
    // decoder and the bridge reads the authored instant form rather than the wire form.
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/tool-library");
    let package = PackagePath::new(root).expect("package path");
    let service = ProductionApplication::new();

    let compiled = service
        .compile_package(&package, LockMode::Frozen)
        .expect("canonical example compiles");
    assert_eq!(compiled.scenarios.len(), 1);
    assert_eq!(
        compiled.scenarios[0].at.to_rfc3339(),
        "2026-09-16T16:00:00.000000000Z"
    );

    let package = compiled.package.as_ref().expect("compiled package");
    let results = service
        .run_scenarios(package, &compiled.scenarios)
        .expect("scenarios run");
    assert_eq!(results.len(), 1);
    let result = results[0].payload();
    assert_eq!(result.status, rulery::scenarios::ScenarioStatus::Passed);
    assert!(result.failures.is_empty());
    assert_eq!(
        result
            .trace
            .as_ref()
            .map(|t| t.outcome.as_ref().map(Outcome::kind)),
        Some(Some(OutcomeKind::Deny))
    );
}
