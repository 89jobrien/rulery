//! Typed scenario compilation against a compiled package.

use std::collections::{BTreeMap, BTreeSet};

use rulery_contracts::{
    DecisionId, FactPath, OutcomeKind, QualifiedRuleId, ReasonCode, ScenarioId, Span, StableId,
    UtcInstant, Value,
};
use rulery_diagnostics::DiagnosticCode;
use rulery_ir::CompiledPackage;

/// Source expectation before package reference validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceExpectedDecision {
    /// Expected outcome kind.
    pub outcome: OutcomeKind,
    /// Expected determining rules.
    pub determining_rules: BTreeSet<QualifiedRuleId>,
    /// Expected required facts.
    pub required_facts: BTreeSet<FactPath>,
    /// Expected reason codes.
    pub reason_codes: BTreeSet<ReasonCode>,
}

/// Rich root-authored scenario source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScenarioSource {
    /// Scenario identity.
    pub id: ScenarioId,
    /// Display title.
    pub title: String,
    /// Optional description.
    pub description: Option<String>,
    /// Decision under test.
    pub decision: DecisionId,
    /// Fixed evaluation instant.
    pub at: UtcInstant,
    /// Ergonomic typed facts keyed by canonical path.
    pub given: BTreeMap<FactPath, Value>,
    /// Expected decision details.
    pub expect: SourceExpectedDecision,
    /// Scenario tags.
    pub tags: BTreeSet<StableId>,
    /// Authored source span.
    pub span: Span,
    /// Whether assembly sourced this scenario from the root package.
    pub root_authored: bool,
}

/// Fully resolved expected decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpectedDecision {
    /// Expected outcome kind.
    pub outcome: OutcomeKind,
    /// Resolved determining rules.
    pub determining_rules: BTreeSet<QualifiedRuleId>,
    /// Resolved required fact paths.
    pub required_facts: BTreeSet<FactPath>,
    /// Expected reason codes.
    pub reason_codes: BTreeSet<ReasonCode>,
}

/// Valid compiled scenario.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledScenario {
    /// Scenario identity.
    pub id: ScenarioId,
    /// Display title.
    pub title: String,
    /// Optional description.
    pub description: Option<String>,
    /// Resolved decision.
    pub decision: DecisionId,
    /// Fixed evaluation instant.
    pub at: UtcInstant,
    /// Typed facts.
    pub given: BTreeMap<FactPath, Value>,
    /// Resolved expectation.
    pub expect: ExpectedDecision,
    /// Canonical tags.
    pub tags: BTreeSet<StableId>,
    /// Authored source span.
    pub span: Span,
}

/// Deterministic scenario diagnostic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScenarioDiagnostic {
    /// Stable diagnostic code.
    pub code: DiagnosticCode,
    /// Scenario identity.
    pub scenario: ScenarioId,
    /// Human-readable message.
    pub message: String,
    /// Authored source span.
    pub span: Span,
}

/// Scenario compilation result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScenarioCompilationOutput {
    /// Valid scenarios only.
    pub scenarios: Vec<CompiledScenario>,
    /// All deterministic diagnostics.
    pub diagnostics: Vec<ScenarioDiagnostic>,
}

/// Compiler for root-authored scenarios.
#[derive(Clone, Debug, Default)]
pub struct ScenarioCompiler;

impl ScenarioCompiler {
    /// Resolves and validates scenarios against a compiled package.
    #[must_use]
    pub fn compile(
        &self,
        package: &CompiledPackage,
        mut sources: Vec<ScenarioSource>,
    ) -> ScenarioCompilationOutput {
        sources.sort_by(|left, right| left.id.cmp(&right.id));
        let mut scenarios = Vec::new();
        let mut diagnostics = Vec::new();
        for source in sources {
            let mut source_diagnostics = Vec::new();
            if !source.root_authored {
                source_diagnostics.push(diagnostic(
                    DiagnosticCode::STALE_SCENARIO,
                    &source,
                    "imported packages cannot contribute executable scenarios",
                ));
            }
            if !package.contains_decision(&source.decision) {
                source_diagnostics.push(diagnostic(
                    DiagnosticCode::STALE_SCENARIO,
                    &source,
                    format!(
                        "decision `{}` is not present in the package",
                        source.decision
                    ),
                ));
            }
            for path in source
                .given
                .keys()
                .chain(source.expect.required_facts.iter())
            {
                if !package.contains_fact_path(path) {
                    source_diagnostics.push(diagnostic(
                        DiagnosticCode::SCENARIO_UNDECLARED_FACT,
                        &source,
                        format!("fact path `{path}` is not declared"),
                    ));
                }
            }
            for rule in &source.expect.determining_rules {
                if !package.contains_rule(rule) {
                    source_diagnostics.push(diagnostic(
                        DiagnosticCode::STALE_SCENARIO,
                        &source,
                        format!("determining rule `{rule}` is not present in the package"),
                    ));
                }
            }

            if source_diagnostics.is_empty() {
                scenarios.push(CompiledScenario {
                    id: source.id,
                    title: source.title,
                    description: source.description,
                    decision: source.decision,
                    at: source.at,
                    given: source.given,
                    expect: ExpectedDecision {
                        outcome: source.expect.outcome,
                        determining_rules: source.expect.determining_rules,
                        required_facts: source.expect.required_facts,
                        reason_codes: source.expect.reason_codes,
                    },
                    tags: source.tags,
                    span: source.span,
                });
            } else {
                diagnostics.extend(source_diagnostics);
            }
        }
        diagnostics.sort_by(|left, right| {
            left.scenario
                .cmp(&right.scenario)
                .then_with(|| left.code.as_str().cmp(right.code.as_str()))
                .then_with(|| left.message.cmp(&right.message))
        });
        ScenarioCompilationOutput {
            scenarios,
            diagnostics,
        }
    }
}

fn diagnostic(
    code: &'static str,
    source: &ScenarioSource,
    message: impl Into<String>,
) -> ScenarioDiagnostic {
    ScenarioDiagnostic {
        code: DiagnosticCode::new(code).expect("registry diagnostic constants are valid"),
        scenario: source.id.clone(),
        message: message.into(),
        span: source.span,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rulery_contracts::{
        ContentHash, LanguageVersion, PackageId, ReasonCode, RuleId, SourceFile, SourceId,
        SourceKey, SourceMap, SourcePath, TypeId, Version,
    };
    use rulery_ir::{
        CompilationInput, CompiledDecision, CompiledPackageDraft, CompiledRule, Expr,
        FieldDeclaration, FieldPresence, PackageIntegritySet, ResolvedRoot, ResolvedType,
        ResolvedVocabulary, TypeDeclaration,
    };

    use super::*;

    #[test]
    fn scenario_compiler_resolves_every_expectation_field() {
        let (package, span) = package();
        let valid = source("scenario.valid", span, true);
        let mut undeclared = source("scenario.undeclared", span, true);
        undeclared.given.insert(
            "member.unknown".parse().expect("path"),
            Value::Text("x".to_owned()),
        );
        let mut stale = source("scenario.stale", span, true);
        stale.expect.determining_rules.insert(QualifiedRuleId::new(
            PackageId::new("pkg.main").expect("package"),
            RuleId::new("rule.removed").expect("rule"),
        ));
        let imported = source("scenario.imported", span, false);

        let output =
            ScenarioCompiler.compile(&package, vec![stale, valid.clone(), imported, undeclared]);
        assert_eq!(output.scenarios.len(), 1);
        assert_eq!(output.scenarios[0].id, valid.id);
        assert_eq!(output.scenarios[0].decision, valid.decision);
        assert_eq!(output.scenarios[0].given, valid.given);
        assert_eq!(output.scenarios[0].expect.outcome, OutcomeKind::Approve);
        assert_eq!(
            output.scenarios[0].expect.determining_rules,
            valid.expect.determining_rules
        );
        assert_eq!(
            output.scenarios[0].expect.required_facts,
            valid.expect.required_facts
        );
        assert_eq!(
            output.scenarios[0].expect.reason_codes,
            valid.expect.reason_codes
        );
        assert_eq!(output.scenarios[0].tags, valid.tags);
        assert_eq!(output.scenarios[0].at, valid.at);
        assert_eq!(output.scenarios[0].span, valid.span);
        assert!(
            output
                .diagnostics
                .iter()
                .any(|entry| entry.code.as_str() == DiagnosticCode::SCENARIO_UNDECLARED_FACT)
        );
        assert!(
            output
                .diagnostics
                .iter()
                .any(|entry| entry.code.as_str() == DiagnosticCode::STALE_SCENARIO)
        );
        assert_eq!(
            output
                .diagnostics
                .iter()
                .map(|entry| entry.scenario.as_str())
                .collect::<Vec<_>>(),
            vec!["scenario.imported", "scenario.stale", "scenario.undeclared"]
        );
    }

    fn source(id: &str, span: Span, root_authored: bool) -> ScenarioSource {
        let package = PackageId::new("pkg.main").expect("package");
        ScenarioSource {
            id: ScenarioId::new(id).expect("scenario"),
            title: id.to_owned(),
            description: Some("scenario".to_owned()),
            decision: DecisionId::new("decision.main").expect("decision"),
            at: UtcInstant::new(1).expect("instant"),
            given: BTreeMap::from([(
                "member.status".parse().expect("path"),
                Value::Text("active".to_owned()),
            )]),
            expect: SourceExpectedDecision {
                outcome: OutcomeKind::Approve,
                determining_rules: BTreeSet::from([QualifiedRuleId::new(
                    package,
                    RuleId::new("rule.allow").expect("rule"),
                )]),
                required_facts: BTreeSet::from(["member.status".parse().expect("path")]),
                reason_codes: BTreeSet::from([ReasonCode::new("allowed").expect("reason")]),
            },
            tags: BTreeSet::from([StableId::new("safety").expect("tag")]),
            span,
            root_authored,
        }
    }

    fn package() -> (CompiledPackage, Span) {
        let mut source_map = SourceMap::new();
        source_map
            .insert(
                SourceKey::new(1),
                SourceFile::new(
                    SourceId::new("source.main").expect("source"),
                    SourcePath::new("rules/main.yaml").expect("path"),
                    Arc::<str>::from("x"),
                ),
            )
            .expect("source");
        let span = source_map.span(SourceKey::new(1), 0, 1).expect("span");
        let package_id = PackageId::new("pkg.main").expect("package");
        let rule = CompiledRule::new(
            RuleId::new("rule.allow").expect("rule"),
            QualifiedRuleId::new(package_id.clone(), RuleId::new("rule.allow").expect("rule")),
            Expr::Constant { value: true, span },
            span,
            1,
        );
        let decision = CompiledDecision::new(
            DecisionId::new("decision.main").expect("decision"),
            vec![rule],
        )
        .expect("decision");
        let record_type = TypeId::new("type.member").expect("type");
        let text_type = TypeId::new("type.text").expect("type");
        let vocabulary = ResolvedVocabulary {
            roots: BTreeMap::from([(
                "member".parse().expect("root"),
                ResolvedRoot {
                    path: "member".parse().expect("root"),
                    type_id: record_type.clone(),
                },
            )]),
            types: BTreeMap::from([
                (
                    record_type.clone(),
                    ResolvedType {
                        id: record_type.clone(),
                        declaration: TypeDeclaration::Record {
                            id: record_type,
                            fields: BTreeMap::from([(
                                StableId::new("status").expect("field"),
                                FieldDeclaration {
                                    type_id: text_type.clone(),
                                    presence: FieldPresence::Required,
                                    derived: false,
                                },
                            )]),
                            closed: true,
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
        };
        let package = CompiledPackage::new(
            CompiledPackageDraft {
                package_id,
                package_version: Version::new("1.0.0").expect("version"),
                language_version: LanguageVersion::V1,
                compiler_identity: "compiler".to_owned(),
                decisions: vec![decision],
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
        .expect("package");
        (package, span)
    }
}
