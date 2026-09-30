//! Out-of-tree embedding fixture: proves a published-style consumer can use the public facade.
//!
//! This binary is deliberately not a unit test. It is a separate workspace that depends on the
//! facade under a renamed name, so every line below compiles against `rulery`'s public re-exports
//! and nothing else. That is the surface a crates.io consumer depends on, and it is the only place
//! where the general evaluation path is exercised on a package this fixture authored itself.
//!
//! The normative tool-library fixture deliberately cannot stand in for that. Its calendar answers a
//! single canonical instant and its package is checked in, so it proves the *fixture* rather than
//! the *contract*. Every assertion here is written as a named function rather than an inline
//! `assert!` so the specification's requirement traceability table can name it.
//!
//! The binary exits non-zero and reports the failing assertion when any contract check fails, so
//! `cargo xtask embedding` can treat a run as the gate.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;

use rlry::compiler::{PolicyCompiler, SourceCompilationInput};
use rlry::contracts::{
    CaseFacts, ContentHash, DecimalValue, DecisionId, DurationValue, EnumValue, FactRootId,
    Outcome, OutcomeKind, PackagePath, PolicyDate, Reason, RulebookLockEnvelope, StableId, TypeId,
    UtcInstant, Value,
};
use rlry::engine::{
    JiffTimeZoneDatabase, PolicyEvaluator, ProductionPolicyEvaluator, TimeZoneDatabase, TraceDetail,
};
use rlry::ir::CompiledPackage;
use rlry::store::{FilesystemPackageStore, PackageStore};
use rlry::syntax::YamlSourceParser;
use rlry::vocabulary::{ResolvedVocabulary, TypeDeclaration};
use rlry::{LockMode, PackageAssembler, PackageAssemblyService};
use serde_json::Value as JsonValue;

/// The decision this fixture's authored package evaluates.
const DECISION: &str = "promote";

/// One fact root the authored package declares.
const ROOT: &str = "release";

/// Canonical instant every case is evaluated at, `2026-09-16T16:00:00Z`.
///
/// The package declares `UTC`, so the policy-local date is stable on every host and the trace hashes
/// below are reproducible without any timezone data from the machine running the gate.
const CANONICAL_INSTANT: i128 = 1_789_574_400_000_000_000;

/// Timezone database identity recorded in every trace.
const DATABASE_IDENTITY: &str = "embedding-fixture-utc";

/// One contract failure, reported as a line so a failing run names every broken guarantee.
type Failures = Vec<String>;

/// One named contract check, taking the authored package root and reporting why it failed.
///
/// Each check is a distinct named function rather than an inline assertion so the specification's
/// requirement traceability table can name it, and so a failing run says which guarantee broke
/// rather than only that something did.
type ContractCheck = (&'static str, fn(&Path) -> Result<(), String>);

/// The outcome a consumer reads back, independent of how the trace spells it.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Decision {
    /// Coarse outcome kind.
    kind: OutcomeKind,
    /// Qualified identity of the rule that determined the outcome.
    rule: String,
    /// Reproducible trace hash.
    trace_hash: String,
    /// Reason codes the outcome carries, in order.
    reason_codes: Vec<String>,
    /// Facts an information request asks the host to supply.
    required_facts: Vec<String>,
}

impl Decision {
    /// Returns the outcome kind's canonical authored name.
    fn kind_name(&self) -> &'static str {
        match self.kind {
            OutcomeKind::Approve => "approve",
            OutcomeKind::Deny => "deny",
            OutcomeKind::Escalate => "escalate",
            OutcomeKind::RequestInformation => "request_information",
        }
    }
}

fn main() -> ExitCode {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("policy");
    let mut failures = Failures::new();

    let base: Vec<ContractCheck> = vec![
        (
            "consumer_package_compiles_through_the_public_facade",
            consumer_package_compiles_through_the_public_facade,
        ),
        (
            "host_records_convert_to_case_facts",
            host_records_convert_to_case_facts,
        ),
        (
            "undeclared_host_fields_are_rejected",
            undeclared_host_fields_are_rejected,
        ),
        (
            "every_outcome_kind_is_reachable",
            every_outcome_kind_is_reachable,
        ),
        (
            "priority_resolves_overlapping_rules",
            priority_resolves_overlapping_rules,
        ),
        (
            "absent_source_escalates_instead_of_denying",
            absent_source_escalates_instead_of_denying,
        ),
        (
            "absent_rollback_plan_requests_information",
            absent_rollback_plan_requests_information,
        ),
        (
            "unmatched_cases_fall_back_to_the_default_outcome",
            unmatched_cases_fall_back_to_the_default_outcome,
        ),
        (
            "evaluation_is_reproducible_for_one_instant",
            evaluation_is_reproducible_for_one_instant,
        ),
        (
            "evaluation_is_stable_across_lock_modes",
            evaluation_is_stable_across_lock_modes,
        ),
    ];
    // The procedural derive is an optional surface, so the check list is extended only when the
    // feature that compiles it is enabled. Binding the two shapes separately keeps `mut` out of the
    // default configuration, where nothing is pushed.
    #[cfg(feature = "macros")]
    let checks = {
        let mut extended = base;
        extended.push((
            "derived_facts_are_accepted_by_the_evaluator",
            derived_facts_are_accepted_by_the_evaluator,
        ));
        extended
    };
    #[cfg(not(feature = "macros"))]
    let checks = base;

    let total = checks.len();
    for (name, check) in &checks {
        if let Err(report) = check(&root) {
            failures.push(format!("{name}: {report}"));
        }
    }

    if failures.is_empty() {
        println!("embedding contract: {total} checks passed");
        return ExitCode::SUCCESS;
    }
    for failure in &failures {
        eprintln!("embedding contract: {failure}");
    }
    ExitCode::FAILURE
}

/// Assembles and compiles the authored package through the public facade.
///
/// This is the chain a consumer owns end to end: read a package from disk, assemble its import
/// closure under a lock mode, and lower it to an executable package. A refactor that makes any
/// link require an internal path fails here rather than at a downstream compile.
fn consumer_package_compiles_through_the_public_facade(root: &Path) -> Result<(), String> {
    let package = load_package(root, LockMode::Frozen)?;
    let decision = package
        .payload()
        .decisions()
        .get(&DecisionId::new(DECISION).map_err(|error| error.to_string())?)
        .ok_or_else(|| format!("compiled package has no `{DECISION}` decision"))?;
    if decision.rules().is_empty() {
        return Err("compiled decision has no rules".to_owned());
    }
    Ok(())
}

/// Converts host-shaped JSON into typed case facts for the authored vocabulary.
///
/// A consumer always has its own domain types and must map them onto the policy's declared
/// vocabulary. That mapping is the adapter contract, and it is entirely the consumer's code: the
/// facade supplies `CaseFacts`, `Value`, and the compiled vocabulary to check against, and nothing
/// else.
fn host_records_convert_to_case_facts(root: &Path) -> Result<(), String> {
    let package = load_package(root, LockMode::Frozen)?;
    let facts = facts_from_host(
        &host_record(
            "production",
            Some("main"),
            Some("MBX-1"),
            Some("revert-1"),
            2,
        ),
        package.vocabulary(),
    )?;
    match facts.root(&FactRootId::new(ROOT).map_err(|error| error.to_string())?) {
        rlry::contracts::FactState::Valid(Value::Record(fields)) => {
            if fields.len() != 5 {
                return Err(format!(
                    "converted record has {} fields, expected 5",
                    fields.len()
                ));
            }
        }
        _ => return Err("converted root is not a valid record".to_owned()),
    }
    Ok(())
}

/// Rejects host data carrying a field the policy never declared.
///
/// The vocabulary is closed, so a host type that grows a field must be rejected rather than
/// silently ignored. A consumer that cannot detect that drift will keep serving stale policy.
fn undeclared_host_fields_are_rejected(root: &Path) -> Result<(), String> {
    let package = load_package(root, LockMode::Frozen)?;
    let record = host_record(
        "production",
        Some("main"),
        Some("MBX-1"),
        Some("revert-1"),
        2,
    );
    let JsonValue::Object(mut fields) = record else {
        return Err("host record must be an object".to_owned());
    };
    fields.insert("undeclared-field".to_owned(), JsonValue::from(true));
    match facts_from_host(&JsonValue::Object(fields), package.vocabulary()) {
        Ok(_) => Err("an undeclared host field was accepted".to_owned()),
        Err(report) if report.contains("not a declared field") => Ok(()),
        Err(report) => Err(format!(
            "undeclared field failed for the wrong reason: {report}"
        )),
    }
}

/// Evaluates one case per outcome kind through the public evaluator.
fn every_outcome_kind_is_reachable(root: &Path) -> Result<(), String> {
    let cases: &[(&str, JsonValue, &str, &str)] = &[
        (
            "promotable",
            host_record(
                "production",
                Some("main"),
                Some("MBX-1"),
                Some("revert-1"),
                2,
            ),
            "approve",
            "approve-promotion",
        ),
        (
            "missing ticket",
            host_record("production", Some("main"), None, Some("revert-1"), 2),
            "deny",
            "deny-missing-change-ticket",
        ),
        (
            "frozen branch",
            host_record(
                "staging",
                Some("frozen-2024"),
                Some("MBX-1"),
                Some("revert-1"),
                1,
            ),
            "deny",
            "deny-frozen-branch",
        ),
        (
            "unknown source",
            host_record("staging", None, Some("MBX-1"), Some("revert-1"), 1),
            "escalate",
            "escalate-unknown-source",
        ),
        (
            "no rollback plan",
            host_record("staging", Some("main"), Some("MBX-1"), None, 1),
            "request_information",
            "request-rollback-plan",
        ),
    ];

    let package = load_package(root, LockMode::Frozen)?;
    for (label, record, expected_kind, expected_rule) in cases {
        let decision = evaluate(&package, record)?;
        if decision.kind_name() != *expected_kind {
            return Err(format!(
                "{label}: expected outcome {expected_kind}, got {}",
                decision.kind_name()
            ));
        }
        if !decision.rule.ends_with(expected_rule) {
            return Err(format!(
                "{label}: expected rule {expected_rule}, got {}",
                decision.rule
            ));
        }
        if decision.reason_codes.is_empty() {
            return Err(format!("{label}: outcome carries no reason code"));
        }
    }
    Ok(())
}

/// Proves priority, not source order, selects the winning rule.
///
/// `missing ticket` and `understaffed production` both hold for the same record and both deny. The
/// higher-priority rule must win, so a package that happened to list them in the other order would
/// be observably wrong rather than accidentally right.
fn priority_resolves_overlapping_rules(root: &Path) -> Result<(), String> {
    let package = load_package(root, LockMode::Frozen)?;
    // No change ticket (priority 1000) and one reviewer in production (priority 700) both deny.
    let record = host_record("production", Some("main"), None, Some("revert-1"), 1);
    let decision = evaluate(&package, &record)?;
    if decision.kind != OutcomeKind::Deny {
        return Err(format!("expected deny, got {}", decision.kind_name()));
    }
    if !decision.rule.ends_with("deny-missing-change-ticket") {
        return Err(format!(
            "priority 1000 must beat priority 700, got {}",
            decision.rule
        ));
    }
    if decision.reason_codes != ["missing-change-ticket"] {
        return Err(format!(
            "expected only the winning reason, got {:?}",
            decision.reason_codes
        ));
    }
    Ok(())
}

/// Proves a missing optional fact escalates rather than denying.
///
/// Four-valued truth is the reason to embed this engine at all: an absent fact is `unknown`, not
/// `false`. A consumer that flattens absence into denial cannot distinguish "we know this is
/// unacceptable" from "we do not know", and the escalation outcome is what carries that difference.
fn absent_source_escalates_instead_of_denying(root: &Path) -> Result<(), String> {
    let package = load_package(root, LockMode::Frozen)?;
    let decision = evaluate(
        &package,
        &host_record("staging", None, Some("MBX-1"), Some("revert-1"), 1),
    )?;
    if decision.kind != OutcomeKind::Escalate {
        return Err(format!("expected escalate, got {}", decision.kind_name()));
    }
    if !decision
        .reason_codes
        .contains(&"unknown-source-branch".to_owned())
    {
        return Err(format!(
            "expected the unknown-source reason, got {:?}",
            decision.reason_codes
        ));
    }
    Ok(())
}

/// Proves an information request names the facts the host must supply.
///
/// The required-fact list is part of the contract, not decoration: a host cannot satisfy a request
/// it cannot read, so the fixture asserts the authored path comes back verbatim.
fn absent_rollback_plan_requests_information(root: &Path) -> Result<(), String> {
    let package = load_package(root, LockMode::Frozen)?;
    let decision = evaluate(
        &package,
        &host_record("staging", Some("main"), Some("MBX-1"), None, 1),
    )?;
    if decision.kind != OutcomeKind::RequestInformation {
        return Err(format!(
            "expected request_information, got {}",
            decision.kind_name()
        ));
    }
    if decision.required_facts != ["release.rollback-plan"] {
        return Err(format!(
            "expected the authored required fact, got {:?}",
            decision.required_facts
        ));
    }
    Ok(())
}

/// Proves a record no rule authorizes reaches the authored default outcome.
///
/// Defaults are where an incomplete policy silently becomes permissive or silently becomes
/// unusable. The authored default here is `escalate`, so an unmatched record must be visible rather
/// than approved.
fn unmatched_cases_fall_back_to_the_default_outcome(root: &Path) -> Result<(), String> {
    let package = load_package(root, LockMode::Frozen)?;
    // Every gate is present except reviewer count, so `approve-promotion` does not hold.
    let decision = evaluate(
        &package,
        &host_record("staging", Some("main"), Some("MBX-1"), Some("revert-1"), 0),
    )?;
    if decision.kind != OutcomeKind::Escalate {
        return Err(format!("expected escalate, got {}", decision.kind_name()));
    }
    if !decision
        .reason_codes
        .contains(&"no-authorizing-rule".to_owned())
    {
        return Err(format!(
            "expected the default reason, got {:?}",
            decision.reason_codes
        ));
    }
    Ok(())
}

/// Proves one package, one instant, and one fact set always produce one trace hash.
///
/// A trace hash is only useful for audit if it is stable. Two evaluations in the same process, and
/// then a third through a separately compiled package, must agree exactly.
fn evaluation_is_reproducible_for_one_instant(root: &Path) -> Result<(), String> {
    let record = host_record(
        "production",
        Some("main"),
        Some("MBX-1"),
        Some("revert-1"),
        2,
    );
    let first = evaluate(&load_package(root, LockMode::Frozen)?, &record)?;
    let second = evaluate(&load_package(root, LockMode::Frozen)?, &record)?;
    if first.trace_hash != second.trace_hash {
        return Err(format!(
            "trace hash is not reproducible: {} then {}",
            first.trace_hash, second.trace_hash
        ));
    }
    if !first.trace_hash.starts_with("blake3:") {
        return Err(format!(
            "trace hash is not a content hash: {}",
            first.trace_hash
        ));
    }
    Ok(())
}

/// Proves `Frozen` and `Update` assembly agree on the compiled package.
///
/// A consumer assembles in `Update` mode at startup and `Frozen` in CI. If the two modes lowered
/// different packages, the policy a consumer tested would not be the policy it shipped.
fn evaluation_is_stable_across_lock_modes(root: &Path) -> Result<(), String> {
    let record = host_record("staging", Some("main"), Some("MBX-1"), None, 1);
    let frozen = evaluate(&load_package(root, LockMode::Frozen)?, &record)?;
    let updated = evaluate(&load_package(root, LockMode::Update)?, &record)?;
    if frozen != updated {
        return Err(format!(
            "lock modes disagree: frozen {frozen:?} versus update {updated:?}"
        ));
    }
    Ok(())
}

// ── Host-shaped input ────────────────────────────────────────────────────────

/// Builds one host record in the shape a consumer's own domain type serializes to.
///
/// Returned as JSON rather than a Rust struct so the fixture asserts the same conversion path a
/// consumer uses when the host already speaks JSON.
fn host_record(
    environment: &str,
    branch: Option<&str>,
    ticket: Option<&str>,
    rollback: Option<&str>,
    reviewers: i64,
) -> JsonValue {
    let mut record = serde_json::Map::new();
    record.insert(
        "target-environment".to_owned(),
        JsonValue::from(environment),
    );
    if let Some(branch) = branch {
        record.insert("source-branch".to_owned(), JsonValue::from(branch));
    }
    if let Some(ticket) = ticket {
        record.insert("change-ticket".to_owned(), JsonValue::from(ticket));
    }
    if let Some(rollback) = rollback {
        record.insert("rollback-plan".to_owned(), JsonValue::from(rollback));
    }
    record.insert("reviewers".to_owned(), JsonValue::from(reviewers));
    JsonValue::Object(record)
}

// ── Package loading ──────────────────────────────────────────────────────────

/// Assembles and compiles the package at `root` under one lock mode.
fn load_package(root: &Path, mode: LockMode) -> Result<CompiledPackage, String> {
    let path = PackagePath::new(root.to_path_buf()).map_err(|error| error.to_string())?;
    let store = FilesystemPackageStore::default();
    let loaded = store.load_lock(&path).map_err(|error| error.to_string())?;
    let assembly = PackageAssembler::new(store, YamlSourceParser)
        .assemble(&path, mode)
        .map_err(|error| error.to_string())?;
    // `Frozen` assembly reports no proposed lock, so the on-disk lock is the only hash source there;
    // `Update` proposes one. Preferring the on-disk lock keeps both modes on the same input.
    let lock = loaded.ok_or_else(|| format!("{} has no rulery.lock", root.display()))?;
    let lock_hash = ContentHash::digest(
        &serde_json::to_vec(&RulebookLockEnvelope::from(lock))
            .map_err(|error| error.to_string())?,
    );
    let output = PolicyCompiler.compile_source(&SourceCompilationInput {
        source_bundle_hash: ContentHash::from_bytes(*assembly.input.integrity.root.as_bytes()),
        root: assembly.input.root,
        imports: assembly.input.imports,
        source_map: assembly.input.source_map,
        lock_hash: Some(lock_hash),
    });
    output.package.ok_or_else(|| {
        format!(
            "compilation failed: {}",
            output
                .diagnostics
                .iter()
                .map(|diagnostic| format!("{} {}", diagnostic.code, diagnostic.message))
                .collect::<Vec<_>>()
                .join("; ")
        )
    })
}

// ── Evaluation ───────────────────────────────────────────────────────────────

/// Evaluates the authored decision for one host record and reads the result back from the trace.
fn evaluate(package: &CompiledPackage, record: &JsonValue) -> Result<Decision, String> {
    let facts = facts_from_host(record, package.vocabulary())?;
    let instant = UtcInstant::new(CANONICAL_INSTANT).map_err(|error| error.to_string())?;
    let database = JiffTimeZoneDatabase::new(DATABASE_IDENTITY);
    let trace = evaluate_with(package, &facts, instant, &database)?;
    let outcome = trace
        .outcome
        .as_ref()
        .ok_or_else(|| "evaluated trace carries no outcome".to_owned())?;
    Ok(Decision {
        kind: outcome.kind(),
        rule: trace
            .determining_rules
            .first()
            .map_or_else(|| "<default>".to_owned(), ToString::to_string),
        trace_hash: trace.trace_hash.to_string(),
        reason_codes: outcome_reasons(outcome)
            .iter()
            .map(|reason| reason.code().as_str().to_owned())
            .collect(),
        required_facts: match outcome {
            Outcome::RequestInformation { required_facts, .. } => required_facts
                .paths()
                .iter()
                .map(ToString::to_string)
                .collect(),
            Outcome::Approve { .. } | Outcome::Deny { .. } | Outcome::Escalate { .. } => Vec::new(),
        },
    })
}

/// Evaluates the authored decision with an explicit clock and timezone port.
fn evaluate_with(
    package: &CompiledPackage,
    facts: &CaseFacts,
    instant: UtcInstant,
    database: &dyn TimeZoneDatabase,
) -> Result<rlry::engine::DecisionTraceV1, String> {
    Ok(ProductionPolicyEvaluator::new(database)
        .evaluate(
            package,
            &DecisionId::new(DECISION).map_err(|error| error.to_string())?,
            facts,
            instant,
            TraceDetail::Complete,
        )
        .map_err(|error| error.to_string())?
        .payload()
        .clone())
}

fn outcome_reasons(outcome: &Outcome) -> Vec<Reason> {
    match outcome {
        Outcome::Approve { reasons, .. }
        | Outcome::Deny { reasons, .. }
        | Outcome::Escalate { reasons, .. }
        | Outcome::RequestInformation { reasons, .. } => reasons.iter().cloned().collect(),
    }
}

// ── The adapter ──────────────────────────────────────────────────────────────

/// Converts one host record into typed case facts under the compiled vocabulary.
///
/// Walks the declared type rather than the host's field names, so an undeclared host field is a
/// hard error instead of a silently dropped constraint. Absent host fields are omitted rather than
/// sent as null, because absence is the fact the engine distinguishes.
fn facts_from_host(
    record: &JsonValue,
    vocabulary: &ResolvedVocabulary,
) -> Result<CaseFacts, String> {
    let type_id = vocabulary
        .roots
        .values()
        .find(|root| root.path.to_string() == ROOT)
        .map(|root| root.type_id.clone())
        .ok_or_else(|| format!("`{ROOT}` is not a declared fact root"))?;
    let value = json_value(record, &type_id, vocabulary, ROOT)?;
    let mut facts = BTreeMap::new();
    facts.insert(FactRootId::new(ROOT).map_err(|e| e.to_string())?, value);
    Ok(CaseFacts::new(facts))
}

/// Converts one host JSON node into the typed value its declared type names.
fn json_value(
    node: &JsonValue,
    type_id: &TypeId,
    vocabulary: &ResolvedVocabulary,
    path: &str,
) -> Result<Value, String> {
    let Some(resolved) = vocabulary.types.get(type_id) else {
        return scalar_value(node, type_id, path);
    };
    match &resolved.declaration {
        TypeDeclaration::Alias { target, .. } => json_value(node, target, vocabulary, path),
        TypeDeclaration::Enum { id, variants } => {
            let symbol = node
                .as_str()
                .ok_or_else(|| format!("`{path}` must be an enum symbol"))?;
            if !variants
                .iter()
                .any(|variant| variant.symbol.as_str() == symbol)
            {
                return Err(format!("`{symbol}` is not a variant of `{id}`"));
            }
            Ok(Value::Enum(EnumValue::new(
                id.clone(),
                StableId::new(symbol).map_err(|error| error.to_string())?,
            )))
        }
        TypeDeclaration::Record { fields, .. } => {
            let JsonValue::Object(entries) = node else {
                return Err(format!("`{path}` must be an object"));
            };
            let mut record = BTreeMap::new();
            for (name, value) in entries {
                let field = StableId::new(name).map_err(|error| error.to_string())?;
                let Some(declaration) = fields.get(&field) else {
                    return Err(format!("`{path}.{name}` is not a declared field"));
                };
                record.insert(
                    field,
                    json_value(
                        value,
                        &declaration.type_id,
                        vocabulary,
                        &format!("{path}.{name}"),
                    )?,
                );
            }
            Ok(Value::Record(record))
        }
        TypeDeclaration::List { element, .. } => {
            let JsonValue::Array(items) = node else {
                return Err(format!("`{path}` must be an array"));
            };
            items
                .iter()
                .map(|item| json_value(item, element, vocabulary, path))
                .collect::<Result<Vec<_>, _>>()
                .map(Value::List)
        }
        TypeDeclaration::Primitive => Err(format!("`{type_id}` declares no value kind")),
    }
}

/// Converts one host scalar into the primitive value its built-in type identity names.
fn scalar_value(node: &JsonValue, type_id: &TypeId, path: &str) -> Result<Value, String> {
    let text = || {
        node.as_str()
            .map(ToOwned::to_owned)
            .or_else(|| Some(node.to_string()))
            .ok_or_else(|| format!("`{path}` must be a scalar"))
    };
    Ok(match type_id.as_str() {
        "bool" | "boolean" => Value::Boolean(
            node.as_bool()
                .ok_or_else(|| format!("`{path}` must be a boolean"))?,
        ),
        "int" | "integer" => Value::Integer(
            text()?
                .parse()
                .map_err(|_| format!("`{path}` is not an integer"))?,
        ),
        "decimal" => Value::Decimal(DecimalValue::parse(text()?).map_err(|e| e.to_string())?),
        "string" | "text" => Value::Text(text()?),
        "date" => Value::Date(PolicyDate::parse(text()?).map_err(|e| e.to_string())?),
        "datetime" | "date-time" => {
            Value::DateTime(UtcInstant::parse(&text()?).map_err(|e| e.to_string())?)
        }
        "duration" => Value::Duration(DurationValue::parse(&text()?).map_err(|e| e.to_string())?),
        other => return Err(format!("`{other}` is not a primitive built-in")),
    })
}

// ── Optional macro surface ───────────────────────────────────────────────────

/// Proves the procedural derive also works from a renamed, out-of-tree dependency.
///
/// Compiled only under the `macros` feature, matching the facade, so the gate runs the evaluator
/// path both with and without the optional procedural crate present. The derived record is fed to
/// the same evaluator the manual adapter uses, so a derive that expands but produces facts the
/// engine rejects is caught here rather than by a downstream consumer.
#[cfg(feature = "macros")]
mod procedural {
    use rlry::macros::RuleFacts;

    /// A host record whose facts are derived rather than hand-built.
    #[derive(RuleFacts)]
    #[rulery(root = "release")]
    pub struct ReleaseFacts {
        /// Reviewer count the host recorded.
        #[rulery(path = "reviewers")]
        pub reviewers: i64,
    }

    /// Converts the derived record, proving the generated code is usable and not only resolvable.
    pub fn derived_facts() -> rlry::contracts::CaseFacts {
        rlry::contracts::CaseFacts::try_from(ReleaseFacts { reviewers: 2 }).expect("conversion")
    }
}

/// Evaluates the derived record, proving the macro path reaches the same engine.
///
/// The derived record is deliberately partial: the closed vocabulary accepts it because undeclared
/// fields are absent rather than null. The engine then applies the same precedence the manual
/// adapter sees, denying on the highest-priority rule whose condition holds. Asserting the deny
/// rather than an approval is the point: a derive that produced facts the engine interpreted
/// differently from hand-built facts would be indistinguishable to a consumer until it shipped.
#[cfg(feature = "macros")]
fn derived_facts_are_accepted_by_the_evaluator(root: &Path) -> Result<(), String> {
    use rlry::contracts::FactState;

    use crate::procedural::derived_facts;

    let package = load_package(root, LockMode::Frozen)?;
    let facts = derived_facts();
    match facts.root(&FactRootId::new(ROOT).map_err(|error| error.to_string())?) {
        FactState::Valid(Value::Record(_)) => {}
        _ => return Err("derived root is not a valid record".to_owned()),
    }
    let instant = UtcInstant::new(CANONICAL_INSTANT).map_err(|error| error.to_string())?;
    let database = JiffTimeZoneDatabase::new(DATABASE_IDENTITY);
    let trace = evaluate_with(&package, &facts, instant, &database)?;
    let outcome = trace
        .outcome
        .as_ref()
        .ok_or_else(|| "derived facts produced no outcome".to_owned())?;
    let reasons = outcome_reasons(outcome);
    let codes: Vec<&str> = reasons
        .iter()
        .map(|reason| reason.code().as_str())
        .collect();
    if outcome.kind() != OutcomeKind::Deny || codes != ["missing-change-ticket"] {
        return Err(format!(
            "derived facts produced {:?} with reasons {codes:?}, where deny on the missing \
             ticket was expected",
            outcome.kind()
        ));
    }
    Ok(())
}
