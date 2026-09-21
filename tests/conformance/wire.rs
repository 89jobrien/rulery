use std::sync::Arc;

use rulery::analysis::{
    AnalysisCompleteness, AnalysisOptions, AnalysisReport, AnalysisReportEnvelope, AnalysisReportV1,
};
use rulery::contracts::{
    ActionId, ContentHash, DecisionId, EscalationId, LanguageVersion, PackageId, PackagePath,
    ReasonCode, RuleId, ScenarioId, SourceFile, SourceId, SourceKey, SourceMap, SourcePath,
    StableId, UtcInstant, Version,
};
use rulery::diagnostics::{
    DiagnosticLabel, DiagnosticReport, DiagnosticReportEnvelope, LabelStyle,
};
use rulery::emit::{ColumnRole, DecisionColumn, DecisionTableEnvelope, DecisionTableV1};
use rulery::engine::DecisionTraceEnvelope;
use rulery::ir::{
    CompilationInput, CompiledPackage, CompiledPackageDraft, CompiledPackageEnvelope,
    PackageIntegritySet, ResolvedVocabulary,
};
use rulery::scenarios::{ScenarioResult, ScenarioResultEnvelope, ScenarioResultV1, ScenarioStatus};
use rulery::store::{FilesystemPackageStore, PackageStore};
use serde::Serialize;
use serde::de::DeserializeOwned;

#[test]
fn all_v01_wire_contracts_round_trip_strictly() {
    for invalid in ["", "INVALID", "bad space", "a::b"] {
        assert!(serde_json::from_value::<StableId>(serde_json::json!(invalid)).is_err());
        assert!(serde_json::from_value::<PackageId>(serde_json::json!(invalid)).is_err());
        assert!(serde_json::from_value::<DecisionId>(serde_json::json!(invalid)).is_err());
        assert!(serde_json::from_value::<RuleId>(serde_json::json!(invalid)).is_err());
        assert!(serde_json::from_value::<ActionId>(serde_json::json!(invalid)).is_err());
        assert!(serde_json::from_value::<ScenarioId>(serde_json::json!(invalid)).is_err());
        assert!(serde_json::from_value::<EscalationId>(serde_json::json!(invalid)).is_err());
        assert!(serde_json::from_value::<ReasonCode>(serde_json::json!(invalid)).is_err());
    }
    assert!(
        serde_json::from_value::<rulery::contracts::FactPath>(serde_json::json!("a..b")).is_err()
    );
    assert!(serde_json::from_value::<SourcePath>(serde_json::json!("../escape")).is_err());
    assert!(serde_json::from_value::<SourceKey>(serde_json::json!(1)).is_err());
    let label = serde_json::to_value(
        DiagnosticLabel::new(
            source_map().span(SourceKey::new(1), 0, 1).expect("span"),
            LabelStyle::Primary,
            None,
        )
        .expect("label"),
    )
    .expect("label JSON");
    assert!(label.get("message").is_none());
    let paths = [
        "z.path"
            .parse::<rulery::contracts::FactPath>()
            .expect("path"),
        "a.path"
            .parse::<rulery::contracts::FactPath>()
            .expect("path"),
    ]
    .into_iter()
    .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        serde_json::to_value(paths).expect("set"),
        serde_json::json!(["a.path", "z.path"])
    );

    assert_strict(lock_envelope());
    assert_strict(compiled_envelope());
    assert_strict(diagnostic_envelope());
    let trace = trace_envelope();
    let trace_value = assert_strict(trace);
    assert!(trace_value["payload"]["outcome"].is_object());
    assert!(trace_value["payload"]["conflict"].is_null());
    assert!(trace_value["payload"]["evaluated_at"].is_string());
    assert_strict(scenario_envelope());
    let analysis = assert_strict(analysis_envelope());
    for field in [
        "reachability",
        "overlaps",
        "coverage",
        "witnesses",
        "semantic_diffs",
    ] {
        assert!(analysis["payload"][field].is_array());
    }
    let table = assert_strict(decision_table_envelope());
    let span = &table["payload"]["source_references"]["rule.main"][0];
    assert!(span["source"].is_string());
    assert!(span["start"].is_string());
    assert!(span["end"].is_string());
}

fn assert_strict<T>(envelope: T) -> serde_json::Value
where
    T: Clone + Serialize + DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let value = serde_json::to_value(&envelope).expect("serialize");
    let round_trip: T = serde_json::from_value(value.clone()).expect("round trip");
    assert_eq!(round_trip, envelope);

    let mut unknown = value.clone();
    unknown
        .as_object_mut()
        .expect("envelope")
        .insert("unexpected".to_owned(), serde_json::Value::Bool(true));
    assert!(serde_json::from_value::<T>(unknown).is_err());
    let mut tag = value.clone();
    tag["schema"] = serde_json::Value::String("rulery.unknown/v1".to_owned());
    assert!(serde_json::from_value::<T>(tag).is_err());
    let mut missing = value.clone();
    missing.as_object_mut().expect("envelope").remove("payload");
    assert!(serde_json::from_value::<T>(missing).is_err());
    let mut payload_unknown = value.clone();
    payload_unknown["payload"]["unexpected"] = serde_json::Value::Bool(true);
    assert!(serde_json::from_value::<T>(payload_unknown).is_err());
    value
}

fn lock_envelope() -> rulery::contracts::RulebookLockEnvelope {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/tool-library");
    let store = FilesystemPackageStore::default();
    let lock = store
        .load_lock(&PackagePath::new(root).expect("path"))
        .expect("load")
        .expect("lock");
    lock.into()
}

fn compiled_envelope() -> CompiledPackageEnvelope {
    let package = CompiledPackage::new(
        CompiledPackageDraft {
            package_id: PackageId::new("pkg.main").expect("package"),
            package_version: Version::new("1.0.0").expect("version"),
            language_version: LanguageVersion::V1,
            compiler_identity: "compiler".to_owned(),
            decisions: Vec::new(),
            actions: Vec::new(),
            integrity: PackageIntegritySet::new(CompilationInput::new(
                ContentHash::from_bytes([1; 32]),
                ContentHash::from_bytes([2; 32]),
                None,
            )),
        },
        source_map(),
        ResolvedVocabulary::default(),
    )
    .expect("package");
    CompiledPackageEnvelope::V1(package.payload().clone())
}

fn diagnostic_envelope() -> DiagnosticReportEnvelope {
    DiagnosticReportEnvelope::V1(
        DiagnosticReport::new(Vec::new(), "test", LanguageVersion::V1)
            .expect("report")
            .payload()
            .clone(),
    )
}

fn trace_envelope() -> DecisionTraceEnvelope {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/tool-library");
    serde_json::from_slice(
        &rulery::tool_library_explain(
            &root,
            UtcInstant::new(1_789_574_400_000_000_000).expect("instant"),
        )
        .expect("explain")
        .json,
    )
    .expect("trace")
}

fn scenario_envelope() -> ScenarioResultEnvelope {
    let result = ScenarioResult::new(ScenarioResultV1 {
        package_hash: ContentHash::from_bytes([1; 32]),
        scenario: ScenarioId::new("scenario.main").expect("scenario"),
        status: ScenarioStatus::Passed,
        failures: Vec::new(),
        trace: None,
    })
    .expect("result");
    ScenarioResultEnvelope::V1(result.payload().clone())
}

fn analysis_envelope() -> AnalysisReportEnvelope {
    let diagnostics =
        DiagnosticReport::new(Vec::new(), "analysis", LanguageVersion::V1).expect("diagnostics");
    let report = AnalysisReport::new(AnalysisReportV1 {
        package_hash: ContentHash::from_bytes([1; 32]),
        options: AnalysisOptions::default(),
        completeness: AnalysisCompleteness::Complete,
        diagnostics: diagnostics.payload().clone(),
        reachability: Vec::new(),
        overlaps: Vec::new(),
        coverage: Vec::new(),
        witnesses: Vec::new(),
        semantic_diffs: Vec::new(),
    });
    AnalysisReportEnvelope::V1(report.payload().clone())
}

fn decision_table_envelope() -> DecisionTableEnvelope {
    let map = source_map();
    let span = map.span(SourceKey::new(1), 0, 1).expect("span");
    DecisionTableEnvelope::V1(DecisionTableV1 {
        package_hash: ContentHash::from_bytes([1; 32]),
        decision: DecisionId::new("decision.main").expect("decision"),
        columns: vec![DecisionColumn {
            id: StableId::new("outcome").expect("column"),
            label: "Outcome".to_owned(),
            role: ColumnRole::Outcome,
        }],
        rows: Vec::new(),
        source_references: [(RuleId::new("rule.main").expect("rule"), vec![span])]
            .into_iter()
            .collect(),
    })
}

fn source_map() -> SourceMap {
    let mut map = SourceMap::new();
    map.insert(
        SourceKey::new(1),
        SourceFile::new(
            SourceId::new("source.main").expect("source"),
            SourcePath::new("main.yaml").expect("path"),
            Arc::<str>::from("x"),
        ),
    )
    .expect("insert");
    map
}
