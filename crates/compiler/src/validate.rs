//! Symbol validation for compiler inputs.

use std::collections::BTreeSet;
use std::str::FromStr;

use rulery_diagnostics::DiagnosticCode;

use crate::{CompilationInput, resolve::ResolvedSymbols, resolve::SymbolError};

/// One validation issue before conversion to diagnostic output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationIssue {
    /// Registry code.
    pub code: DiagnosticCode,
    /// Human-readable explanation.
    pub message: String,
    /// Provenance source label.
    pub source_label: String,
}

/// Validates declared and referenced symbols.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn validate_symbols(
    input: &CompilationInput,
    resolved: &Result<ResolvedSymbols, SymbolError>,
) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    if matches!(resolved, Err(SymbolError::EmptyCompilerIdentity)) {
        issues.push(issue(
            DiagnosticCode::UNKNOWN_SYMBOL,
            "compiler identity is required",
            "rulery.yaml:compiler",
        ));
        return issues;
    }
    let Ok(resolved) = resolved else {
        return issues;
    };

    push_duplicate_issues(
        &mut issues,
        input
            .decisions
            .iter()
            .map(rulery_contracts::DecisionId::as_str),
        "decision",
        "rulery.yaml:decisions",
    );
    push_duplicate_issues(
        &mut issues,
        input.actions.iter().map(rulery_contracts::ActionId::as_str),
        "action",
        "actions.yaml:actions",
    );
    push_duplicate_issues(
        &mut issues,
        input.types.iter().map(rulery_contracts::TypeId::as_str),
        "type",
        "vocabulary.yaml:types",
    );

    for rule in &input.rules {
        if !resolved.decisions.contains(&rule.decision) {
            issues.push(issue(
                DiagnosticCode::UNKNOWN_SYMBOL,
                format!("decision `{}` is not declared", rule.decision),
                &rule.source_label,
            ));
        }

        for action in &rule.action_calls {
            if !resolved.actions.contains(action) {
                issues.push(issue(
                    DiagnosticCode::UNKNOWN_SYMBOL,
                    format!("action `{action}` is not declared"),
                    &rule.source_label,
                ));
            }
        }

        for fact_path in &rule.fact_paths {
            if !resolved.fact_paths.contains(fact_path) {
                issues.push(issue(
                    DiagnosticCode::INVALID_FACT_PATH,
                    format!("fact path `{fact_path}` is not declared"),
                    &rule.source_label,
                ));
            }
        }

        for term in &rule.terms {
            if resolved.terms.contains(term) {
                continue;
            }
            let code = if term.as_str().contains("temporal") {
                DiagnosticCode::TEMPORAL_TERM_UNDEFINED
            } else {
                DiagnosticCode::UNDEFINED_OPERATIONAL_TERM
            };
            issues.push(issue(
                code,
                format!("operational term `{term}` is not declared"),
                &rule.source_label,
            ));
        }

        if let Some(target) = &rule.override_target {
            match rulery_contracts::QualifiedRuleId::from_str(target) {
                Ok(qualified) => {
                    if qualified.package() != &input.package_id
                        && !resolved.imports.contains(qualified.package())
                    {
                        issues.push(issue(
                            DiagnosticCode::UNKNOWN_SYMBOL,
                            format!(
                                "override target `{target}` is not qualified to this package or imports"
                            ),
                            &rule.source_label,
                        ));
                    }
                }
                Err(_) => {
                    issues.push(issue(
                        DiagnosticCode::UNKNOWN_SYMBOL,
                        format!("override target `{target}` is not a valid qualified rule id"),
                        &rule.source_label,
                    ));
                }
            }

            if rule
                .override_rationale
                .as_ref()
                .is_none_or(|value| value.trim().is_empty())
            {
                issues.push(issue(
                    DiagnosticCode::UNKNOWN_SYMBOL,
                    "override requires non-empty rationale",
                    &rule.source_label,
                ));
            }
        }
    }

    issues.sort_by(|left, right| {
        left.code
            .as_str()
            .cmp(right.code.as_str())
            .then_with(|| left.source_label.cmp(&right.source_label))
            .then_with(|| left.message.cmp(&right.message))
    });
    issues
}

fn issue(
    code: &'static str,
    message: impl Into<String>,
    source_label: impl Into<String>,
) -> ValidationIssue {
    ValidationIssue {
        code: DiagnosticCode::new(code).expect("registry diagnostic code constants are valid"),
        message: message.into(),
        source_label: source_label.into(),
    }
}

fn push_duplicate_issues<'a>(
    issues: &mut Vec<ValidationIssue>,
    values: impl Iterator<Item = &'a str>,
    kind: &str,
    label: &str,
) {
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(value.to_owned()) {
            issues.push(issue(
                DiagnosticCode::DUPLICATE_DECLARATION,
                format!("duplicate {kind} declaration `{value}`"),
                label,
            ));
        }
    }
}
