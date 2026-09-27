//! Canonical tool-library fixture workflow.
//!
//! Loads the authored package through the real store, parser, assembler, and compiler, converts
//! the authored case file into typed case facts, evaluates the canonical decision through the
//! production engine adapter, and renders both artifacts from the resulting trace. Every reported
//! value is read back from that trace, so the workflow can only succeed when the engine really
//! evaluated the compiled package.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use rulery_compiler::{PolicyCompiler, SourceCompilationInput};
use rulery_contracts::{
    CaseFacts, ContentHash, DecimalValue, DecisionId, DurationValue, EnumValue, FactPath,
    FactRootId, Outcome, OutcomeKind, PackagePath, PolicyDate, PolicyTimeZone, QualifiedRuleId,
    Reason, RulebookLock, RulebookLockEnvelope, StableId, TimeZoneDatabaseIdentity, TypeId,
    UtcInstant, Value,
};
use rulery_emit::{ArtifactRenderer, HumanExplanation, HumanRenderer, JsonRenderer};
use rulery_engine::{
    ClockOperand, DecisionTraceEnvelope, DecisionTraceV1, EvaluatedOperand, ExpressionTrace,
    PolicyEvaluator, PredicateTrace, ProductionPolicyEvaluator, RuleTrace, ShortCircuitReason,
    SupersessionReason, TimeZoneDatabase, TimeZoneError, TraceDetail, Truth,
};
use rulery_ir::{CompiledPackage, Operator};
use rulery_store::{FilesystemPackageStore, PackageStore};
use rulery_syntax::{SourceExpectedDecision, SourceScenario, YamlSourceParser};
use rulery_vocabulary::{ResolvedVocabulary, TypeDeclaration};
use serde_yaml::Value as YamlValue;

use crate::{LockMode, PackageAssembler, PackageAssembly, PackageAssemblyService};

/// Canonical decision the fixture explains.
const CANONICAL_DECISION: &str = "checkout";
/// Canonical UTC instant the fixture is evaluated at, `2026-09-16T16:00:00Z`.
const CANONICAL_INSTANT: i128 = 1_789_574_400_000_000_000;
/// Policy timezone the fixture package declares.
const CANONICAL_TIMEZONE: &str = "America/New_York";
/// Policy-local date the canonical instant has in the policy timezone.
const CANONICAL_POLICY_DATE: &str = "2026-09-16";
/// Authored case file supplying the evaluated facts.
const CANONICAL_CASE: &str = "cases/expired-training.yaml";

/// Reproducible fixture explanation artifacts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolLibraryExplain {
    /// Selected outcome kind.
    pub outcome: OutcomeKind,
    /// Determining rule.
    pub determining_rule: QualifiedRuleId,
    /// Policy-local date the engine resolved for the evaluated temporal operands.
    pub policy_date: PolicyDate,
    /// Compiled package hash recorded by the trace.
    pub package_hash: ContentHash,
    /// Canonical case-facts hash recorded by the trace.
    pub facts_hash: ContentHash,
    /// Trace hash recorded by the trace.
    pub trace_hash: ContentHash,
    /// Timezone database identity.
    pub timezone_database: TimeZoneDatabaseIdentity,
    /// Evaluated decision trace both artifacts were rendered from.
    pub trace: DecisionTraceV1,
    /// Strict JSON envelope.
    pub json: Vec<u8>,
    /// Canonical human explanation.
    pub human: String,
}

/// Runs the canonical frozen tool-library check/test/explain workflow.
///
/// The authored package is assembled under [`LockMode::Frozen`] and compiled through
/// [`PolicyCompiler`], `cases/expired-training.yaml` is converted into typed case facts against
/// the compiled vocabulary, and `checkout` is evaluated at `evaluated_at` by
/// [`ProductionPolicyEvaluator`] over a policy-local calendar pinned to the canonical instant, so
/// the workflow never reads host timezone data. The authored root scenario expectation is compared
/// against the evaluated trace, and both artifacts are rendered from that trace.
///
/// Only the canonical instant has pinned policy-local date data, so any other instant is rejected
/// by the calendar port instead of being answered from an unpinned conversion.
///
/// # Errors
///
/// Returns an error when frozen assembly or compilation fails, the authored case file cannot be
/// converted into typed facts, evaluation fails, the authored scenario expectation disagrees with
/// the trace, or the trace lacks the outcome, rule, or policy date the explanation renders.
#[allow(clippy::too_many_lines)]
pub fn tool_library_explain(
    root: &Path,
    evaluated_at: UtcInstant,
) -> Result<ToolLibraryExplain, String> {
    let package_path = PackagePath::new(root.to_path_buf()).map_err(|error| error.to_string())?;
    let store = FilesystemPackageStore::default();
    let lock = store
        .load_lock(&package_path)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "frozen workflow requires rulery.lock".to_owned())?;
    let assembly = PackageAssembler::new(store, YamlSourceParser)
        .assemble(&package_path, LockMode::Frozen)
        .map_err(|error| error.to_string())?;
    let scenario = assembly
        .source_scenarios
        .first()
        .cloned()
        .ok_or_else(|| "frozen workflow requires one root scenario".to_owned())?;
    let package = compile(assembly, &lock)?;
    let facts = case_facts(root, package.vocabulary())?;
    let calendar = pinned_calendar()?;
    let trace = ProductionPolicyEvaluator::new(&calendar)
        .evaluate(
            &package,
            &DecisionId::new(CANONICAL_DECISION).map_err(|error| error.to_string())?,
            &facts,
            evaluated_at,
            TraceDetail::Complete,
        )
        .map_err(|error| error.to_string())?
        .payload()
        .clone();
    let outcome = trace
        .outcome
        .as_ref()
        .ok_or_else(|| "evaluated trace has no outcome".to_owned())?;
    let determining_rule = trace
        .determining_rules
        .first()
        .cloned()
        .ok_or_else(|| "evaluated trace has no determining rule".to_owned())?;
    let policy_date = evaluated_policy_date(&trace, &determining_rule)?;
    expect_scenario(&scenario, outcome, &trace)?;
    let human = HumanRenderer.explain(&HumanExplanation {
        outcome: outcome.kind(),
        package: trace.package.clone(),
        version: trace.package_version.clone(),
        decision: trace.decision.clone(),
        evaluated_at: trace.evaluated_at,
        policy_date: policy_date.clone(),
        timezone: trace.timezone.clone(),
        reasons: reason_lines(outcome),
        determining_rules: trace.determining_rules.clone(),
        conditions: condition_lines(&trace, &determining_rule)?,
        required_facts: required_fact_lines(outcome),
        invalid_facts: invalid_fact_lines(&trace),
        superseded_rules: superseded_lines(&trace),
    });
    let json = JsonRenderer
        .render(&DecisionTraceEnvelope::V1(trace.clone()))
        .map_err(|error| error.to_string())?;
    Ok(ToolLibraryExplain {
        outcome: outcome.kind(),
        determining_rule,
        policy_date,
        package_hash: trace.package_hash,
        facts_hash: trace.facts_hash,
        trace_hash: trace.trace_hash,
        timezone_database: trace.timezone_database.clone(),
        trace,
        json,
        human,
    })
}

/// Compiles one frozen assembly, using the on-disk lock hash as compiler integrity input.
fn compile(assembly: PackageAssembly, lock: &RulebookLock) -> Result<CompiledPackage, String> {
    let lock_hash = ContentHash::digest(
        &serde_json::to_vec(&RulebookLockEnvelope::from(lock.clone()))
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
            "tool-library compilation failed: {}",
            output
                .diagnostics
                .iter()
                .map(|diagnostic| format!("{} {}", diagnostic.code, diagnostic.message))
                .collect::<Vec<_>>()
                .join("; ")
        )
    })
}

/// Converts the authored case file into typed case facts using the compiled vocabulary.
fn case_facts(root: &Path, vocabulary: &ResolvedVocabulary) -> Result<CaseFacts, String> {
    let path = root.join(CANONICAL_CASE);
    let bytes = std::fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let document: YamlValue =
        serde_yaml::from_slice(&bytes).map_err(|error| format!("{}: {error}", path.display()))?;
    let YamlValue::Mapping(roots) = document else {
        return Err(format!("{}: case facts must be a mapping", path.display()));
    };
    let mut facts = BTreeMap::new();
    for (key, value) in roots {
        let name = scalar_text(&key)
            .ok_or_else(|| format!("{}: root names must be scalars", path.display()))?;
        let type_id = vocabulary
            .roots
            .values()
            .find(|root| root.path.to_string() == name)
            .map(|root| root.type_id.clone())
            .ok_or_else(|| format!("`{name}` is not a declared fact root"))?;
        facts.insert(
            FactRootId::new(&name).map_err(|error| error.to_string())?,
            case_value(&value, &type_id, vocabulary, &name)?,
        );
    }
    Ok(CaseFacts::new(facts))
}

/// Converts one authored YAML node into a typed value under its declared type.
fn case_value(
    node: &YamlValue,
    type_id: &TypeId,
    vocabulary: &ResolvedVocabulary,
    path: &str,
) -> Result<Value, String> {
    let Some(declaration) = vocabulary
        .types
        .get(type_id)
        .map(|resolved| &resolved.declaration)
    else {
        return scalar_value(node, type_id, path);
    };
    match declaration {
        TypeDeclaration::Alias { target, .. } => case_value(node, target, vocabulary, path),
        TypeDeclaration::Enum { id, variants } => {
            let symbol =
                scalar_text(node).ok_or_else(|| format!("`{path}` must be an enum symbol"))?;
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
            let YamlValue::Mapping(entries) = node else {
                return Err(format!("`{path}` must be a mapping"));
            };
            let mut record = BTreeMap::new();
            for (key, value) in entries {
                let name = scalar_text(key)
                    .ok_or_else(|| format!("`{path}` field names must be scalars"))?;
                let field = StableId::new(&name).map_err(|error| error.to_string())?;
                let Some(declaration) = fields.get(&field) else {
                    return Err(format!("`{path}.{name}` is not a declared field"));
                };
                let child = format!("{path}.{name}");
                record.insert(
                    field,
                    case_value(value, &declaration.type_id, vocabulary, &child)?,
                );
            }
            Ok(Value::Record(record))
        }
        TypeDeclaration::List { element, .. } => {
            let YamlValue::Sequence(items) = node else {
                return Err(format!("`{path}` must be a sequence"));
            };
            items
                .iter()
                .map(|item| case_value(item, element, vocabulary, path))
                .collect::<Result<Vec<_>, _>>()
                .map(Value::List)
        }
        TypeDeclaration::Primitive => Err(format!("`{type_id}` declares no value kind")),
    }
}

/// Converts one authored scalar into the primitive value its built-in type identity names.
fn scalar_value(node: &YamlValue, type_id: &TypeId, path: &str) -> Result<Value, String> {
    let text = || scalar_text(node).ok_or_else(|| format!("`{path}` must be a scalar"));
    Ok(match type_id.as_str() {
        "bool" | "boolean" => Value::Boolean(
            text()?
                .parse()
                .map_err(|_| format!("`{path}` is not a boolean"))?,
        ),
        "int" | "integer" => Value::Integer(
            text()?
                .parse()
                .map_err(|_| format!("`{path}` is not an integer"))?,
        ),
        "decimal" => {
            Value::Decimal(DecimalValue::parse(text()?).map_err(|error| error.to_string())?)
        }
        "string" | "text" => Value::Text(text()?),
        "date" => Value::Date(PolicyDate::parse(text()?).map_err(|error| error.to_string())?),
        "datetime" | "date-time" => {
            Value::DateTime(UtcInstant::parse(&text()?).map_err(|error| error.to_string())?)
        }
        "duration" => {
            Value::Duration(DurationValue::parse(&text()?).map_err(|error| error.to_string())?)
        }
        other => return Err(format!("`{other}` is not a primitive built-in")),
    })
}

/// Returns the canonical scalar spelling of one YAML scalar node.
fn scalar_text(node: &YamlValue) -> Option<String> {
    match node {
        YamlValue::String(text) => Some(text.clone()),
        YamlValue::Number(number) => Some(number.to_string()),
        YamlValue::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

/// Fails when the authored scenario expectation disagrees with the evaluated trace.
fn expect_scenario(
    scenario: &SourceScenario,
    outcome: &Outcome,
    trace: &DecisionTraceV1,
) -> Result<(), String> {
    let evaluated = SourceExpectedDecision {
        outcome: outcome_name(outcome.kind()).to_owned(),
        determining_rules: trace.determining_rules.iter().cloned().collect(),
        required_facts: required_fact_paths(outcome),
        reason_codes: outcome_reasons(outcome)
            .iter()
            .map(|reason| StableId::new(reason.code().as_str()))
            .collect::<Result<BTreeSet<_>, _>>()
            .map_err(|error| error.to_string())?,
        span: scenario.span,
    };
    let authored = expectation_summary(&scenario.expect);
    let produced = expectation_summary(&evaluated);
    if authored != produced {
        return Err(format!(
            "authored scenario `{}` expects {authored}, evaluation produced {produced}",
            scenario.id
        ));
    }
    Ok(())
}

/// Formats the compared fields of one scenario expectation for a mismatch message.
fn expectation_summary(expectation: &SourceExpectedDecision) -> String {
    format!(
        "{} with determining rules {:?}, required facts {:?}, and reason codes {:?}",
        expectation.outcome,
        expectation.determining_rules,
        expectation.required_facts,
        expectation.reason_codes
    )
}

/// Returns the non-empty reasons of one evaluated outcome.
fn outcome_reasons(outcome: &Outcome) -> Vec<Reason> {
    match outcome {
        Outcome::Approve { reasons, .. }
        | Outcome::Deny { reasons, .. }
        | Outcome::Escalate { reasons, .. }
        | Outcome::RequestInformation { reasons, .. } => reasons.iter().cloned().collect(),
    }
}

/// Renders one outcome's reasons as canonical explanation lines.
fn reason_lines(outcome: &Outcome) -> Vec<String> {
    outcome_reasons(outcome)
        .iter()
        .map(|reason| {
            let code = reason.code();
            let message = reason.message();
            match reason.detail() {
                Some(detail) => format!("[{code}] {message} ({detail})"),
                None => format!("[{code}] {message}"),
            }
        })
        .collect()
}

/// Renders the requested facts of an information-request outcome.
fn required_fact_lines(outcome: &Outcome) -> Vec<String> {
    required_fact_paths(outcome)
        .iter()
        .map(ToString::to_string)
        .collect()
}

/// Returns the sorted requested facts of an information-request outcome.
fn required_fact_paths(outcome: &Outcome) -> BTreeSet<FactPath> {
    match outcome {
        Outcome::RequestInformation { required_facts, .. } => required_facts.paths().clone(),
        Outcome::Approve { .. } | Outcome::Deny { .. } | Outcome::Escalate { .. } => {
            BTreeSet::new()
        }
    }
}

/// Renders the malformed evidence retained by the trace.
fn invalid_fact_lines(trace: &DecisionTraceV1) -> Vec<String> {
    trace
        .invalid_facts
        .iter()
        .map(|entry| match &entry.path {
            Some(path) => format!("{path} {}", entry.error),
            None => entry.error.to_string(),
        })
        .collect()
}

/// Renders the rules the trace records as superseded.
fn superseded_lines(trace: &DecisionTraceV1) -> Vec<String> {
    trace
        .superseded_rules
        .iter()
        .map(|entry| {
            let rule = &entry.rule;
            format!("{rule} ({})", supersession_phrase(entry.reason))
        })
        .collect()
}

/// Returns the policy-local date the engine resolved for a rule's temporal operands.
fn evaluated_policy_date(
    trace: &DecisionTraceV1,
    rule: &QualifiedRuleId,
) -> Result<PolicyDate, String> {
    let entry = rule_trace(trace, rule)?;
    find_policy_date(&entry.condition)
        .ok_or_else(|| format!("`{rule}` evaluated no policy-local date"))
}

/// Renders the evaluated leaves of one rule's condition in trace order.
fn condition_lines(trace: &DecisionTraceV1, rule: &QualifiedRuleId) -> Result<Vec<String>, String> {
    Ok(expression_lines(&rule_trace(trace, rule)?.condition, false))
}

/// Returns the complete rule trace the engine recorded for one qualified rule.
fn rule_trace<'a>(
    trace: &'a DecisionTraceV1,
    rule: &QualifiedRuleId,
) -> Result<&'a RuleTrace, String> {
    trace
        .rule_traces
        .iter()
        .find(|entry| &entry.rule == rule)
        .ok_or_else(|| format!("trace has no rule trace for `{rule}`"))
}

/// Returns the first policy-local date a condition evaluated against.
fn find_policy_date(expression: &ExpressionTrace) -> Option<PolicyDate> {
    match expression {
        ExpressionTrace::All { children, .. } | ExpressionTrace::Any { children, .. } => {
            children.iter().find_map(find_policy_date)
        }
        ExpressionTrace::Not { child, .. } => find_policy_date(child),
        ExpressionTrace::Predicate(predicate) => operand_policy_date(&predicate.lhs)
            .or_else(|| predicate.rhs.as_ref().and_then(operand_policy_date)),
        ExpressionTrace::Constant { .. } | ExpressionTrace::NotEvaluated { .. } => None,
    }
}

/// Returns the policy-local date carried by a reserved `today` operand.
fn operand_policy_date(operand: &EvaluatedOperand) -> Option<PolicyDate> {
    match operand {
        EvaluatedOperand::ClockValue {
            value: Value::Date(date),
            name: ClockOperand::Today,
        } => Some(date.clone()),
        _ => None,
    }
}

/// Renders one condition tree as one line per evaluated leaf, preserving trace order.
///
/// Grouping nodes contribute no line of their own, a negation marks each of its leaves as negated,
/// and a short-circuited leaf is reported as not evaluated with the reason the parent decided.
fn expression_lines(expression: &ExpressionTrace, negated: bool) -> Vec<String> {
    match expression {
        ExpressionTrace::All { children, .. } | ExpressionTrace::Any { children, .. } => children
            .iter()
            .flat_map(|child| expression_lines(child, negated))
            .collect(),
        ExpressionTrace::Not { child, .. } => expression_lines(child, !negated),
        ExpressionTrace::Predicate(predicate) => vec![predicate_line(predicate, negated)],
        ExpressionTrace::Constant { result, value, .. } => {
            vec![format!("{}  constant {value}", truth_text(*result))]
        }
        ExpressionTrace::NotEvaluated { result, reason, .. } => vec![format!(
            "{}  not evaluated ({})",
            truth_text(*result),
            short_circuit_phrase(*reason)
        )],
    }
}

/// Renders one evaluated predicate as an explanation line.
fn predicate_line(predicate: &PredicateTrace, negated: bool) -> String {
    let subject = subject_text(&predicate.lhs);
    let subject = if negated {
        format!("not {subject}")
    } else {
        subject
    };
    let phrase = operator_phrase(predicate.operator);
    match &predicate.rhs {
        Some(rhs) => format!(
            "{}  {subject} {phrase} {}",
            truth_text(predicate.result),
            operand_text(rhs)
        ),
        None => format!("{}  {subject} {phrase}", truth_text(predicate.result)),
    }
}

/// Returns the explained subject of one evaluated operand.
fn subject_text(operand: &EvaluatedOperand) -> String {
    match operand {
        EvaluatedOperand::Value {
            source_path: Some(path),
            ..
        }
        | EvaluatedOperand::Missing { path } => path.to_string(),
        EvaluatedOperand::Value {
            value,
            source_path: None,
        } => value_text(value),
        EvaluatedOperand::Invalid { path, error } => match path {
            Some(path) => format!("{path} ({error})"),
            None => format!("value ({error})"),
        },
        EvaluatedOperand::ClockValue { name, .. } => clock_name(*name).to_owned(),
    }
}

/// Returns the compared value of one evaluated operand.
fn operand_text(operand: &EvaluatedOperand) -> String {
    match operand {
        EvaluatedOperand::Value { value, .. } => value_text(value),
        EvaluatedOperand::Missing { path } => format!("absent {path}"),
        EvaluatedOperand::Invalid { path, error } => match path {
            Some(path) => format!("invalid {path} ({error})"),
            None => format!("invalid value ({error})"),
        },
        EvaluatedOperand::ClockValue { value, name } => {
            format!("{} ({})", clock_name(*name), value_text(value))
        }
    }
}

/// Returns the canonical spelling of one typed value.
fn value_text(value: &Value) -> String {
    match value {
        Value::Null => "null".to_owned(),
        Value::Boolean(flag) => flag.to_string(),
        Value::Integer(number) => number.to_string(),
        Value::Decimal(decimal) => decimal.to_string(),
        Value::Text(text) => text.clone(),
        Value::Date(date) => date.to_string(),
        Value::DateTime(instant) => instant.to_rfc3339(),
        Value::Duration(duration) => duration.to_string(),
        Value::Enum(value) => value.variant().to_string(),
        Value::List(values) => values.iter().map(value_text).collect::<Vec<_>>().join(", "),
        Value::Record(_) => "record".to_owned(),
    }
}

/// Returns the canonical spelling of one truth result.
const fn truth_text(truth: Truth) -> &'static str {
    match truth {
        Truth::True => "true",
        Truth::False => "false",
        Truth::Unknown => "unknown",
        Truth::Invalid => "invalid",
    }
}

/// Returns the authored spelling of one evaluated operator.
const fn operator_phrase(operator: Operator) -> &'static str {
    match operator {
        Operator::Exists => "is present",
        Operator::Missing => "is absent",
        Operator::Equals => "equals",
        Operator::NotEquals => "does not equal",
        Operator::LessThan => "is less than",
        Operator::LessOrEqual => "is at most",
        Operator::GreaterThan => "is greater than",
        Operator::GreaterOrEqual => "is at least",
        Operator::IsOneOf => "is one of",
        Operator::Contains => "contains",
        Operator::StartsWith => "starts with",
        Operator::EndsWith => "ends with",
        Operator::Matches => "matches",
        Operator::Before => "is before",
        Operator::After => "is after",
        Operator::Between => "is between",
        Operator::OnOrBefore => "is on or before",
        Operator::OnOrAfter => "is on or after",
        Operator::IsTrue => "is true",
        Operator::IsFalse => "is false",
    }
}

/// Returns the authored spelling of one reserved clock operand.
const fn clock_name(name: ClockOperand) -> &'static str {
    match name {
        ClockOperand::Today => "today",
        ClockOperand::Now => "now",
    }
}

/// Returns the authored spelling of one short-circuit reason.
const fn short_circuit_phrase(reason: ShortCircuitReason) -> &'static str {
    match reason {
        ShortCircuitReason::DecisiveAnd => "conjunction already decisive",
        ShortCircuitReason::DecisiveOr => "disjunction already decisive",
    }
}

/// Returns the authored spelling of one supersession reason.
const fn supersession_phrase(reason: SupersessionReason) -> &'static str {
    match reason {
        SupersessionReason::LowerSemanticKey => "lower semantic key",
        SupersessionReason::EqualKeyIdenticalOutcome => "equal key with identical outcome",
        SupersessionReason::IrrelevantUnknown => "irrelevant unknown",
        SupersessionReason::IrrelevantInvalid => "irrelevant invalid",
    }
}

/// Returns the authored spelling of one evaluated outcome kind.
const fn outcome_name(kind: OutcomeKind) -> &'static str {
    match kind {
        OutcomeKind::Approve => "approve",
        OutcomeKind::Deny => "deny",
        OutcomeKind::Escalate => "escalate",
        OutcomeKind::RequestInformation => "request_information",
    }
}

/// Policy-local calendar pinned to the canonical fixture instant and zone.
///
/// The fixture must not depend on host timezone data, so the adapter answers only the canonical
/// instant in the canonical zone and refuses everything else.
#[derive(Clone, Debug, Eq, PartialEq)]
struct PinnedPolicyCalendar {
    instant: UtcInstant,
    zone: PolicyTimeZone,
    date: PolicyDate,
}

/// Builds the calendar pinned to the canonical fixture instant.
fn pinned_calendar() -> Result<PinnedPolicyCalendar, String> {
    Ok(PinnedPolicyCalendar {
        instant: UtcInstant::new(CANONICAL_INSTANT).map_err(|error| error.to_string())?,
        zone: PolicyTimeZone::new(CANONICAL_TIMEZONE).map_err(|error| error.to_string())?,
        date: PolicyDate::parse(CANONICAL_POLICY_DATE).map_err(|error| error.to_string())?,
    })
}

impl TimeZoneDatabase for PinnedPolicyCalendar {
    fn identity(&self) -> &'static str {
        "fixture/pinned-policy-calendar-2026a"
    }

    fn local_date(
        &self,
        instant: UtcInstant,
        zone: &PolicyTimeZone,
    ) -> Result<PolicyDate, TimeZoneError> {
        if zone != &self.zone {
            return Err(TimeZoneError::UnknownZone(zone.as_str().to_owned()));
        }
        if instant != self.instant {
            return Err(TimeZoneError::DataUnavailable);
        }
        Ok(self.date.clone())
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use std::sync::Arc;

    use rulery_contracts::{
        FactPath, SourceFile, SourceId, SourceKey, SourceMap, SourcePath, Span,
    };

    use super::*;

    #[test]
    fn condition_lines_render_leaves_negation_and_short_circuit_in_order() {
        let span = span();
        let condition = ExpressionTrace::All {
            result: Truth::True,
            children: vec![
                ExpressionTrace::Predicate(PredicateTrace {
                    result: Truth::True,
                    operator: Operator::Before,
                    lhs: EvaluatedOperand::Value {
                        value: Value::Date(PolicyDate::parse("2026-09-15").expect("date")),
                        source_path: Some(
                            FactPath::from_str("member.training.valid-until").expect("path"),
                        ),
                    },
                    rhs: Some(EvaluatedOperand::ClockValue {
                        value: Value::Date(PolicyDate::parse("2026-09-16").expect("date")),
                        name: ClockOperand::Today,
                    }),
                    span,
                }),
                ExpressionTrace::Not {
                    result: Truth::True,
                    child: Box::new(ExpressionTrace::Predicate(PredicateTrace {
                        result: Truth::True,
                        operator: Operator::Missing,
                        lhs: EvaluatedOperand::Missing {
                            path: FactPath::from_str("tool.reserved-for-member-id").expect("path"),
                        },
                        rhs: None,
                        span,
                    })),
                    span,
                },
                ExpressionTrace::NotEvaluated {
                    result: Truth::True,
                    reason: ShortCircuitReason::DecisiveAnd,
                    span,
                },
            ],
            span,
        };

        assert_eq!(
            expression_lines(&condition, false),
            vec![
                "true  member.training.valid-until is before today (2026-09-16)".to_owned(),
                "true  not tool.reserved-for-member-id is absent".to_owned(),
                "true  not evaluated (conjunction already decisive)".to_owned(),
            ]
        );
    }

    fn span() -> Span {
        let mut map = SourceMap::new();
        map.insert(
            SourceKey::new(1),
            SourceFile::new(
                SourceId::new("source.rules").expect("source"),
                SourcePath::new("rules/checkout.yaml").expect("path"),
                Arc::<str>::from("member access rules"),
            ),
        )
        .expect("source map");
        map.span(SourceKey::new(1), 0, 6).expect("span")
    }
}
