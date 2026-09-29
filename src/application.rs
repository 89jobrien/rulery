//! Production application services.
//!
//! This module closes the composition gaps between the workspace adapters: it lowers an assembled
//! package into compiler input, bridges authored scenario sources into the scenario crate's typed
//! inputs, and drives evaluation for the scenario runner.
//!
//! Bounded static analysis and semantic diff are derived from the compiled package by
//! [`rulery_analysis::PackageAnalyzer`], which owns partition construction, cell evaluation,
//! budget accounting, and report assembly. The application supplies its existing time-zone database
//! and the fixed analysis instant and delegates; it derives no analysis semantics.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

use rulery_analysis::{
    AnalysisOptions, AnalysisReport, PackageAnalyzer, PolicyAnalyzer, build_case_facts,
};
use rulery_compiler::{PolicyCompiler, SourceCompilationInput};
use rulery_contracts::{
    CaseFacts, ContentHash, DecimalValue, DecisionId, DurationValue, EnumValue, FactPath, Outcome,
    OutcomeKind, PackagePath, PolicyDate, ReasonCode, RulebookLock, RulebookLockEnvelope,
    ScenarioId, StableId, TypeId, UtcInstant, Value,
};
use rulery_engine::{
    DecisionTrace, DecisionTraceV1, JiffTimeZoneDatabase, PolicyEvaluationError, PolicyEvaluator,
    ProductionPolicyEvaluator, TimeZoneDatabase, TraceDetail,
};
use rulery_ir::CompiledPackage;
use rulery_scenarios::{
    ActualDecision, CompiledScenario, EvaluationResult, ScenarioCompiler, ScenarioEvaluator,
    ScenarioResult, ScenarioRunner, ScenarioSource, SourceExpectedDecision,
};
use rulery_store::{FilesystemPackageStore, PackageStore};
use rulery_syntax::{SourceOperand, SourceScenario, SourceValue, YamlSourceParser};
use rulery_vocabulary::{ResolvedType, ResolvedVocabulary, TypeDeclaration};
use thiserror::Error;

use crate::{
    AssemblyError, CompileWorkflowOutput, LockMode, PackageAssembler, PackageAssemblyService,
};

/// Application boundary failure.
#[derive(Debug, Error)]
pub enum ApplicationError {
    /// Invocation arguments cannot produce a runnable request.
    #[error("invalid invocation: {0}")]
    InvalidInvocation(String),
    /// Source loading, parsing, import traversal, or lock policy failed.
    #[error(transparent)]
    Assembly(#[from] AssemblyError),
    /// Compilation rejected the package.
    #[error("compilation produced {count} diagnostic(s)", count = .diagnostics.len())]
    Compilation {
        /// Deterministically ordered compiler diagnostics.
        diagnostics: Vec<String>,
    },
    /// One authored scenario could not be bridged into typed scenario input.
    #[error("scenario `{scenario}`: {message}")]
    Scenario {
        /// Authored scenario identity.
        scenario: StableId,
        /// Conversion failure detail.
        message: String,
    },
    /// Decision evaluation failed for a reason other than rejected facts.
    #[error("evaluation failed: {0}")]
    Evaluation(String),
    /// Relevant malformed facts were rejected by compiled semantics.
    #[error("case facts are invalid for the compiled package")]
    InvalidFacts,
    /// Runtime candidate conflict with boundary evidence.
    #[error("RUL205 runtime conflict")]
    RuntimeConflict {
        /// Conflicting rule or outcome evidence.
        evidence: Vec<String>,
    },
    /// Filesystem or stream I/O failed.
    #[error("{0}")]
    Io(String),
    /// Internal invariant failed.
    #[error("{0}")]
    Internal(String),
}

/// Stateless application use-case boundary.
pub trait ApplicationService: Send + Sync {
    /// Assembles and compiles one package, then compiles its root scenarios.
    ///
    /// # Errors
    ///
    /// Returns [`ApplicationError`] when assembly, compilation, or scenario bridging fails.
    fn compile_package(
        &self,
        root: &PackagePath,
        lock_mode: LockMode,
    ) -> Result<CompileWorkflowOutput, ApplicationError>;

    /// Evaluates one decision at an explicit instant without reopening sources.
    ///
    /// # Errors
    ///
    /// Returns [`ApplicationError`] when the decision is unknown, relevant facts are rejected, or
    /// a runtime conflict occurs.
    fn evaluate_at(
        &self,
        package: &CompiledPackage,
        decision: &DecisionId,
        facts: &CaseFacts,
        at: UtcInstant,
    ) -> Result<DecisionTrace, ApplicationError>;

    /// Runs bounded static analysis over a compiled package.
    ///
    /// # Errors
    ///
    /// Returns [`ApplicationError`] when the analyzer cannot be constructed for the supplied
    /// package.
    fn analyze(
        &self,
        package: &CompiledPackage,
        options: &AnalysisOptions,
        at: UtcInstant,
    ) -> Result<AnalysisReport, ApplicationError>;

    /// Compares two compiled packages semantically.
    ///
    /// # Errors
    ///
    /// Returns [`ApplicationError`] when the analyzer cannot be constructed for the supplied
    /// packages.
    fn diff(
        &self,
        before: &CompiledPackage,
        after: &CompiledPackage,
        decision: Option<&DecisionId>,
        options: &AnalysisOptions,
        at: UtcInstant,
    ) -> Result<AnalysisReport, ApplicationError>;

    /// Executes compiled scenarios against a package, returning one result per scenario.
    ///
    /// # Errors
    ///
    /// Returns [`ApplicationError`] when a scenario cannot be evaluated.
    fn run_scenarios(
        &self,
        package: &CompiledPackage,
        scenarios: &[CompiledScenario],
    ) -> Result<Vec<ScenarioResult>, ApplicationError>;

    /// Atomically writes a validated replacement lock.
    ///
    /// # Errors
    ///
    /// Returns [`ApplicationError::Io`] when the temporary write, sync, or rename fails.
    fn write_lock(&self, root: &PackagePath, lock: &RulebookLock) -> Result<(), ApplicationError>;
}

/// Production application composing the real workspace adapters.
#[derive(Clone)]
pub struct ProductionApplication {
    store: FilesystemPackageStore,
    time_zones: Arc<dyn TimeZoneDatabase>,
    compiler: PolicyCompiler,
    scenarios: ScenarioCompiler,
}

impl fmt::Debug for ProductionApplication {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProductionApplication")
            .field("store", &self.store)
            .field("time_zones", &self.time_zones.identity())
            .field("compiler", &self.compiler)
            .field("scenarios", &self.scenarios)
            .finish()
    }
}

impl Default for ProductionApplication {
    fn default() -> Self {
        Self {
            store: FilesystemPackageStore::default(),
            time_zones: Arc::new(JiffTimeZoneDatabase::default()),
            compiler: PolicyCompiler,
            scenarios: ScenarioCompiler,
        }
    }
}

impl ProductionApplication {
    /// Creates the production application with default resource and time-zone limits.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates the production application with a caller-supplied time-zone database.
    ///
    /// Every witness records the database identity that produced it, so an injected database
    /// changes the report rather than only the internal adapter.
    #[must_use]
    pub fn with_time_zones(time_zones: Arc<dyn TimeZoneDatabase>) -> Self {
        Self {
            time_zones,
            ..Self::default()
        }
    }

    /// Creates the analyzer used by the analysis and diff operations.
    fn analyzer(&self, at: UtcInstant) -> PackageAnalyzer<'_> {
        PackageAnalyzer::new(&*self.time_zones, at)
    }
}

impl ApplicationService for ProductionApplication {
    fn compile_package(
        &self,
        root: &PackagePath,
        lock_mode: LockMode,
    ) -> Result<CompileWorkflowOutput, ApplicationError> {
        let assembly = PackageAssembler::new(self.store.clone(), YamlSourceParser)
            .assemble(root, lock_mode)?;
        let lock_hash = self
            .store
            .load_lock(root)
            .map_err(AssemblyError::from)?
            .as_ref()
            .map(hash_lock)
            .transpose()?;

        let output = self.compiler.compile_source(&SourceCompilationInput {
            source_bundle_hash: ContentHash::from_bytes(*assembly.input.integrity.root.as_bytes()),
            root: assembly.input.root,
            imports: assembly.input.imports,
            source_map: assembly.input.source_map,
            lock_hash,
        });
        let Some(package) = output.package else {
            return Err(ApplicationError::Compilation {
                diagnostics: output
                    .diagnostics
                    .iter()
                    .map(|diagnostic| diagnostic.message.clone())
                    .collect(),
            });
        };

        let sources = assembly
            .source_scenarios
            .iter()
            .map(|source| scenario_source(source, package.vocabulary()))
            .collect::<Result<Vec<_>, _>>()?;
        let compiled = self.scenarios.compile(&package, sources);
        Ok(CompileWorkflowOutput {
            package: Some(package),
            scenarios: compiled.scenarios,
            diagnostics: compiled
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.message.clone())
                .collect(),
            proposed_lock: assembly.proposed_lock,
        })
    }

    fn evaluate_at(
        &self,
        package: &CompiledPackage,
        decision: &DecisionId,
        facts: &CaseFacts,
        at: UtcInstant,
    ) -> Result<DecisionTrace, ApplicationError> {
        map_evaluation(ProductionPolicyEvaluator::new(&*self.time_zones).evaluate(
            package,
            decision,
            facts,
            at,
            TraceDetail::Complete,
        ))
    }

    fn analyze(
        &self,
        package: &CompiledPackage,
        options: &AnalysisOptions,
        at: UtcInstant,
    ) -> Result<AnalysisReport, ApplicationError> {
        Ok(self.analyzer(at).analyze(package, options))
    }

    fn diff(
        &self,
        before: &CompiledPackage,
        after: &CompiledPackage,
        decision: Option<&DecisionId>,
        options: &AnalysisOptions,
        at: UtcInstant,
    ) -> Result<AnalysisReport, ApplicationError> {
        Ok(self.analyzer(at).diff(before, after, decision, options))
    }

    fn run_scenarios(
        &self,
        package: &CompiledPackage,
        scenarios: &[CompiledScenario],
    ) -> Result<Vec<ScenarioResult>, ApplicationError> {
        let runner = ScenarioRunner::new(ProductionScenarioEvaluator {
            package,
            time_zones: &*self.time_zones,
        });
        let package_hash = package.payload().package_hash();
        Ok(scenarios
            .iter()
            .map(|scenario| runner.run(package_hash, scenario))
            .collect())
    }

    fn write_lock(&self, root: &PackagePath, lock: &RulebookLock) -> Result<(), ApplicationError> {
        self.store
            .write_lock(root, lock)
            .map_err(|error| ApplicationError::Io(error.to_string()))
    }
}

/// Scenario evaluation bound to one package and time-zone database.
struct ProductionScenarioEvaluator<'a> {
    package: &'a CompiledPackage,
    time_zones: &'a dyn TimeZoneDatabase,
}

impl ScenarioEvaluator for ProductionScenarioEvaluator<'_> {
    fn evaluate(
        &self,
        scenario: &CompiledScenario,
        at: UtcInstant,
    ) -> Result<EvaluationResult, String> {
        let facts = root_facts(&scenario.given)?;
        match map_evaluation(ProductionPolicyEvaluator::new(self.time_zones).evaluate(
            self.package,
            &scenario.decision,
            &facts,
            at,
            TraceDetail::Complete,
        )) {
            Ok(trace) => Ok(EvaluationResult::Decision(Box::new(actual_decision(
                trace.payload(),
            )?))),
            Err(ApplicationError::InvalidFacts) => Ok(EvaluationResult::Invalid),
            Err(error) => Err(error.to_string()),
        }
    }
}

/// Flattens one evaluated trace into a scenario comparison value.
fn actual_decision(trace: &DecisionTraceV1) -> Result<ActualDecision, String> {
    let outcome = trace
        .outcome
        .clone()
        .ok_or_else(|| "evaluated trace has no outcome".to_owned())?;
    let reason_codes = match &outcome {
        Outcome::Approve { reasons, .. }
        | Outcome::Deny { reasons, .. }
        | Outcome::Escalate { reasons, .. }
        | Outcome::RequestInformation { reasons, .. } => {
            reasons.iter().map(|reason| reason.code().clone()).collect()
        }
    };
    let required_facts = match &outcome {
        Outcome::RequestInformation { required_facts, .. } => required_facts.paths().clone(),
        Outcome::Approve { .. } | Outcome::Deny { .. } | Outcome::Escalate { .. } => {
            std::collections::BTreeSet::new()
        }
    };
    Ok(ActualDecision {
        outcome: outcome.kind(),
        determining_rules: trace.determining_rules.iter().cloned().collect(),
        required_facts,
        reason_codes,
        trace: trace.clone(),
    })
}

/// Maps an evaluation failure onto the application boundary.
fn map_evaluation<T>(result: Result<T, PolicyEvaluationError>) -> Result<T, ApplicationError> {
    result.map_err(|error| match error {
        PolicyEvaluationError::InvalidFact(_) => ApplicationError::InvalidFacts,
        error => ApplicationError::Evaluation(error.to_string()),
    })
}

/// Computes the canonical lock hash used as compiler integrity input.
fn hash_lock(lock: &RulebookLock) -> Result<ContentHash, ApplicationError> {
    let bytes = serde_json::to_vec(&RulebookLockEnvelope::from(lock.clone()))
        .map_err(|error| ApplicationError::Internal(error.to_string()))?;
    Ok(ContentHash::digest(&bytes))
}

/// Re-keys authored scenario facts into the root-keyed set the engine consumes.
///
/// The nesting itself is [`build_case_facts`], shared with partition evaluation, so a scenario and
/// an analysis witness produce byte-identical fact maps for the same authored paths.
fn root_facts(given: &BTreeMap<FactPath, Value>) -> Result<CaseFacts, String> {
    build_case_facts(given).map_err(|error| error.to_string())
}

/// Bridges one authored scenario into the scenario crate's typed input.
fn scenario_source(
    source: &SourceScenario,
    vocabulary: &ResolvedVocabulary,
) -> Result<ScenarioSource, ApplicationError> {
    let fail = |message: String| ApplicationError::Scenario {
        scenario: source.id.clone(),
        message,
    };
    let id = ScenarioId::new(source.id.as_str()).map_err(|error| fail(error.to_string()))?;
    let at = UtcInstant::parse_rfc3339(&source.at).map_err(|error| fail(error.to_string()))?;
    let given = source
        .given
        .iter()
        .map(|(name, operand)| {
            let path =
                FactPath::from_str(name.as_str()).map_err(|error| fail(error.to_string()))?;
            let value = literal(operand, &path, vocabulary).map_err(&fail)?;
            Ok((path, value))
        })
        .collect::<Result<BTreeMap<_, _>, ApplicationError>>()?;
    let reason_codes = source
        .expect
        .reason_codes
        .iter()
        .map(|code| ReasonCode::new(code.as_str()).map_err(|error| fail(error.to_string())))
        .collect::<Result<_, _>>()?;
    Ok(ScenarioSource {
        id,
        title: source.title.clone(),
        description: source.description.clone(),
        decision: source.decision.clone(),
        at,
        given,
        expect: SourceExpectedDecision {
            outcome: outcome_kind(&source.expect.outcome).map_err(&fail)?,
            determining_rules: source.expect.determining_rules.clone(),
            required_facts: source.expect.required_facts.clone(),
            reason_codes,
        },
        tags: source.tags.clone(),
        span: source.span,
        root_authored: true,
    })
}

/// Maps an authored outcome name onto its typed kind.
fn outcome_kind(value: &str) -> Result<OutcomeKind, String> {
    match value {
        "approve" => Ok(OutcomeKind::Approve),
        "deny" => Ok(OutcomeKind::Deny),
        "escalate" => Ok(OutcomeKind::Escalate),
        "request_information" => Ok(OutcomeKind::RequestInformation),
        other => Err(format!("unknown outcome `{other}`")),
    }
}

/// Resolves one authored operand to a typed value using the declared vocabulary type.
fn literal(
    operand: &SourceOperand,
    path: &FactPath,
    vocabulary: &ResolvedVocabulary,
) -> Result<Value, String> {
    let value = match operand {
        SourceOperand::Literal(value) => value,
        SourceOperand::Fact(reference) => {
            return Err(format!(
                "fact `{reference}` cannot be supplied as a literal"
            ));
        }
        SourceOperand::Reserved(value) => {
            return Err(format!("reserved operand `{value}` is not a value"));
        }
    };
    let Some(ty) = fact_type(path, vocabulary) else {
        return Err(format!("fact `{path}` is not declared in the vocabulary"));
    };
    decode(value, &ty, vocabulary, path)
}

/// Decodes one authored value against the type declared for it, recursing through records and
/// lists so a nested authored mapping becomes a nested record rather than flattened text.
fn decode(
    value: &SourceValue,
    ty: &ResolvedType,
    vocabulary: &ResolvedVocabulary,
    path: &FactPath,
) -> Result<Value, String> {
    match &ty.declaration {
        TypeDeclaration::Enum { id, .. } => {
            let SourceValue::Scalar(text) = value else {
                return Err(format!("enum fact `{path}` requires a scalar literal"));
            };
            let variant = StableId::new(text.as_str()).map_err(|error| error.to_string())?;
            Ok(Value::Enum(EnumValue::new(
                TypeId::new(id.as_str()).map_err(|e| e.to_string())?,
                variant,
            )))
        }
        TypeDeclaration::Record { id, fields, .. } => {
            let SourceValue::Mapping(entries) = value else {
                return Err(format!("record fact `{path}` requires a mapping literal"));
            };
            let mut record = BTreeMap::new();
            for (name, item) in entries {
                let Some(field) = fields.get(name) else {
                    return Err(format!(
                        "record fact `{path}` has no field `{name}` in `{id}`"
                    ));
                };
                let element = resolve(&field.type_id, vocabulary).ok_or_else(|| {
                    format!("field `{name}` of `{path}` has an unresolvable type")
                })?;
                let child = FactPath::from_str(&format!("{path}.{name}"))
                    .map_err(|error| error.to_string())?;
                record.insert(name.clone(), decode(item, &element, vocabulary, &child)?);
            }
            Ok(Value::Record(record))
        }
        TypeDeclaration::List { element, .. } => {
            let SourceValue::Sequence(items) = value else {
                return Err(format!("list fact `{path}` requires a sequence literal"));
            };
            let element = resolve(element, vocabulary)
                .ok_or_else(|| format!("list fact `{path}` has an unresolvable element type"))?;
            items
                .iter()
                .map(|item| decode(item, &element, vocabulary, path))
                .collect::<Result<Vec<_>, _>>()
                .map(Value::List)
        }
        TypeDeclaration::Alias { .. } => {
            Err(format!("fact `{path}` resolved to an unresolved alias"))
        }
        TypeDeclaration::Primitive => primitive(value, ty.id.as_str(), path),
    }
}

/// Decodes one authored scalar against a primitive built-in named by identity.
fn primitive(value: &SourceValue, name: &str, path: &FactPath) -> Result<Value, String> {
    if matches!(value, SourceValue::Null) {
        return Ok(Value::Null);
    }
    let SourceValue::Scalar(text) = value else {
        return Err(format!(
            "fact `{path}` of built-in `{name}` requires a scalar"
        ));
    };
    match name {
        "bool" | "boolean" => text
            .parse()
            .map(Value::Boolean)
            .map_err(|_| format!("`{text}` is not boolean")),
        "int" | "integer" => text
            .parse()
            .map(Value::Integer)
            .map_err(|_| format!("`{text}` is not integer")),
        "decimal" => DecimalValue::parse(text)
            .map(Value::Decimal)
            .map_err(|error| error.to_string()),
        "string" | "text" => Ok(Value::Text(text.to_owned())),
        "date" => PolicyDate::parse(text)
            .map(Value::Date)
            .map_err(|error| error.to_string()),
        "datetime" | "date-time" => UtcInstant::parse(text)
            .map(Value::DateTime)
            .map_err(|error| error.to_string()),
        "duration" => DurationValue::parse(text)
            .map(Value::Duration)
            .map_err(|error| error.to_string()),
        other => Err(format!("literal type `{other}` is not supported")),
    }
}

/// Resolves the declared type of one fact path through its root, record fields, and aliases.
fn fact_type(path: &FactPath, vocabulary: &ResolvedVocabulary) -> Option<ResolvedType> {
    let (root_path, root) = vocabulary
        .roots
        .iter()
        .find(|(root_path, _)| path.segments().starts_with(root_path.segments()))?;
    let mut type_id = root.type_id.clone();
    for segment in &path.segments()[root_path.len()..] {
        let TypeDeclaration::Record { fields, .. } = &resolve(&type_id, vocabulary)?.declaration
        else {
            return None;
        };
        type_id = fields
            .iter()
            .find(|(name, _)| name.as_str() == segment.as_str())?
            .1
            .type_id
            .clone();
    }
    resolve(&type_id, vocabulary)
}

/// Maximum alias hops followed before a type chain is treated as unresolvable.
const MAX_ALIAS_HOPS: usize = 16;

/// Primitive built-in identities, which are named by identity and never declared in the
/// vocabulary, so resolution must synthesize them rather than look them up.
const PRIMITIVE_BUILT_INS: &[&str] = &[
    "bool",
    "boolean",
    "int",
    "integer",
    "decimal",
    "string",
    "text",
    "date",
    "datetime",
    "date-time",
    "duration",
];

/// Returns the resolved type of a primitive built-in identity, or `None` for a declared name.
fn primitive_type(type_id: &TypeId) -> Option<ResolvedType> {
    PRIMITIVE_BUILT_INS
        .contains(&type_id.as_str())
        .then(|| ResolvedType {
            id: type_id.clone(),
            declaration: TypeDeclaration::Primitive,
        })
}

/// Follows alias declarations to the underlying type.
fn resolve(type_id: &TypeId, vocabulary: &ResolvedVocabulary) -> Option<ResolvedType> {
    let mut current = type_id.clone();
    for _ in 0..MAX_ALIAS_HOPS {
        let ty = vocabulary
            .types
            .get(&current)
            .cloned()
            .or_else(|| primitive_type(&current))?;
        match &ty.declaration {
            TypeDeclaration::Alias { target, .. } => current = target.clone(),
            _ => return Some(ty),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use rulery_analysis::{
        AnalysisCompleteness, FactPartitionValue, FiniteDomain, OutcomeChangeKind,
        OverlapClassification, ReachabilityStatus, UncoveredCategory, WitnessClaim,
        analysis_instant,
    };
    use rulery_contracts::{
        EscalationId, FactPath, FactRootId, LanguageVersion, PackageId, PolicyTimeZone,
        QualifiedRuleId, Reason, Reasons, RuleId, SourceFile, SourceId, SourceKey, SourceMap,
        SourcePath, Span, Version,
    };
    use rulery_engine::{TimeZoneDatabase, TimeZoneError};
    use rulery_ir::{
        CompilationInput, CompiledDecision, CompiledEffect, CompiledPackageDraft, CompiledRule,
        DecisionPrecedence, DecisionSemantics, ExpiryPolicy, Expr, ExprOperand, FieldDeclaration,
        FieldPresence, InvalidFactStrategy, MissingFactStrategy, Operator, PackageIntegritySet,
        Predicate, ResolvedRoot, ResolvedType, ResolvedVocabulary, TypeDeclaration,
    };
    use rulery_scenarios::{ExpectedDecision, ScenarioExpectationField, ScenarioStatus};

    use super::*;

    /// Test double whose only purpose is to be identifiable, so a caller can prove the
    /// application used the database it was given rather than the Jiff default.
    struct InjectedTimeZoneDatabase;

    impl TimeZoneDatabase for InjectedTimeZoneDatabase {
        fn identity(&self) -> &'static str {
            "test/injected-tzdb"
        }

        fn local_date(
            &self,
            instant: UtcInstant,
            zone: &PolicyTimeZone,
        ) -> Result<PolicyDate, TimeZoneError> {
            JiffTimeZoneDatabase::default().local_date(instant, zone)
        }
    }

    fn path(value: &str) -> FactPath {
        FactPath::from_str(value).expect("canonical path")
    }

    #[test]
    fn root_facts_nest_authored_paths_under_root_identities() {
        let facts = root_facts(&BTreeMap::from([
            (path("member.id"), Value::Text("m-1".to_owned())),
            (path("member.age"), Value::Integer(41)),
            (path("active"), Value::Boolean(true)),
        ]))
        .expect("root facts");

        let member = facts.root(&FactRootId::new("member").expect("root id"));
        let rulery_contracts::FactState::Valid(rulery_contracts::Value::Record(fields)) = member
        else {
            panic!("member root must be a record");
        };
        assert_eq!(fields.len(), 2);
        assert!(matches!(
            fields.get(&StableId::new("id").expect("id")),
            Some(Value::Text(value)) if value == "m-1"
        ));
        assert!(matches!(
            fields.get(&StableId::new("age").expect("age")),
            Some(Value::Integer(41))
        ));
        assert!(matches!(
            facts.root(&FactRootId::new("active").expect("root id")),
            rulery_contracts::FactState::Valid(Value::Boolean(true))
        ));
    }

    #[test]
    fn root_facts_reject_conflicting_root_and_child_paths() {
        let error = root_facts(&BTreeMap::from([
            (path("member"), Value::Text("root wins".to_owned())),
            (path("member.id"), Value::Text("child loses".to_owned())),
        ]))
        .expect_err("conflict");
        assert!(error.contains("conflicts"), "{error}");

        let error = root_facts(&BTreeMap::from([
            (path("member.id"), Value::Text("child".to_owned())),
            (path("member"), Value::Text("root".to_owned())),
        ]))
        .expect_err("conflict");
        assert!(error.contains("conflicts"), "{error}");
    }

    #[test]
    fn outcome_kind_rejects_unknown_names() {
        for name in ["approve", "deny", "escalate", "request_information"] {
            assert!(outcome_kind(name).is_ok(), "rejected {name}");
        }
        for name in ["Approve", "permit", ""] {
            assert!(outcome_kind(name).is_err(), "accepted {name}");
        }
    }

    #[allow(clippy::too_many_lines)]
    #[test]
    fn analyze_derives_reachability_interaction_and_coverage_findings() {
        let application = ProductionApplication::new();
        let package = analysis_package();
        let options = explicit_status_domain();

        let report = application
            .analyze(&package, &options, analysis_instant())
            .expect("analysis report")
            .payload()
            .clone();

        assert_eq!(report.package_hash, package.payload().package_hash());
        assert_eq!(
            report
                .reachability
                .iter()
                .map(|entry| (entry.rule.rule().as_str(), entry.status))
                .collect::<Vec<_>>(),
            vec![
                ("rule.allow", ReachabilityStatus::Reachable),
                ("rule.allow-copy", ReachabilityStatus::Reachable),
                ("rule.block", ReachabilityStatus::Reachable),
                ("rule.block-alt", ReachabilityStatus::Reachable),
                ("rule.note", ReachabilityStatus::Reachable),
                ("rule.unlisted", ReachabilityStatus::Unreachable),
            ]
        );
        let unreachable = report
            .reachability
            .iter()
            .find(|entry| entry.rule.rule().as_str() == "rule.unlisted")
            .expect("unreachable rule");
        assert!(unreachable.witness.is_none());
        assert_eq!(
            unreachable
                .proof
                .as_ref()
                .expect("unreachability proof")
                .method,
            "finite-partition-exhaustion"
        );

        assert_eq!(
            report
                .overlaps
                .iter()
                .map(|overlap| {
                    (
                        overlap.left.rule().as_str().to_owned(),
                        overlap.right.rule().as_str().to_owned(),
                        overlap.classification,
                    )
                })
                .collect::<Vec<_>>(),
            vec![
                (
                    "rule.allow".to_owned(),
                    "rule.allow-copy".to_owned(),
                    OverlapClassification::Redundant
                ),
                (
                    "rule.allow".to_owned(),
                    "rule.block".to_owned(),
                    OverlapClassification::ShadowedApproval
                ),
                (
                    "rule.allow".to_owned(),
                    "rule.block-alt".to_owned(),
                    OverlapClassification::ShadowedApproval
                ),
                (
                    "rule.allow-copy".to_owned(),
                    "rule.block".to_owned(),
                    OverlapClassification::ShadowedApproval
                ),
                (
                    "rule.allow-copy".to_owned(),
                    "rule.block-alt".to_owned(),
                    OverlapClassification::ShadowedApproval
                ),
                (
                    "rule.block".to_owned(),
                    "rule.block-alt".to_owned(),
                    OverlapClassification::Conflict
                ),
            ]
        );
        assert!(report.overlaps.iter().all(|overlap| {
            matches!(overlap.witness.claim, WitnessClaim::RuleOverlap(_, _))
                && overlap.witness.synthetic
        }));

        let codes = diagnostic_codes(&report);
        assert_eq!(
            codes,
            BTreeSet::from([
                "RUL200".to_owned(),
                "RUL201".to_owned(),
                "RUL202".to_owned(),
                "RUL203".to_owned(),
                "RUL250".to_owned(),
                "RUL252".to_owned(),
            ])
        );

        let access = report
            .coverage
            .iter()
            .find(|entry| entry.decision.as_str() == "decision.access")
            .expect("access coverage")
            .report
            .clone();
        assert_eq!(access.denominator, 3);
        assert_eq!(access.numerator, 0);
        assert_eq!(access.percent_basis_points, Some(0));
        assert_eq!(access.completeness, AnalysisCompleteness::Complete);
        assert_eq!(
            access
                .uncovered
                .iter()
                .map(|case| case.category)
                .collect::<Vec<_>>(),
            vec![
                UncoveredCategory::MissingFact,
                UncoveredCategory::DefaultOnly,
                UncoveredCategory::NoMatchingRule,
            ]
        );
        assert_eq!(
            access.diagnostics,
            BTreeSet::from(["RUL250".to_owned(), "RUL252".to_owned()])
        );
        assert!(access.uncovered.iter().all(|case| case.witness.synthetic));

        let notes = report
            .coverage
            .iter()
            .find(|entry| entry.decision.as_str() == "decision.notes")
            .expect("notes coverage")
            .report
            .clone();
        // `decision.notes` reads a text field only through presence predicates, so its value
        // space cannot change any outcome. It now reports a real coverage percentage instead of
        // refusing to claim completeness, and the only finding left is its own partial coverage.
        assert_eq!(notes.completeness, AnalysisCompleteness::Complete);
        assert_eq!(notes.percent_basis_points, Some(6666));
        assert_eq!(notes.diagnostics, BTreeSet::from(["RUL250".to_owned()]));
        assert_eq!(report.completeness, AnalysisCompleteness::Complete);

        let repeated = application
            .analyze(&package, &options, analysis_instant())
            .expect("analysis report");
        assert_eq!(
            serde_json::to_vec(repeated.payload()).expect("report json"),
            serde_json::to_vec(&report).expect("report json")
        );
    }

    #[test]
    fn analyze_honors_an_explicit_instant() {
        let application = ProductionApplication::new();
        let package = analysis_package();
        let at = UtcInstant::parse_rfc3339("2026-09-16T16:00:00.000000000Z").expect("instant");

        let report = application
            .analyze(&package, &AnalysisOptions::default(), at)
            .expect("analysis report")
            .payload()
            .clone();

        assert!(
            !report.witnesses.is_empty(),
            "analysis produced no witness to observe the instant on"
        );
        for witness in &report.witnesses {
            assert_eq!(witness.at, at, "witness recorded another instant");
        }
    }

    #[test]
    fn analyze_records_the_injected_database_identity() {
        let application =
            ProductionApplication::with_time_zones(Arc::new(InjectedTimeZoneDatabase));
        let package = analysis_package();
        let at = UtcInstant::parse_rfc3339("2026-09-16T16:00:00.000000000Z").expect("instant");

        let report = application
            .analyze(&package, &AnalysisOptions::default(), at)
            .expect("analysis report")
            .payload()
            .clone();

        assert!(
            !report.witnesses.is_empty(),
            "analysis produced no witness to observe the database on"
        );
        for witness in &report.witnesses {
            assert_eq!(
                witness.timezone_database.implementation(),
                "test",
                "witness recorded another database implementation"
            );
            assert_eq!(
                witness.timezone_database.version(),
                "injected-tzdb",
                "witness recorded another database version"
            );
        }
    }

    #[test]
    fn analyze_without_explicit_domains_reports_only_presence_states() {
        let application = ProductionApplication::new();
        let package = analysis_package();

        let report = application
            .analyze(&package, &AnalysisOptions::default(), analysis_instant())
            .expect("analysis report")
            .payload()
            .clone();

        let access = report
            .coverage
            .iter()
            .find(|entry| entry.decision.as_str() == "decision.access")
            .expect("access coverage")
            .report
            .clone();
        assert_eq!(access.denominator, 2);
        assert_eq!(access.numerator, 0);
        assert_eq!(access.percent_basis_points, None);
        assert_eq!(
            access
                .uncovered
                .iter()
                .map(|case| case.category)
                .collect::<Vec<_>>(),
            vec![
                UncoveredCategory::MissingFact,
                UncoveredCategory::DefaultOnly
            ]
        );
        assert_eq!(
            report
                .reachability
                .iter()
                .map(|entry| (entry.rule.rule().as_str(), entry.status))
                .collect::<Vec<_>>(),
            vec![
                ("rule.allow", ReachabilityStatus::Inconclusive),
                ("rule.allow-copy", ReachabilityStatus::Inconclusive),
                ("rule.block", ReachabilityStatus::Inconclusive),
                ("rule.block-alt", ReachabilityStatus::Inconclusive),
                ("rule.note", ReachabilityStatus::Reachable),
                ("rule.unlisted", ReachabilityStatus::Inconclusive),
            ]
        );
        assert!(report.overlaps.is_empty());
        assert!(matches!(
            report.completeness,
            AnalysisCompleteness::Inconclusive { .. }
        ));
    }

    #[test]
    fn diff_derives_outcome_changes_between_compiled_packages() {
        let application = ProductionApplication::new();
        let before = diff_package(false);
        let after = diff_package(true);
        let options = explicit_active_domain();

        let report = application
            .diff(&before, &after, None, &options, analysis_instant())
            .expect("diff report")
            .payload()
            .clone();

        assert_eq!(report.semantic_diffs.len(), 1);
        let semantic = &report.semantic_diffs[0];
        assert_eq!(semantic.decision.as_str(), "decision.access");
        assert_eq!(semantic.completeness, AnalysisCompleteness::Complete);
        assert_eq!(semantic.unchanged, Some(false));
        assert_eq!(semantic.changes.len(), 1);
        let change = &semantic.changes[0];
        assert_eq!(change.before.kind(), OutcomeKind::Approve);
        assert_eq!(change.after.kind(), OutcomeKind::Deny);
        assert_eq!(change.classification, OutcomeChangeKind::MoreRestrictive);
        assert_eq!(change.diagnostic_code, "RUL351");
        assert!(matches!(change.witness.claim, WitnessClaim::BehaviorChange));
        assert!(change.witness.synthetic);
        assert_eq!(
            diagnostic_codes(&report),
            BTreeSet::from(["RUL351".to_owned()])
        );

        let unchanged = application
            .diff(&before, &before, None, &options, analysis_instant())
            .expect("diff report")
            .payload()
            .clone();
        assert_eq!(unchanged.semantic_diffs[0].unchanged, Some(true));
        assert!(unchanged.semantic_diffs[0].changes.is_empty());
    }

    /// Reads the diagnostic codes of a report through its wire form.
    ///
    /// The report keeps its diagnostic report as an opaque versioned payload, so the codes are read
    /// from the same bytes a consumer would receive.
    fn diagnostic_codes(report: &rulery_analysis::AnalysisReportV1) -> BTreeSet<String> {
        let value = serde_json::to_value(report).expect("report json");
        value["diagnostics"]["diagnostics"]
            .as_array()
            .expect("diagnostics array")
            .iter()
            .map(|entry| entry["code"].as_str().expect("diagnostic code").to_owned())
            .collect()
    }

    #[allow(clippy::too_many_lines)]
    fn analysis_package() -> CompiledPackage {
        let (source_map, span) = sample_source_map();
        build_package(
            vec![analysis_decision(span), notes_decision(span)],
            source_map,
            analysis_vocabulary(),
        )
    }

    #[allow(clippy::too_many_lines)]
    fn analysis_decision(span: Span) -> CompiledDecision {
        CompiledDecision::new(
            decision("decision.access"),
            decision_semantics(
                MissingFactStrategy::PreserveUnknown,
                InvalidFactStrategy::PreserveInvalid,
            ),
            deny_effect("no-access-rule", "No access rule matched this member."),
            vec![
                compiled_rule(
                    "rule.allow",
                    50,
                    status_contains("active", span),
                    approve_effect("member-active", "An active member is allowed."),
                    3,
                    span,
                ),
                compiled_rule(
                    "rule.allow-copy",
                    50,
                    status_contains("active", span),
                    approve_effect("member-active", "An active member is allowed."),
                    3,
                    span,
                ),
                compiled_rule(
                    "rule.block",
                    100,
                    status_contains("blocked", span),
                    deny_effect("member-blocked", "A blocked member is denied."),
                    3,
                    span,
                ),
                compiled_rule(
                    "rule.block-alt",
                    100,
                    status_contains("blocked", span),
                    deny_effect("member-blocked-alt", "A blocked member is denied outright."),
                    3,
                    span,
                ),
                compiled_rule(
                    "rule.unlisted",
                    50,
                    status_contains("unlisted", span),
                    deny_effect("never", "This status is never supplied."),
                    3,
                    span,
                ),
            ],
            span,
        )
        .expect("analysis decision")
    }

    fn notes_decision(span: Span) -> CompiledDecision {
        CompiledDecision::new(
            decision("decision.notes"),
            decision_semantics(
                MissingFactStrategy::PreserveUnknown,
                InvalidFactStrategy::PreserveInvalid,
            ),
            deny_effect("no-note-rule", "No note rule matched this member."),
            vec![compiled_rule(
                "rule.note",
                50,
                Expr::Predicate(Predicate {
                    operator: Operator::Exists,
                    left: ExprOperand::Fact(path("member.label")),
                    right: None,
                    span,
                }),
                approve_effect("member-noted", "A noted member is allowed."),
                1,
                span,
            )],
            span,
        )
        .expect("notes decision")
    }

    fn diff_package(with_block: bool) -> CompiledPackage {
        let (source_map, span) = sample_source_map();
        let mut rules = vec![compiled_rule(
            "rule.allow",
            50,
            status_contains("active", span),
            approve_effect("member-active", "An active member is allowed."),
            2,
            span,
        )];
        if with_block {
            rules.push(compiled_rule(
                "rule.block",
                100,
                status_contains("active", span),
                deny_effect("member-blocked", "An active member is denied."),
                2,
                span,
            ));
        }
        let decision = CompiledDecision::new(
            decision("decision.access"),
            decision_semantics(
                MissingFactStrategy::PreserveUnknown,
                InvalidFactStrategy::PreserveInvalid,
            ),
            deny_effect("no-access-rule", "No access rule matched this member."),
            rules,
            span,
        )
        .expect("diff decision");
        build_package(vec![decision], source_map, analysis_vocabulary())
    }

    fn explicit_status_domain() -> AnalysisOptions {
        let mut options = AnalysisOptions::default();
        options.explicit_domains.insert(
            path("member.status"),
            FiniteDomain::new(vec![FactPartitionValue::Valid(Value::List(vec![
                Value::Text("active".to_owned()),
                Value::Text("blocked".to_owned()),
            ]))]),
        );
        options
    }

    fn explicit_active_domain() -> AnalysisOptions {
        let mut options = AnalysisOptions::default();
        options.explicit_domains.insert(
            path("member.status"),
            FiniteDomain::new(vec![FactPartitionValue::Valid(Value::List(vec![
                Value::Text("active".to_owned()),
            ]))]),
        );
        options
    }

    fn analysis_vocabulary() -> ResolvedVocabulary {
        let member_type = TypeId::new("type.member").expect("type");
        let status_type = TypeId::new("type.status").expect("type");
        let text_type = TypeId::new("type.text").expect("type");
        ResolvedVocabulary {
            roots: BTreeMap::from([(
                path("member"),
                ResolvedRoot {
                    path: path("member"),
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
                                    StableId::new("label").expect("field"),
                                    FieldDeclaration {
                                        type_id: TypeId::new("string").expect("type"),
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
                    text_type.clone(),
                    ResolvedType {
                        id: text_type,
                        declaration: TypeDeclaration::Primitive,
                    },
                ),
            ]),
            terms: BTreeMap::new(),
        }
    }

    #[allow(clippy::too_many_lines)]
    #[test]
    fn evaluate_at_returns_trace_with_the_deciding_rule() {
        let application = ProductionApplication::new();
        let (package, _) = access_package();
        let facts = member_facts(&[(
            "status",
            Value::List(vec![Value::Text("active".to_owned())]),
        )]);

        let trace = application
            .evaluate_at(
                &package,
                &decision("decision.access"),
                &facts,
                evaluated_at(),
            )
            .expect("evaluated trace")
            .payload()
            .clone();

        assert_eq!(
            trace.outcome.as_ref().map(Outcome::kind),
            Some(OutcomeKind::Approve)
        );
        assert_eq!(outcome_codes(&trace), vec!["member-active"]);
        assert_eq!(trace.determining_rules, vec![qualified("rule.allow")]);
        assert!(trace.conflict.is_none());
        assert!(trace.superseded_rules.is_empty());
        assert_eq!(trace.package_hash, package.payload().package_hash());
        assert_eq!(trace.package, package.payload().package_id().clone());
        assert_eq!(trace.evaluated_at, evaluated_at());
        assert_eq!(trace.language_version, LanguageVersion::V1);
        assert_eq!(trace.compiler, "compiler");
        assert_eq!(trace.timezone_database.implementation(), "jiff");
        assert_eq!(trace.missing_facts, BTreeSet::from([path("member.age")]));
        assert!(trace.invalid_facts.is_empty());
        assert_eq!(trace.rule_traces.len(), 3);
        assert_eq!(
            trace
                .rule_traces
                .iter()
                .filter(|entry| entry.selected)
                .map(|entry| entry.rule.clone())
                .collect::<Vec<_>>(),
            vec![qualified("rule.allow")]
        );
        assert_eq!(
            trace
                .rule_traces
                .iter()
                .find(|entry| entry.rule == qualified("rule.review"))
                .map(|entry| entry.result),
            Some(rulery_engine::Truth::Unknown)
        );
    }

    #[test]
    fn evaluate_at_uses_the_default_outcome_without_a_determining_rule() {
        let application = ProductionApplication::new();
        let (package, _) = access_package();
        let facts = member_facts(&[(
            "status",
            Value::List(vec![Value::Text("unlisted".to_owned())]),
        )]);

        let trace = application
            .evaluate_at(
                &package,
                &decision("decision.access"),
                &facts,
                evaluated_at(),
            )
            .expect("evaluated trace")
            .payload()
            .clone();

        assert_eq!(
            trace.outcome.as_ref().map(Outcome::kind),
            Some(OutcomeKind::Deny)
        );
        assert_eq!(outcome_codes(&trace), vec!["no-matching-rule"]);
        assert!(trace.determining_rules.is_empty());
        assert!(trace.rule_traces.iter().all(|entry| !entry.selected));
        assert_eq!(trace.rule_traces.len(), 3);
    }

    #[test]
    fn evaluate_at_maps_unknown_decision_and_rejected_facts_to_errors() {
        let application = ProductionApplication::new();
        let (package, _) = access_package();

        let error = application
            .evaluate_at(
                &package,
                &decision("decision.absent"),
                &member_facts(&[]),
                evaluated_at(),
            )
            .expect_err("unknown decision");
        assert!(
            matches!(error, ApplicationError::Evaluation(ref message) if message.contains("unknown decision `decision.absent`")),
            "{error}"
        );

        let error = application
            .evaluate_at(
                &package,
                &decision("decision.strict"),
                &member_facts(&[("age", Value::Text("sixty".to_owned()))]),
                evaluated_at(),
            )
            .expect_err("rejected facts");
        assert!(matches!(error, ApplicationError::InvalidFacts), "{error}");
    }

    #[allow(clippy::too_many_lines)]
    #[test]
    fn run_scenarios_reports_passing_and_failing_expectations() {
        let application = ProductionApplication::new();
        let (package, span) = access_package();
        let given = BTreeMap::from([(
            path("member.status"),
            Value::List(vec![Value::Text("active".to_owned())]),
        )]);
        let passing = compiled_scenario(
            "scenario.active",
            "decision.access",
            given.clone(),
            expected_decision(OutcomeKind::Approve, &["rule.allow"], &["member-active"]),
            span,
        );
        let mut denied = compiled_scenario(
            "scenario.denied",
            "decision.access",
            given.clone(),
            expected_decision(OutcomeKind::Deny, &["rule.allow"], &["member-active"]),
            span,
        );
        denied
            .tags
            .insert(StableId::new("regression").expect("tag"));
        let misrouted = compiled_scenario(
            "scenario.misrouted",
            "decision.access",
            given,
            expected_decision(OutcomeKind::Deny, &["rule.block"], &["member-blocked"]),
            span,
        );

        let results = application
            .run_scenarios(
                &package,
                &[passing.clone(), denied.clone(), misrouted.clone()],
            )
            .expect("scenario results");
        assert_eq!(results.len(), 3);

        let passed = results[0].payload();
        assert_eq!(passed.scenario, passing.id);
        assert_eq!(passed.status, ScenarioStatus::Passed);
        assert!(passed.failures.is_empty());
        assert_eq!(passed.package_hash, package.payload().package_hash());
        let trace = passed.trace.as_ref().expect("decision trace");
        assert_eq!(
            trace.outcome.as_ref().map(Outcome::kind),
            Some(OutcomeKind::Approve)
        );
        assert_eq!(outcome_codes(trace), vec!["member-active"]);
        assert_eq!(trace.determining_rules, vec![qualified("rule.allow")]);

        let failed = results[1].payload();
        assert_eq!(failed.scenario, denied.id);
        assert_eq!(failed.status, ScenarioStatus::Failed);
        assert_eq!(
            failed
                .failures
                .iter()
                .map(|entry| entry.field)
                .collect::<Vec<_>>(),
            vec![ScenarioExpectationField::Outcome]
        );
        assert_eq!(failed.failures[0].expected, serde_json::json!("deny"));
        assert_eq!(failed.failures[0].actual, serde_json::json!("approve"));
        assert!(failed.trace.is_some());

        let misrouted_result = results[2].payload();
        assert_eq!(misrouted_result.scenario, misrouted.id);
        assert_eq!(misrouted_result.status, ScenarioStatus::Failed);
        assert_eq!(
            misrouted_result
                .failures
                .iter()
                .map(|entry| entry.field)
                .collect::<Vec<_>>(),
            vec![
                ScenarioExpectationField::Outcome,
                ScenarioExpectationField::DeterminingRules,
                ScenarioExpectationField::ReasonCodes,
            ]
        );
    }

    #[test]
    fn run_scenarios_marks_rejected_facts_as_invalid() {
        let application = ProductionApplication::new();
        let (package, span) = access_package();
        let scenario = compiled_scenario(
            "scenario.malformed",
            "decision.strict",
            BTreeMap::from([(path("member.age"), Value::Text("sixty".to_owned()))]),
            expected_decision(
                OutcomeKind::Escalate,
                &["rule.guard"],
                &["strict-age-review"],
            ),
            span,
        );

        let results = application
            .run_scenarios(&package, &[scenario])
            .expect("scenario results");
        assert_eq!(results.len(), 1);
        let result = results[0].payload();
        assert_eq!(result.status, ScenarioStatus::Invalid);
        assert!(result.failures.is_empty());
        assert!(result.trace.is_none());
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

    fn compiled_scenario(
        id: &str,
        decision_id: &str,
        given: BTreeMap<FactPath, Value>,
        expect: ExpectedDecision,
        span: Span,
    ) -> CompiledScenario {
        CompiledScenario {
            id: ScenarioId::new(id).expect("scenario"),
            title: id.to_owned(),
            description: None,
            decision: decision(decision_id),
            at: evaluated_at(),
            given,
            expect,
            tags: BTreeSet::new(),
            span,
        }
    }

    fn expected_decision(
        outcome: OutcomeKind,
        determining: &[&str],
        reason_codes: &[&str],
    ) -> ExpectedDecision {
        ExpectedDecision {
            outcome,
            determining_rules: determining.iter().copied().map(qualified).collect(),
            required_facts: BTreeSet::new(),
            reason_codes: reason_codes
                .iter()
                .copied()
                .map(|code| ReasonCode::new(code).expect("code"))
                .collect(),
        }
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

    fn access_package() -> (CompiledPackage, Span) {
        let (source_map, span) = sample_source_map();
        let package = build_package(
            vec![access_decision(span), strict_decision(span)],
            source_map,
            sample_vocabulary(),
        );
        (package, span)
    }

    fn access_decision(span: Span) -> CompiledDecision {
        CompiledDecision::new(
            decision("decision.access"),
            decision_semantics(
                MissingFactStrategy::PreserveUnknown,
                InvalidFactStrategy::PreserveInvalid,
            ),
            deny_effect("no-matching-rule", "No access rule matched this member."),
            vec![
                compiled_rule(
                    "rule.block",
                    100,
                    status_contains("blocked", span),
                    deny_effect("member-blocked", "A blocked member is denied."),
                    3,
                    span,
                ),
                compiled_rule(
                    "rule.review",
                    75,
                    age_contains(70, span),
                    escalate_effect("age-review", "An exact age of 70 needs a human review."),
                    2,
                    span,
                ),
                compiled_rule(
                    "rule.allow",
                    50,
                    status_contains("active", span),
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
            decision("decision.strict"),
            decision_semantics(
                MissingFactStrategy::PreserveUnknown,
                InvalidFactStrategy::RejectEvaluation,
            ),
            deny_effect("no-strict-match", "No strict rule matched this member."),
            vec![compiled_rule(
                "rule.guard",
                200,
                age_contains(70, span),
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

    fn decision_semantics(
        missing: MissingFactStrategy,
        invalid: InvalidFactStrategy,
    ) -> DecisionSemantics {
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

    fn status_contains(expected: &str, span: Span) -> Expr {
        Expr::Predicate(Predicate {
            operator: Operator::Contains,
            left: ExprOperand::Fact(path("member.status")),
            right: Some(ExprOperand::Literal(Value::Text(expected.to_owned()))),
            span,
        })
    }

    fn age_contains(expected: i64, span: Span) -> Expr {
        Expr::Predicate(Predicate {
            operator: Operator::Contains,
            left: ExprOperand::Fact(path("member.age")),
            right: Some(ExprOperand::Literal(Value::Integer(expected))),
            span,
        })
    }

    fn approve_effect(code: &str, message: &str) -> CompiledEffect {
        CompiledEffect::new(Outcome::approve(outcome_reasons(code, message), Vec::new()))
    }

    fn deny_effect(code: &str, message: &str) -> CompiledEffect {
        CompiledEffect::new(Outcome::deny(outcome_reasons(code, message), Vec::new()))
    }

    fn escalate_effect(code: &str, message: &str) -> CompiledEffect {
        CompiledEffect::new(Outcome::escalate(
            EscalationId::new("review.safety").expect("destination"),
            outcome_reasons(code, message),
            Vec::new(),
        ))
    }

    fn outcome_reasons(code: &str, message: &str) -> Reasons {
        Reasons::new(vec![
            Reason::new(ReasonCode::new(code).expect("code"), message).expect("reason"),
        ])
        .expect("reasons")
    }

    fn sample_vocabulary() -> ResolvedVocabulary {
        let member_type = TypeId::new("type.member").expect("type");
        let status_type = TypeId::new("type.status").expect("type");
        let age_type = TypeId::new("type.age").expect("type");
        let text_type = TypeId::new("type.text").expect("type");
        let integer_type = TypeId::new("type.integer").expect("type");
        ResolvedVocabulary {
            roots: BTreeMap::from([(
                path("member"),
                ResolvedRoot {
                    path: path("member"),
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

    fn sample_source_map() -> (SourceMap, Span) {
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

    fn build_package(
        decisions: Vec<CompiledDecision>,
        source_map: SourceMap,
        vocabulary: ResolvedVocabulary,
    ) -> CompiledPackage {
        CompiledPackage::new(
            CompiledPackageDraft {
                package_id: PackageId::new("pkg.main").expect("package"),
                package_version: Version::new("1.0.0").expect("version"),
                language_version: LanguageVersion::V1,
                compiler_identity: "compiler".to_owned(),
                decisions,
                actions: Vec::new(),
                integrity: PackageIntegritySet::new(CompilationInput::new(
                    ContentHash::from_bytes([1; 32]),
                    ContentHash::from_bytes([2; 32]),
                    None,
                )),
            },
            source_map,
            vocabulary,
        )
        .expect("package")
    }

    fn decision(value: &str) -> DecisionId {
        DecisionId::new(value).expect("decision")
    }

    fn qualified(rule: &str) -> QualifiedRuleId {
        QualifiedRuleId::new(
            PackageId::new("pkg.main").expect("package"),
            RuleId::new(rule).expect("rule"),
        )
    }

    fn evaluated_at() -> UtcInstant {
        UtcInstant::new(1_772_955_000_000_000_000).expect("instant")
    }
}
