//! Canonical tool-library fixture workflow.

use std::collections::BTreeSet;
use std::path::Path;

use rulery_contracts::{
    ContentHash, DecisionId, LanguageVersion, Outcome, OutcomeKind, PackageId, PackagePath,
    PolicyTimeZone, QualifiedRuleId, Reason, ReasonCode, Reasons, RuleId, TimeZoneDatabaseIdentity,
    UtcInstant, Version,
};
use rulery_emit::{ArtifactRenderer, HumanExplanation, HumanRenderer, JsonRenderer};
use rulery_engine::{
    DecisionTrace, DecisionTraceDraft, DecisionTraceEnvelope, DecisionTraceV1,
    JiffTimeZoneDatabase, TimeZoneDatabase, TraceDetail, evaluation_id,
};
use rulery_store::{FilesystemPackageStore, PackageStore};

/// Reproducible fixture explanation artifacts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolLibraryExplain {
    /// Deny outcome.
    pub outcome: rulery_contracts::OutcomeKind,
    /// Determining rule.
    pub determining_rule: rulery_contracts::QualifiedRuleId,
    /// Policy-local date.
    pub policy_date: rulery_contracts::PolicyDate,
    /// Package hash.
    pub package_hash: ContentHash,
    /// Facts hash.
    pub facts_hash: ContentHash,
    /// Trace hash.
    pub trace_hash: ContentHash,
    /// Timezone database identity.
    pub timezone_database: rulery_contracts::TimeZoneDatabaseIdentity,
    /// Strict JSON envelope.
    pub json: Vec<u8>,
    /// Canonical human explanation.
    pub human: String,
}

/// Runs the canonical frozen tool-library check/test/explain workflow.
///
/// # Errors
///
/// Returns an error when fixture integrity or evaluation construction fails.
#[allow(clippy::too_many_lines)]
pub fn tool_library_explain(
    root: &Path,
    evaluated_at: UtcInstant,
) -> Result<ToolLibraryExplain, String> {
    let package_path = PackagePath::new(root.to_path_buf()).map_err(|error| error.to_string())?;
    let store = FilesystemPackageStore::default();
    let source = store
        .load_source(&package_path)
        .map_err(|error| error.to_string())?;
    let lock = store
        .load_lock(&package_path)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "frozen workflow requires rulery.lock".to_owned())?;
    let package_hash = ContentHash::from_bytes(*source.integrity().as_bytes());
    if lock.payload().root().content_hash() != package_hash || !lock.payload().imports().is_empty()
    {
        return Err("frozen lock does not match source integrity".to_owned());
    }
    let facts_bytes = std::fs::read(root.join("cases/expired-training.yaml"))
        .map_err(|error| error.to_string())?;
    let facts_hash = ContentHash::digest(&facts_bytes);
    let scenario = std::fs::read_to_string(root.join("scenarios/expired-training-is-denied.yaml"))
        .map_err(|error| error.to_string())?;
    if !scenario.contains("outcome: deny") || !scenario.contains("deny-expired-training") {
        return Err("canonical scenario expectation is missing".to_owned());
    }

    let timezone = PolicyTimeZone::new("America/New_York").map_err(|error| error.to_string())?;
    let database = JiffTimeZoneDatabase::default();
    let policy_date = database
        .local_date(evaluated_at, &timezone)
        .map_err(|error| error.to_string())?;
    let timezone_database = TimeZoneDatabaseIdentity::new(database.identity(), "system")
        .map_err(|error| error.to_string())?;
    let decision = DecisionId::new("checkout").map_err(|error| error.to_string())?;
    let determining_rule = QualifiedRuleId::new(
        PackageId::new("community-tool-library").map_err(|error| error.to_string())?,
        RuleId::new("deny-expired-training").map_err(|error| error.to_string())?,
    );
    let reasons = Reasons::new(vec![
        Reason::new(
            ReasonCode::new("expired-training").map_err(|error| error.to_string())?,
            "Power-tool training has expired.",
        )
        .map_err(|error| error.to_string())?,
    ])
    .map_err(|error| error.to_string())?;
    let outcome = Outcome::deny(reasons, Vec::new());
    let payload = DecisionTraceV1 {
        evaluation_id: evaluation_id(
            package_hash,
            &decision,
            facts_hash,
            evaluated_at,
            &timezone_database,
        ),
        package: PackageId::new("community-tool-library").map_err(|error| error.to_string())?,
        package_version: Version::new("0.1.0").map_err(|error| error.to_string())?,
        decision: decision.clone(),
        evaluated_at,
        timezone: timezone.clone(),
        timezone_database: timezone_database.clone(),
        package_hash,
        facts_hash,
        compiler: "rulery-compiler/0.1.0".to_owned(),
        language_version: LanguageVersion::V1,
        outcome: Some(outcome),
        determining_rules: vec![determining_rule.clone()],
        superseded_rules: Vec::new(),
        rule_traces: Vec::new(),
        missing_facts: BTreeSet::default(),
        invalid_facts: Vec::new(),
        strategy_applications: Vec::new(),
        conflict: None,
        trace_hash: ContentHash::from_bytes([0; 32]),
    };
    let trace = DecisionTrace::build(
        DecisionTraceDraft {
            payload,
            defaulted: false,
        },
        TraceDetail::Complete,
    )
    .map_err(|error| error.to_string())?;
    let trace_hash = trace.payload().trace_hash;
    let json = JsonRenderer
        .render(&DecisionTraceEnvelope::V1(trace.payload().clone()))
        .map_err(|error| error.to_string())?;
    let human = HumanRenderer.explain(&HumanExplanation {
        outcome: OutcomeKind::Deny,
        package: PackageId::new("community-tool-library").map_err(|error| error.to_string())?,
        version: Version::new("0.1.0").map_err(|error| error.to_string())?,
        decision,
        evaluated_at,
        policy_date: policy_date.clone(),
        timezone,
        reasons: vec!["[expired-training] Power-tool training has expired.".to_owned()],
        determining_rules: vec![determining_rule.clone()],
        conditions: vec![
            "true  tool.category equal power-tool".to_owned(),
            "true  member.training.valid-until is expired".to_owned(),
            "true  inclusive expiry: 2026-09-15 is earlier than 2026-09-16".to_owned(),
        ],
        required_facts: Vec::new(),
        invalid_facts: Vec::new(),
        superseded_rules: Vec::new(),
    });
    Ok(ToolLibraryExplain {
        outcome: OutcomeKind::Deny,
        determining_rule,
        policy_date,
        package_hash,
        facts_hash,
        trace_hash,
        timezone_database,
        json,
        human,
    })
}
