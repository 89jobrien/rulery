//! Artifact projection for every stdout artifact the CLI can emit.

use rulery::analysis::{AnalysisReport, AnalysisReportEnvelope, AnalysisReportV1};
use rulery::contracts::{
    DecisionId, LanguageVersion, Outcome, PolicyDate, PolicyTimeZone, UtcInstant, Value,
};
use rulery::diagnostics::{Diagnostic, DiagnosticReport, DiagnosticReportEnvelope};
use rulery::emit::{
    ArtifactRenderer, DecisionProjection, DecisionTableEnvelope, HumanExplanation, HumanRenderer,
    JsonRenderer, MarkdownRenderer, ProjectionCapabilities, SarifRenderer, build_decision_table,
    validate_projection,
};
use rulery::engine::{
    ClockOperand, DecisionTrace, DecisionTraceEnvelope, DecisionTraceV1, EvaluatedOperand,
    ExpressionTrace, ShortCircuitReason, SupersessionReason, Truth,
};
use rulery::ir::{
    CompiledDecision, CompiledPackage, CompiledPackageDraft, CompiledPackageEnvelope, Operator,
};
use rulery::scenarios::{ScenarioResult, ScenarioResultEnvelope};

use crate::findings::CLI_PRODUCER;
use crate::{Findings, HostError};

/// Renders one diagnostic report envelope from constructible registry diagnostics.
///
/// # Errors
///
/// Returns [`HostError::Internal`] when the diagnostics registry refuses a report whose producer
/// identity this module always supplies.
pub fn diagnostic_envelope(
    diagnostics: &[Diagnostic],
) -> Result<DiagnosticReportEnvelope, HostError> {
    let report = DiagnosticReport::new(diagnostics.to_vec(), CLI_PRODUCER, LanguageVersion::V1)
        .map_err(|error| HostError::Internal(error.to_string()))?;
    Ok(DiagnosticReportEnvelope::from(report))
}

/// Returns the strict analysis report envelope for one completed analysis or diff.
#[must_use]
pub fn analysis_envelope(report: &AnalysisReport) -> AnalysisReportEnvelope {
    AnalysisReportEnvelope::V1(report.payload().clone())
}

/// Renders exactly one strict analysis report envelope.
///
/// # Errors
///
/// Returns [`HostError::Internal`] when the envelope cannot be serialized.
pub fn analysis_json(report: &AnalysisReport) -> Result<Vec<u8>, HostError> {
    JsonRenderer
        .render(&analysis_envelope(report))
        .map_err(|error| HostError::Internal(error.to_string()))
}

/// Renders exactly one strict diagnostic report envelope.
///
/// # Errors
///
/// Returns [`HostError::Internal`] when the diagnostics registry refuses the report.
pub fn diagnostic_json(diagnostics: &[Diagnostic]) -> Result<Vec<u8>, HostError> {
    JsonRenderer
        .render(&diagnostic_envelope(diagnostics)?)
        .map_err(|error| HostError::Internal(error.to_string()))
}

/// Returns the human report lines of one completed analysis.
#[must_use]
pub fn analysis_lines(report: &AnalysisReport, findings: &Findings) -> Vec<String> {
    let payload = report.payload();
    findings
        .lines
        .iter()
        .cloned()
        .chain(coverage_lines(payload))
        .chain(reachability_lines(payload))
        .chain(vec![format!(
            "analysis completeness: {:?}",
            payload.completeness
        )])
        .collect()
}

/// Returns the human report lines of one completed semantic diff.
#[must_use]
pub fn diff_lines(report: &AnalysisReport, findings: &Findings) -> Vec<String> {
    let payload = report.payload();
    analysis_lines(report, findings)
        .into_iter()
        .chain(payload.semantic_diffs.iter().flat_map(|diff| {
            let decision = diff.decision.as_str().to_owned();
            [format!(
                "decision `{decision}` unchanged: {:?}",
                diff.unchanged
            )]
            .into_iter()
            .chain(diff.changes.iter().map(move |change| {
                format!(
                    "decision `{decision}` {}: {:?} -> {:?} ({:?})",
                    change.diagnostic_code,
                    change.before.kind(),
                    change.after.kind(),
                    change.classification
                )
            }))
        }))
        .collect()
}

/// Returns the human report lines of one completed scenario run.
#[must_use]
pub fn scenario_lines(results: &[ScenarioResult]) -> Vec<String> {
    use std::fmt::Write as _;

    results
        .iter()
        .map(|result| {
            let payload = result.payload();
            let mut line = format!("{}: {:?}", payload.scenario.as_str(), payload.status);
            for failure in &payload.failures {
                let _ = write!(
                    line,
                    " ({} expected {} got {})",
                    format!("{:?}", failure.field).to_lowercase(),
                    failure.expected,
                    failure.actual
                );
            }
            line
        })
        .collect()
}

/// Returns one coverage line per decision in canonical report order.
fn coverage_lines(payload: &AnalysisReportV1) -> Vec<String> {
    payload
        .coverage
        .iter()
        .map(|entry| {
            format!(
                "coverage `{}`: {}/{} {:?} {:?}",
                entry.decision.as_str(),
                entry.report.numerator,
                entry.report.denominator,
                entry.report.percent_basis_points,
                entry.report.completeness
            )
        })
        .collect()
}

/// Returns one reachability line per rule in canonical report order.
fn reachability_lines(payload: &AnalysisReportV1) -> Vec<String> {
    payload
        .reachability
        .iter()
        .map(|entry| {
            format!(
                "reachability `{}`: {:?}",
                entry.rule.rule().as_str(),
                entry.status
            )
        })
        .collect()
}

/// Renders exactly one SARIF 2.1.0 log.
///
/// # Errors
///
/// Returns [`HostError::Internal`] when the SARIF log cannot be serialized.
pub fn sarif_log(findings: &Findings) -> Result<Vec<u8>, HostError> {
    let log = SarifRenderer.render(&findings.sarif);
    serde_json::to_vec(&log).map_err(|error| HostError::Internal(error.to_string()))
}

/// Renders the documented JSON array of scenario results in scenario-identity order.
///
/// # Errors
///
/// Returns [`HostError::Internal`] when the array cannot be serialized.
pub fn scenario_results(results: &[ScenarioResult]) -> Result<Vec<u8>, HostError> {
    let mut payloads = results
        .iter()
        .map(|result| ScenarioResultEnvelope::V1(result.payload().clone()))
        .collect::<Vec<_>>();
    payloads.sort_by_key(scenario_identity);
    JsonRenderer
        .render_scenario_results(&payloads)
        .map_err(|error| HostError::Internal(error.to_string()))
}

/// Returns the scenario identity one envelope names.
fn scenario_identity(envelope: &ScenarioResultEnvelope) -> String {
    match envelope {
        ScenarioResultEnvelope::V1(payload) => payload.scenario.as_str().to_owned(),
    }
}

/// Renders exactly one decision-trace envelope.
///
/// # Errors
///
/// Returns [`HostError::Internal`] when the trace envelope cannot be serialized.
pub fn trace_envelope(trace: &DecisionTrace) -> Result<Vec<u8>, HostError> {
    JsonRenderer
        .render(&DecisionTraceEnvelope::V1(trace.payload().clone()))
        .map_err(|error| HostError::Internal(error.to_string()))
}

/// Renders exactly one compiled-package envelope, optionally restricted to one decision.
///
/// # Errors
///
/// Returns [`HostError::Invocation`] when the selected decision is unknown and
/// [`HostError::Internal`] when the restricted package or its envelope cannot be built.
pub fn package_envelope(
    package: &CompiledPackage,
    decision: Option<&DecisionId>,
) -> Result<Vec<u8>, HostError> {
    let rendered = match decision {
        None => package.clone(),
        Some(selected) => restrict_to_decision(package, selected)?,
    };
    JsonRenderer
        .render(&CompiledPackageEnvelope::V1(rendered.payload().clone()))
        .map_err(|error| HostError::Internal(error.to_string()))
}

/// Rebuilds the package with a single decision, retaining actions, vocabulary, and integrity.
///
/// The restricted package is a complete compiled package in its own right, so its content hash is
/// recomputed from the retained payload instead of copied from the unrestricted package.
fn restrict_to_decision(
    package: &CompiledPackage,
    decision: &DecisionId,
) -> Result<CompiledPackage, HostError> {
    let compiled = select_decision(package, decision)?;
    CompiledPackage::new(
        CompiledPackageDraft {
            package_id: package.payload().package_id().clone(),
            package_version: package.payload().package_version().clone(),
            language_version: package.payload().language_version(),
            compiler_identity: package.payload().compiler_identity().to_owned(),
            decisions: vec![compiled.clone()],
            actions: package.payload().actions().values().cloned().collect(),
            integrity: package.payload().integrity().clone(),
        },
        package.source_map().clone(),
        package.vocabulary().clone(),
    )
    .map_err(|error| HostError::Internal(error.to_string()))
}

/// Renders one Markdown truth document covering the selected or every decision in ascending order.
///
/// # Errors
///
/// Returns [`HostError::Invocation`] when the selected decision is unknown.
pub fn markdown_document(
    package: &CompiledPackage,
    decision: Option<&DecisionId>,
) -> Result<Vec<u8>, HostError> {
    let projections = select_projections(package, decision)?;
    let mut document = String::new();
    for (index, projection) in projections.iter().enumerate() {
        if index > 0 {
            document.push('\n');
        }
        document.push_str("## ");
        document.push_str(projection.decision.as_str());
        document.push('\n');
        for rule in &projection.rules {
            document.push_str(
                &MarkdownRenderer.render_truths(
                    &rule
                        .truth_states
                        .iter()
                        .map(|truth| (rule.rule.rule().as_str(), *truth))
                        .collect::<Vec<_>>(),
                ),
            );
        }
    }
    Ok(document.into_bytes())
}

/// Returns the decisions one render covers, in ascending identity order.
///
/// # Errors
///
/// Returns [`HostError::Invocation`] when the selected decision is unknown.
fn select_decisions<'a>(
    package: &'a CompiledPackage,
    decision: Option<&DecisionId>,
) -> Result<Vec<&'a CompiledDecision>, HostError> {
    match decision {
        None => Ok(package.payload().decisions().values().collect()),
        Some(selected) => select_decision(package, selected).map(|compiled| vec![compiled]),
    }
}

/// Returns one compiled decision, rejecting an unknown identity as an invalid argument value.
fn select_decision<'a>(
    package: &'a CompiledPackage,
    decision: &DecisionId,
) -> Result<&'a CompiledDecision, HostError> {
    package
        .payload()
        .decisions()
        .get(decision)
        .ok_or_else(|| unknown_decision(decision))
}

/// Returns the decision-table projection of every decision one render covers.
///
/// # Errors
///
/// Returns [`HostError::Invocation`] when the selected decision is unknown.
fn select_projections(
    package: &CompiledPackage,
    decision: Option<&DecisionId>,
) -> Result<Vec<DecisionProjection>, HostError> {
    select_decisions(package, decision)?
        .into_iter()
        .map(|compiled| {
            DecisionProjection::from_decision(package, compiled.id())
                .ok_or_else(|| unknown_decision(compiled.id()))
        })
        .collect()
}

/// Reports one unknown decision identity as an invalid argument value.
fn unknown_decision(decision: &DecisionId) -> HostError {
    HostError::Invocation(format!(
        "`{decision}` is not a decision of the compiled package"
    ))
}

/// Lowers one decision to exactly one decision-table envelope.
///
/// # Errors
///
/// Returns [`HostError::Invocation`] when the decision is unknown and [`HostError::Internal`]
/// when the table or its envelope cannot be built.
pub fn decision_table(
    package: &CompiledPackage,
    decision: &DecisionId,
) -> Result<Vec<u8>, HostError> {
    let projection = DecisionProjection::from_decision(package, decision)
        .ok_or_else(|| unknown_decision(decision))?;
    let table = build_decision_table(package.payload().package_hash(), vec![projection])
        .map_err(|error| HostError::Internal(error.to_string()))?;
    JsonRenderer
        .render(&DecisionTableEnvelope::V1(table.payload().clone()))
        .map_err(|error| HostError::Internal(error.to_string()))
}

/// Reports every semantic feature one decision-table projection cannot preserve.
///
/// # Errors
///
/// Returns [`HostError::Invocation`] when the decision is unknown.
pub fn projection_losses(
    package: &CompiledPackage,
    decision: &DecisionId,
) -> Result<Vec<String>, HostError> {
    let projection = DecisionProjection::from_decision(package, decision)
        .ok_or_else(|| unknown_decision(decision))?;
    Ok(validate_projection(
        &projection.rules,
        ProjectionCapabilities {
            escalation: true,
            information_request: true,
            uncertainty: true,
            actions: true,
            reasons: true,
        },
    )
    .iter()
    .map(|loss| {
        format!(
            "{}: `{}` cannot represent {:?} for `{}`",
            loss.code,
            package.payload().package_id(),
            loss.feature,
            loss.rule
        )
    })
    .collect())
}

/// Renders the canonical human explanation text for one evaluated trace.
///
/// # Errors
///
/// Returns [`HostError::Internal`] when the trace records no outcome, or when the policy-local date
/// cannot be resolved for the timezone the engine recorded.
pub fn human_explanation_text(trace: &DecisionTrace) -> Result<Vec<u8>, HostError> {
    Ok(HumanRenderer.explain(&explanation(trace)?).into_bytes())
}

/// Renders stable line-oriented human text.
#[must_use]
pub fn human_lines(lines: &[String]) -> Vec<u8> {
    HumanRenderer.diagnostics(lines).into_bytes()
}

/// Builds the complete presentation data for one human decision explanation.
fn explanation(trace: &DecisionTrace) -> Result<HumanExplanation, HostError> {
    let payload = trace.payload();
    let Some(outcome) = payload.outcome.as_ref() else {
        return Err(HostError::Internal(
            "evaluated trace has no outcome to explain".to_owned(),
        ));
    };
    Ok(HumanExplanation {
        outcome: outcome.kind(),
        package: payload.package.clone(),
        version: payload.package_version.clone(),
        decision: payload.decision.clone(),
        evaluated_at: payload.evaluated_at,
        policy_date: policy_date(payload)?,
        timezone: payload.timezone.clone(),
        reasons: reason_lines(outcome),
        determining_rules: payload.determining_rules.clone(),
        conditions: condition_lines(payload),
        required_facts: required_fact_lines(outcome),
        invalid_facts: invalid_fact_lines(payload),
        superseded_rules: superseded_lines(payload),
    })
}

/// Returns the policy-local date the evaluated trace already carries.
///
/// A trace that evaluated a reserved `today` operand records the date the engine resolved. A trace
/// that evaluated none has no date of its own, so the date falls back to the same versioned
/// time-zone adapter the engine used, which introduces no new host dependency: the recorded
/// database identity already names that adapter.
fn policy_date(trace: &DecisionTraceV1) -> Result<PolicyDate, HostError> {
    trace
        .rule_traces
        .iter()
        .find_map(|rule| today_operand(&rule.condition))
        .map_or_else(
            || resolve_local_date(trace.evaluated_at, &trace.timezone),
            Ok,
        )
}

/// Resolves the policy-local date through the engine's own time-zone adapter.
fn resolve_local_date(
    instant: UtcInstant,
    timezone: &PolicyTimeZone,
) -> Result<PolicyDate, HostError> {
    let zones = rulery::engine::JiffTimeZoneDatabase::default();
    rulery::engine::TimeZoneDatabase::local_date(&zones, instant, timezone)
        .map_err(|error| HostError::Internal(error.to_string()))
}

/// Returns the policy date carried by the first reserved `today` operand in a condition.
fn today_operand(expression: &ExpressionTrace) -> Option<PolicyDate> {
    match expression {
        ExpressionTrace::All { children, .. } | ExpressionTrace::Any { children, .. } => {
            children.iter().find_map(today_operand)
        }
        ExpressionTrace::Not { child, .. } => today_operand(child),
        ExpressionTrace::Predicate(predicate) => {
            operand_date(&predicate.lhs).or_else(|| predicate.rhs.as_ref().and_then(operand_date))
        }
        ExpressionTrace::Constant { .. } | ExpressionTrace::NotEvaluated { .. } => None,
    }
}

/// Returns the policy date a reserved `today` operand carried.
fn operand_date(operand: &EvaluatedOperand) -> Option<PolicyDate> {
    match operand {
        EvaluatedOperand::ClockValue {
            value: Value::Date(date),
            name: ClockOperand::Today,
        } => Some(date.clone()),
        _ => None,
    }
}

/// Returns the canonical reason lines of one evaluated outcome.
fn reason_lines(outcome: &Outcome) -> Vec<String> {
    let reasons = match outcome {
        Outcome::Approve { reasons, .. }
        | Outcome::Deny { reasons, .. }
        | Outcome::Escalate { reasons, .. }
        | Outcome::RequestInformation { reasons, .. } => reasons,
    };
    reasons
        .iter()
        .map(|reason| match reason.detail() {
            Some(detail) => format!("[{}] {} ({detail})", reason.code(), reason.message()),
            None => format!("[{}] {}", reason.code(), reason.message()),
        })
        .collect()
}

/// Returns the requested-fact lines of one evaluated outcome.
fn required_fact_lines(outcome: &Outcome) -> Vec<String> {
    match outcome {
        Outcome::RequestInformation { required_facts, .. } => required_facts
            .paths()
            .iter()
            .map(ToString::to_string)
            .collect(),
        Outcome::Approve { .. } | Outcome::Deny { .. } | Outcome::Escalate { .. } => Vec::new(),
    }
}

/// Returns the malformed-evidence lines retained by a trace.
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

/// Returns the superseded-rule lines retained by a trace.
fn superseded_lines(trace: &DecisionTraceV1) -> Vec<String> {
    trace
        .superseded_rules
        .iter()
        .map(|entry| format!("{} ({})", entry.rule, supersession_phrase(entry.reason)))
        .collect()
}

/// Renders every evaluated leaf of the determining rules' conditions, in trace order.
fn condition_lines(trace: &DecisionTraceV1) -> Vec<String> {
    trace
        .rule_traces
        .iter()
        .filter(|entry| entry.selected)
        .flat_map(|entry| expression_lines(&entry.condition))
        .collect()
}

/// Renders one condition tree as one line per evaluated leaf, preserving trace order.
fn expression_lines(expression: &ExpressionTrace) -> Vec<String> {
    match expression {
        ExpressionTrace::All { children, .. } | ExpressionTrace::Any { children, .. } => {
            children.iter().flat_map(expression_lines).collect()
        }
        ExpressionTrace::Not { child, .. } => expression_lines(child)
            .into_iter()
            .map(|line| format!("not ({line})"))
            .collect(),
        ExpressionTrace::Predicate(predicate) => {
            let phrase = operator_phrase(predicate.operator);
            vec![match &predicate.rhs {
                Some(rhs) => format!(
                    "{}  {} {phrase} {}",
                    truth_text(predicate.result),
                    operand_text(&predicate.lhs),
                    operand_text(rhs)
                ),
                None => format!(
                    "{}  {} {phrase}",
                    truth_text(predicate.result),
                    operand_text(&predicate.lhs)
                ),
            }]
        }
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

/// Returns the explained subject of one evaluated operand.
fn operand_text(operand: &EvaluatedOperand) -> String {
    match operand {
        EvaluatedOperand::Value {
            value: _,
            source_path: Some(path),
        } => path.to_string(),
        EvaluatedOperand::Value {
            value,
            source_path: None,
        } => value_text(value),
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

/// Returns the authored spelling of one reserved clock operand.
const fn clock_name(name: ClockOperand) -> &'static str {
    match name {
        ClockOperand::Today => "today",
        ClockOperand::Now => "now",
    }
}
