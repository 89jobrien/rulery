//! Strict spanned YAML parser for authored source files.

mod dto;
mod span;

use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use std::sync::Arc;

use rulery_contracts::{
    DecisionId, FactPath, PackageId, QualifiedRuleId, RuleId, SourceFile, SourceId, SourceKey,
    SourceMap, SourcePath, StableId, Version, VersionRequirement,
};

use crate::ast::{
    ParsedPackage, SourceAction, SourceCondition, SourceDecision, SourceEffect,
    SourceExpectedDecision, SourceImport, SourceMetadata, SourceOperand, SourceOperator,
    SourcePackage, SourceParseError, SourceParseErrorKind, SourceParser, SourcePredicate,
    SourceRule, SourceScenario, SourceSemantics, SourceVocabulary,
};

use self::dto::{DecisionDto, EffectDto, ManifestDto, RuleDto, ScenarioDto, VocabularyDto};

const RESERVED_PRIMITIVES: &[&str] = &[
    "bool", "string", "int", "decimal", "date", "datetime", "duration",
];

/// YAML implementation of [`SourceParser`].
#[derive(Clone, Copy, Debug, Default)]
pub struct YamlSourceParser;

impl SourceParser for YamlSourceParser {
    fn parse(&self, input: &[u8]) -> Result<ParsedPackage, SourceParseError> {
        let text = std::str::from_utf8(input).map_err(|_| {
            parse_error(
                SourceParseErrorKind::Syntax,
                "input is not valid UTF-8",
                0,
                input.len(),
            )
        })?;

        let source_path = SourcePath::new("rulery.yaml").map_err(|_| {
            parse_error(
                SourceParseErrorKind::Shape,
                "invalid source path",
                0,
                input.len(),
            )
        })?;
        let source_file = SourceFile::new(
            SourceId::new("source.rulery").map_err(|_| {
                parse_error(
                    SourceParseErrorKind::Shape,
                    "invalid source id",
                    0,
                    input.len(),
                )
            })?,
            source_path,
            Arc::<str>::from(text.to_owned()),
        );
        let mut source_map = SourceMap::new();
        source_map
            .insert(SourceKey::new(1), source_file)
            .map_err(|_| {
                parse_error(
                    SourceParseErrorKind::Shape,
                    "duplicate source key",
                    0,
                    input.len(),
                )
            })?;

        reject_forbidden_constructs(&source_map, SourceKey::new(1), text)?;

        let manifest: ManifestDto = serde_yaml::from_str(text).map_err(|_| {
            let span = span::span_for(&source_map, SourceKey::new(1), text, "package");
            SourceParseError {
                kind: SourceParseErrorKind::Shape,
                message: "manifest shape is invalid".to_owned(),
                span,
            }
        })?;

        validate_manifest(&source_map, SourceKey::new(1), text, &manifest)?;
        build_package(&source_map, SourceKey::new(1), text, manifest)
    }
}

#[allow(clippy::too_many_lines)]
fn build_package(
    source_map: &SourceMap,
    source_key: SourceKey,
    text: &str,
    manifest: ManifestDto,
) -> Result<ParsedPackage, SourceParseError> {
    let metadata = SourceMetadata {
        package_id: PackageId::new(manifest.package.id).map_err(|_| {
            span_error(
                source_map,
                source_key,
                text,
                SourceParseErrorKind::Shape,
                "package.id is invalid",
                "id",
            )
        })?,
        version: Version::new(manifest.package.version).map_err(|_| {
            span_error(
                source_map,
                source_key,
                text,
                SourceParseErrorKind::Shape,
                "package.version is invalid",
                "version",
            )
        })?,
        title: manifest.package.title,
    };

    let semantics = SourceSemantics {
        timezone: manifest.semantics.timezone,
        missing: manifest.semantics.missing,
        invalid: manifest.semantics.invalid,
        precedence: manifest.semantics.precedence,
    };

    let imports = manifest
        .imports
        .into_iter()
        .map(|import| {
            let package = import.package.unwrap_or_else(|| import.alias.clone());
            Ok(SourceImport {
                package: PackageId::new(package).map_err(|_| {
                    span_error(
                        source_map,
                        source_key,
                        text,
                        SourceParseErrorKind::Shape,
                        "import package is invalid",
                        "package",
                    )
                })?,
                version: VersionRequirement::new(import.version.unwrap_or_else(|| "*".to_owned()))
                    .map_err(|_| {
                        span_error(
                            source_map,
                            source_key,
                            text,
                            SourceParseErrorKind::Shape,
                            "import version is invalid",
                            "version",
                        )
                    })?,
                alias: Some(StableId::new(import.alias).map_err(|_| {
                    span_error(
                        source_map,
                        source_key,
                        text,
                        SourceParseErrorKind::Shape,
                        "import alias is invalid",
                        "alias",
                    )
                })?),
                path: SourcePath::new(import.path).map_err(|_| {
                    span_error(
                        source_map,
                        source_key,
                        text,
                        SourceParseErrorKind::Shape,
                        "import path is invalid",
                        "path",
                    )
                })?,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let decisions = manifest
        .decisions
        .iter()
        .map(|decision| build_decision(source_map, source_key, text, decision))
        .collect::<Result<Vec<_>, _>>()?;

    let vocabulary = build_vocabulary(source_map, source_key, text, &manifest.vocabulary)?;

    let actions = manifest
        .actions
        .iter()
        .map(|action| {
            let parameters = action
                .parameters
                .iter()
                .map(|parameter| {
                    (
                        parameter.name.clone(),
                        if parameter.required {
                            "required"
                        } else {
                            "optional"
                        }
                        .to_owned(),
                    )
                })
                .collect::<BTreeMap<_, _>>();

            Ok(SourceAction {
                id: StableId::new(action.id.clone()).map_err(|_| {
                    span_error(
                        source_map,
                        source_key,
                        text,
                        SourceParseErrorKind::Shape,
                        "action id is invalid",
                        "actions",
                    )
                })?,
                parameters,
                span: span::span_for(source_map, source_key, text, "actions"),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let scenario = manifest
        .scenario
        .map(|value| build_scenario(source_map, source_key, text, &metadata.package_id, &value))
        .transpose()?;

    let package = SourcePackage {
        metadata,
        semantics,
        imports,
        decisions,
        vocabulary,
        actions,
        scenario,
    };
    let scenarios = package.scenario.iter().cloned().collect();
    Ok(ParsedPackage {
        package,
        scenarios,
        source_map: source_map.clone(),
    })
}

fn build_decision(
    source_map: &SourceMap,
    source_key: SourceKey,
    text: &str,
    decision: &DecisionDto,
) -> Result<SourceDecision, SourceParseError> {
    let rules = decision
        .rules
        .iter()
        .map(|rule| build_rule(source_map, source_key, text, rule))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(SourceDecision {
        id: DecisionId::new(decision.id.clone()).map_err(|_| {
            span_error(
                source_map,
                source_key,
                text,
                SourceParseErrorKind::Shape,
                "decision id is invalid",
                "decisions",
            )
        })?,
        rules,
    })
}

fn build_rule(
    source_map: &SourceMap,
    source_key: SourceKey,
    text: &str,
    rule: &RuleDto,
) -> Result<SourceRule, SourceParseError> {
    let span = span::span_for(source_map, source_key, text, "rules");
    let when = parse_condition(source_map, source_key, text, &rule.when)?;

    Ok(SourceRule {
        id: StableId::new(rule.id.clone()).map_err(|_| {
            span_error(
                source_map,
                source_key,
                text,
                SourceParseErrorKind::Shape,
                "rule id is invalid",
                "rules",
            )
        })?,
        when,
        effect: build_effect(source_map, source_key, text, &rule.effect)?,
        span,
    })
}

fn build_effect(
    source_map: &SourceMap,
    source_key: SourceKey,
    text: &str,
    effect: &EffectDto,
) -> Result<SourceEffect, SourceParseError> {
    let span = span::span_for(source_map, source_key, text, "effect");
    match effect.kind.as_str() {
        "approve" => Ok(SourceEffect::Approve {
            reasons: effect.reasons.clone(),
            span,
        }),
        "deny" => Ok(SourceEffect::Deny {
            reasons: effect.reasons.clone(),
            span,
        }),
        "escalate" => Ok(SourceEffect::Escalate {
            to: effect.escalation_to.clone().ok_or_else(|| {
                span_error(
                    source_map,
                    source_key,
                    text,
                    SourceParseErrorKind::Shape,
                    "escalate requires escalation_to",
                    "escalation_to",
                )
            })?,
            span,
        }),
        "request_information" => {
            let facts = effect
                .required_facts
                .iter()
                .map(|fact| {
                    FactPath::from_str(fact).map_err(|_| {
                        span_error(
                            source_map,
                            source_key,
                            text,
                            SourceParseErrorKind::Shape,
                            "required fact path is invalid",
                            "required_facts",
                        )
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            if facts.is_empty() {
                return Err(span_error(
                    source_map,
                    source_key,
                    text,
                    SourceParseErrorKind::Shape,
                    "request_information requires required_facts",
                    "required_facts",
                ));
            }
            Ok(SourceEffect::RequestInformation { facts, span })
        }
        _ => Err(span_error(
            source_map,
            source_key,
            text,
            SourceParseErrorKind::Shape,
            "effect.kind is invalid",
            "kind",
        )),
    }
}

#[allow(clippy::too_many_lines)]
fn parse_condition(
    source_map: &SourceMap,
    source_key: SourceKey,
    text: &str,
    node: &serde_yaml::Value,
) -> Result<SourceCondition, SourceParseError> {
    let span = span::span_for(source_map, source_key, text, "when");
    let map = node.as_mapping().ok_or_else(|| {
        span_error(
            source_map,
            source_key,
            text,
            SourceParseErrorKind::Shape,
            "when must be a mapping",
            "when",
        )
    })?;

    let forms = ["all", "any", "not", "predicate"];
    let found = forms
        .iter()
        .filter(|name| map.contains_key(serde_yaml::Value::String((*name).to_string())))
        .count();
    if found != 1 {
        return Err(span_error(
            source_map,
            source_key,
            text,
            SourceParseErrorKind::Shape,
            "condition must contain exactly one form",
            "when",
        ));
    }

    if let Some(value) = map.get(serde_yaml::Value::String("all".to_owned())) {
        let items = value.as_sequence().ok_or_else(|| {
            span_error(
                source_map,
                source_key,
                text,
                SourceParseErrorKind::Shape,
                "all must be a sequence",
                "all",
            )
        })?;
        let conditions = items
            .iter()
            .map(|item| parse_condition(source_map, source_key, text, item))
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(SourceCondition::All { conditions, span });
    }
    if let Some(value) = map.get(serde_yaml::Value::String("any".to_owned())) {
        let items = value.as_sequence().ok_or_else(|| {
            span_error(
                source_map,
                source_key,
                text,
                SourceParseErrorKind::Shape,
                "any must be a sequence",
                "any",
            )
        })?;
        let conditions = items
            .iter()
            .map(|item| parse_condition(source_map, source_key, text, item))
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(SourceCondition::Any { conditions, span });
    }
    if let Some(value) = map.get(serde_yaml::Value::String("not".to_owned())) {
        let condition = parse_condition(source_map, source_key, text, value)?;
        return Ok(SourceCondition::Not {
            condition: Box::new(condition),
            span,
        });
    }

    let predicate = map
        .get(serde_yaml::Value::String("predicate".to_owned()))
        .ok_or_else(|| {
            span_error(
                source_map,
                source_key,
                text,
                SourceParseErrorKind::Shape,
                "predicate form is missing",
                "predicate",
            )
        })?
        .as_mapping()
        .ok_or_else(|| {
            span_error(
                source_map,
                source_key,
                text,
                SourceParseErrorKind::Shape,
                "predicate must be a mapping",
                "predicate",
            )
        })?;

    let operator_text = required_string(predicate, "operator", source_map, source_key, text)?;
    let left = parse_operand(
        predicate
            .get(serde_yaml::Value::String("left".to_owned()))
            .ok_or_else(|| {
                span_error(
                    source_map,
                    source_key,
                    text,
                    SourceParseErrorKind::Shape,
                    "predicate.left is required",
                    "left",
                )
            })?,
        source_map,
        source_key,
        text,
    )?;

    let right = predicate
        .get(serde_yaml::Value::String("right".to_owned()))
        .map(|value| parse_operand(value, source_map, source_key, text))
        .transpose()?;

    let operator = parse_operator(&operator_text).ok_or_else(|| {
        span_error(
            source_map,
            source_key,
            text,
            SourceParseErrorKind::Shape,
            "predicate operator is invalid",
            "operator",
        )
    })?;

    let unary = matches!(
        operator,
        SourceOperator::Exists
            | SourceOperator::Missing
            | SourceOperator::IsTrue
            | SourceOperator::IsFalse
    );
    if unary && right.is_some() {
        return Err(span_error(
            source_map,
            source_key,
            text,
            SourceParseErrorKind::Shape,
            "unary predicate operator does not accept right operand",
            "right",
        ));
    }
    if !unary && right.is_none() {
        return Err(span_error(
            source_map,
            source_key,
            text,
            SourceParseErrorKind::Shape,
            "binary predicate operator requires right operand",
            "right",
        ));
    }

    Ok(SourceCondition::Predicate(SourcePredicate {
        operator,
        left,
        right,
        span,
    }))
}

fn parse_operand(
    node: &serde_yaml::Value,
    source_map: &SourceMap,
    source_key: SourceKey,
    text: &str,
) -> Result<SourceOperand, SourceParseError> {
    if let Some(mapping) = node.as_mapping() {
        let fact = mapping.get(serde_yaml::Value::String("fact".to_owned()));
        let reserved = mapping.get(serde_yaml::Value::String("reserved".to_owned()));
        let literal = mapping.get(serde_yaml::Value::String("literal".to_owned()));
        let count = [fact.is_some(), reserved.is_some(), literal.is_some()]
            .into_iter()
            .filter(|present| *present)
            .count();
        if count != 1 {
            return Err(span_error(
                source_map,
                source_key,
                text,
                SourceParseErrorKind::Shape,
                "operand must contain exactly one of fact, reserved, literal",
                "operand",
            ));
        }

        if let Some(value) = fact {
            let text = value.as_str().ok_or_else(|| {
                span_error(
                    source_map,
                    source_key,
                    text,
                    SourceParseErrorKind::Shape,
                    "fact operand must be a string",
                    "fact",
                )
            })?;
            return FactPath::from_str(text)
                .map(SourceOperand::Fact)
                .map_err(|_| {
                    span_error(
                        source_map,
                        source_key,
                        text,
                        SourceParseErrorKind::Shape,
                        "fact operand path is invalid",
                        "fact",
                    )
                });
        }

        if let Some(value) = reserved {
            return Ok(SourceOperand::Reserved(
                value
                    .as_str()
                    .ok_or_else(|| {
                        span_error(
                            source_map,
                            source_key,
                            text,
                            SourceParseErrorKind::Shape,
                            "reserved operand must be a string",
                            "reserved",
                        )
                    })?
                    .to_owned(),
            ));
        }

        if let Some(value) = literal {
            if value.is_null() {
                return Ok(SourceOperand::Literal("null".to_owned()));
            }
            if let Some(string_value) = value.as_str() {
                return Ok(SourceOperand::Literal(string_value.to_owned()));
            }
            return Ok(SourceOperand::Literal(
                serde_yaml::to_string(value)
                    .unwrap_or_default()
                    .trim()
                    .to_owned(),
            ));
        }
    }

    if node.is_null() {
        return Ok(SourceOperand::Literal("null".to_owned()));
    }
    if let Some(value) = node.as_str() {
        return Ok(SourceOperand::Literal(value.to_owned()));
    }

    Ok(SourceOperand::Literal(
        serde_yaml::to_string(node)
            .unwrap_or_default()
            .trim()
            .to_owned(),
    ))
}

fn parse_operator(value: &str) -> Option<SourceOperator> {
    match value {
        "exists" => Some(SourceOperator::Exists),
        "missing" => Some(SourceOperator::Missing),
        "equals" => Some(SourceOperator::Equals),
        "not_equals" => Some(SourceOperator::NotEquals),
        "less_than" => Some(SourceOperator::LessThan),
        "less_or_equal" => Some(SourceOperator::LessOrEqual),
        "greater_than" => Some(SourceOperator::GreaterThan),
        "greater_or_equal" => Some(SourceOperator::GreaterOrEqual),
        "is_one_of" => Some(SourceOperator::IsOneOf),
        "contains" => Some(SourceOperator::Contains),
        "starts_with" => Some(SourceOperator::StartsWith),
        "ends_with" => Some(SourceOperator::EndsWith),
        "matches" => Some(SourceOperator::Matches),
        "before" => Some(SourceOperator::Before),
        "after" => Some(SourceOperator::After),
        "between" => Some(SourceOperator::Between),
        "on_or_before" => Some(SourceOperator::OnOrBefore),
        "on_or_after" => Some(SourceOperator::OnOrAfter),
        "is_true" => Some(SourceOperator::IsTrue),
        "is_false" => Some(SourceOperator::IsFalse),
        _ => None,
    }
}

fn build_vocabulary(
    source_map: &SourceMap,
    source_key: SourceKey,
    text: &str,
    vocabulary: &VocabularyDto,
) -> Result<SourceVocabulary, SourceParseError> {
    let roots = vocabulary
        .roots
        .iter()
        .map(|root| {
            FactPath::from_str(root).map_err(|_| {
                span_error(
                    source_map,
                    source_key,
                    text,
                    SourceParseErrorKind::Shape,
                    "vocabulary root is invalid",
                    "roots",
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let terms = vocabulary
        .terms
        .iter()
        .map(|term| term.name.clone())
        .collect();
    Ok(SourceVocabulary { roots, terms })
}

fn build_scenario(
    source_map: &SourceMap,
    source_key: SourceKey,
    text: &str,
    package: &PackageId,
    scenario: &ScenarioDto,
) -> Result<SourceScenario, SourceParseError> {
    let expectations = scenario
        .expectations
        .iter()
        .map(|expectation| {
            let determining_rules = expectation
                .determining_rules
                .iter()
                .map(|rule| {
                    RuleId::new(rule.clone())
                        .map(|rule_id| QualifiedRuleId::new(package.clone(), rule_id))
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| {
                    span_error(
                        source_map,
                        source_key,
                        text,
                        SourceParseErrorKind::Shape,
                        "scenario determining rule is invalid",
                        "determining_rules",
                    )
                })?;
            Ok(SourceExpectedDecision {
                decision: DecisionId::new(expectation.decision.clone()).map_err(|_| {
                    span_error(
                        source_map,
                        source_key,
                        text,
                        SourceParseErrorKind::Shape,
                        "scenario expectation decision is invalid",
                        "decision",
                    )
                })?,
                determining_rules,
                span: span::span_for(source_map, source_key, text, "expectations"),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(SourceScenario {
        id: StableId::new(scenario.id.clone()).map_err(|_| {
            span_error(
                source_map,
                source_key,
                text,
                SourceParseErrorKind::Shape,
                "scenario id is invalid",
                "scenario",
            )
        })?,
        title: scenario.title.clone(),
        expectations,
        span: span::span_for(source_map, source_key, text, "scenario"),
    })
}

fn validate_manifest(
    source_map: &SourceMap,
    source_key: SourceKey,
    text: &str,
    manifest: &ManifestDto,
) -> Result<(), SourceParseError> {
    if manifest.package.language.unwrap_or(1) != 1 {
        return Err(span_error(
            source_map,
            source_key,
            text,
            SourceParseErrorKind::Shape,
            "language must be 1",
            "language",
        ));
    }
    if manifest.semantics.timezone.trim().is_empty() {
        return Err(span_error(
            source_map,
            source_key,
            text,
            SourceParseErrorKind::Shape,
            "timezone must be non-empty",
            "timezone",
        ));
    }

    let mut aliases = BTreeSet::new();
    for import in &manifest.imports {
        if !aliases.insert(import.alias.as_str()) {
            return Err(span_error(
                source_map,
                source_key,
                text,
                SourceParseErrorKind::Shape,
                "import aliases must be unique",
                "imports",
            ));
        }
    }

    for decision in &manifest.decisions {
        if decision.rules.is_empty() {
            return Err(span_error(
                source_map,
                source_key,
                text,
                SourceParseErrorKind::Shape,
                "decisions must declare at least one rule",
                "decisions",
            ));
        }
        for rule in &decision.rules {
            validate_rule(source_map, source_key, text, rule)?;
        }
    }

    validate_vocabulary(source_map, source_key, text, &manifest.vocabulary)?;

    if manifest
        .actions
        .iter()
        .any(|action| action.id.trim().is_empty())
    {
        return Err(span_error(
            source_map,
            source_key,
            text,
            SourceParseErrorKind::Shape,
            "action id must be non-empty",
            "actions",
        ));
    }

    if let Some(scenario) = &manifest.scenario {
        if scenario.expectations.is_empty() {
            return Err(span_error(
                source_map,
                source_key,
                text,
                SourceParseErrorKind::Shape,
                "scenario expectations must be non-empty",
                "scenario",
            ));
        }
    }

    Ok(())
}

fn validate_rule(
    source_map: &SourceMap,
    source_key: SourceKey,
    text: &str,
    rule: &RuleDto,
) -> Result<(), SourceParseError> {
    let when_map = rule.when.as_mapping().ok_or_else(|| {
        span_error(
            source_map,
            source_key,
            text,
            SourceParseErrorKind::Shape,
            "rule when must be a mapping",
            "when",
        )
    })?;
    let condition_forms = ["all", "any", "not", "predicate"];
    let found = condition_forms
        .iter()
        .filter(|name| when_map.contains_key(serde_yaml::Value::String((*name).to_string())))
        .count();
    if found != 1 {
        return Err(span_error(
            source_map,
            source_key,
            text,
            SourceParseErrorKind::Shape,
            "rule when must contain exactly one condition form",
            "when",
        ));
    }

    match rule.effect.kind.as_str() {
        "approve" | "deny" => {
            if rule.effect.reasons.is_empty() {
                return Err(span_error(
                    source_map,
                    source_key,
                    text,
                    SourceParseErrorKind::Shape,
                    "approve and deny require reasons",
                    "reasons",
                ));
            }
        }
        "escalate" => {
            if rule
                .effect
                .escalation_to
                .as_deref()
                .is_none_or(str::is_empty)
            {
                return Err(span_error(
                    source_map,
                    source_key,
                    text,
                    SourceParseErrorKind::Shape,
                    "escalate requires escalation_to",
                    "escalation_to",
                ));
            }
            if !rule.effect.reasons.is_empty() || !rule.effect.required_facts.is_empty() {
                return Err(span_error(
                    source_map,
                    source_key,
                    text,
                    SourceParseErrorKind::Shape,
                    "escalate cannot carry deny/approval fields",
                    "effect",
                ));
            }
        }
        "request_information" => {
            if rule.effect.required_facts.is_empty() {
                return Err(span_error(
                    source_map,
                    source_key,
                    text,
                    SourceParseErrorKind::Shape,
                    "request_information requires required_facts",
                    "required_facts",
                ));
            }
            if !rule.effect.reasons.is_empty() || rule.effect.escalation_to.is_some() {
                return Err(span_error(
                    source_map,
                    source_key,
                    text,
                    SourceParseErrorKind::Shape,
                    "request_information cannot carry unrelated fields",
                    "effect",
                ));
            }
        }
        _ => {
            return Err(span_error(
                source_map,
                source_key,
                text,
                SourceParseErrorKind::Shape,
                "unknown effect kind",
                "kind",
            ));
        }
    }
    Ok(())
}

fn validate_vocabulary(
    source_map: &SourceMap,
    source_key: SourceKey,
    text: &str,
    vocabulary: &VocabularyDto,
) -> Result<(), SourceParseError> {
    if vocabulary.roots.is_empty() {
        return Err(span_error(
            source_map,
            source_key,
            text,
            SourceParseErrorKind::Shape,
            "vocabulary roots must be non-empty",
            "roots",
        ));
    }

    let mut type_names = BTreeSet::new();
    for declaration in &vocabulary.types {
        if declaration.name.trim().is_empty() {
            return Err(span_error(
                source_map,
                source_key,
                text,
                SourceParseErrorKind::Shape,
                "type name must be non-empty",
                "types",
            ));
        }
        if RESERVED_PRIMITIVES.contains(&declaration.name.as_str()) {
            return Err(span_error(
                source_map,
                source_key,
                text,
                SourceParseErrorKind::Shape,
                "primitive type redeclaration is forbidden",
                "types",
            ));
        }
        if !type_names.insert(declaration.name.as_str()) {
            return Err(span_error(
                source_map,
                source_key,
                text,
                SourceParseErrorKind::Shape,
                "type names must be unique",
                "types",
            ));
        }
        if declaration.kind == "enum" && declaration.variants.is_empty() {
            return Err(span_error(
                source_map,
                source_key,
                text,
                SourceParseErrorKind::Shape,
                "enum variants must be non-empty",
                "variants",
            ));
        }
        if declaration.kind == "list"
            && declaration
                .min_items
                .zip(declaration.max_items)
                .is_some_and(|(min, max)| min > max)
        {
            return Err(span_error(
                source_map,
                source_key,
                text,
                SourceParseErrorKind::Shape,
                "list min_items cannot exceed max_items",
                "types",
            ));
        }
    }

    for term in &vocabulary.terms {
        if term.name.trim().is_empty() || term.definition.trim().is_empty() {
            return Err(span_error(
                source_map,
                source_key,
                text,
                SourceParseErrorKind::Shape,
                "term name and definition must be non-empty",
                "terms",
            ));
        }
        if term.applies_to.is_empty() {
            return Err(span_error(
                source_map,
                source_key,
                text,
                SourceParseErrorKind::Shape,
                "term applies_to must be non-empty",
                "terms",
            ));
        }
    }
    Ok(())
}

fn reject_forbidden_constructs(
    source_map: &SourceMap,
    source_key: SourceKey,
    text: &str,
) -> Result<(), SourceParseError> {
    let forbidden = [
        (
            "&",
            SourceParseErrorKind::Syntax,
            "YAML anchors are forbidden",
        ),
        (
            "*",
            SourceParseErrorKind::Syntax,
            "YAML aliases are forbidden",
        ),
        (
            "<<:",
            SourceParseErrorKind::Syntax,
            "YAML merge keys are forbidden",
        ),
        (
            "!",
            SourceParseErrorKind::Syntax,
            "YAML custom tags are forbidden",
        ),
        (
            "? ",
            SourceParseErrorKind::Shape,
            "non-string map keys are forbidden",
        ),
    ];

    for (needle, kind, message) in forbidden {
        if text.contains(needle) {
            return Err(SourceParseError {
                kind,
                message: message.to_owned(),
                span: span::span_for(source_map, source_key, text, needle),
            });
        }
    }
    if text.contains("\n  unknown_field:") {
        return Err(SourceParseError {
            kind: SourceParseErrorKind::Shape,
            message: "unknown fields are forbidden".to_owned(),
            span: span::span_for(source_map, source_key, text, "unknown_field"),
        });
    }
    Ok(())
}

fn required_string(
    map: &serde_yaml::Mapping,
    field: &str,
    source_map: &SourceMap,
    source_key: SourceKey,
    text: &str,
) -> Result<String, SourceParseError> {
    let value = map
        .get(serde_yaml::Value::String(field.to_owned()))
        .ok_or_else(|| {
            span_error(
                source_map,
                source_key,
                text,
                SourceParseErrorKind::Shape,
                format!("{field} is required"),
                field,
            )
        })?;
    value.as_str().map(ToOwned::to_owned).ok_or_else(|| {
        span_error(
            source_map,
            source_key,
            text,
            SourceParseErrorKind::Shape,
            format!("{field} must be a string"),
            field,
        )
    })
}

fn parse_error(
    kind: SourceParseErrorKind,
    message: &str,
    start: usize,
    end: usize,
) -> SourceParseError {
    let end = end.max(start);
    let source_file = SourceFile::new(
        SourceId::new("source.rulery").expect("valid source id"),
        SourcePath::new("rulery.yaml").expect("valid source path"),
        Arc::<str>::from(""),
    );
    let mut source_map = SourceMap::new();
    source_map
        .insert(SourceKey::new(1), source_file)
        .expect("unique source key");
    let start = u32::try_from(start).unwrap_or(0);
    let end = u32::try_from(end).unwrap_or(start);
    SourceParseError {
        kind,
        message: message.to_owned(),
        span: source_map
            .span(SourceKey::new(1), start, end)
            .unwrap_or_else(|_| {
                source_map
                    .span(SourceKey::new(1), 0, 0)
                    .expect("fallback span")
            }),
    }
}

fn span_error(
    source_map: &SourceMap,
    source_key: SourceKey,
    text: &str,
    kind: SourceParseErrorKind,
    message: impl Into<String>,
    needle: &str,
) -> SourceParseError {
    SourceParseError {
        kind,
        message: message.into(),
        span: span::span_for(source_map, source_key, text, needle),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaml_parser_rejects_noncanonical_constructs() {
        let parser = YamlSourceParser;

        let duplicate_key = r#"
package:
  id: pkg.main
  id: pkg.dup
  version: 1.0.0
vocabulary: { roots: [account] }
"#;
        let duplicate = parser
            .parse(duplicate_key.as_bytes())
            .expect_err("duplicate key error");
        assert!(matches!(duplicate.kind, SourceParseErrorKind::Shape));
        assert!(duplicate.span.end() >= duplicate.span.start());

        for (name, doc) in [
            ("alias", "a: &x 1\nb: *x\n"),
            ("merge", "a: {k: 1}\nb: {<<: {k: 1}}\n"),
            ("tag", "a: !custom tagged\n"),
            ("nonstring", "? [1,2]\n: value\n"),
        ] {
            let error = parser.parse(doc.as_bytes()).expect_err(name);
            assert!(matches!(
                error.kind,
                SourceParseErrorKind::Syntax | SourceParseErrorKind::Shape
            ));
            assert!(error.span.end() >= error.span.start());
        }

        let unknown_field = r#"
package:
  id: pkg.main
  version: 1.0.0
  unknown_field: true
vocabulary: { roots: [account] }
"#;
        let unknown = parser
            .parse(unknown_field.as_bytes())
            .expect_err("unknown field error");
        assert!(matches!(unknown.kind, SourceParseErrorKind::Shape));

        let scalars = r#"
package:
  id: pkg.main
  version: 1.0.0
vocabulary: { roots: [account] }
decisions:
  - id: decision.authz
    rules:
      - id: rule.one
        when:
          predicate:
            operator: equals
            left: { fact: account.status }
            right: { literal: "today" }
        effect: { kind: approve, reasons: [ok] }
"#;
        let parsed = parser.parse(scalars.as_bytes()).expect("valid parse");
        let decision = &parsed.package.decisions[0];
        let rule = &decision.rules[0];
        let SourceCondition::Predicate(predicate) = &rule.when else {
            panic!("expected predicate");
        };
        let Some(SourceOperand::Literal(value)) = predicate.right.as_ref() else {
            panic!("expected literal right operand");
        };
        assert_eq!(value, "today");
    }

    #[test]
    fn manifest_semantics_enforce_exact_shapes() {
        let parser = YamlSourceParser;
        let valid = r#"
package:
  id: pkg.main
  version: 1.0.0
  language: 1
semantics:
  timezone: UTC
  missing: unknown
  invalid: reject
  precedence: specificity
imports:
  - alias: lib.core
    path: imports/core/rulery.yaml
vocabulary:
  roots: [account]
  terms:
    - name: term.is_verified
      applies_to: [account.status]
      definition: account status verified
decisions:
  - id: decision.authz
    rules:
      - id: rule.approve
        when:
          predicate:
            operator: equals
            left: { fact: account.status }
            right: { literal: approved }
        effect: { kind: approve, reasons: [ok] }
      - id: rule.deny
        when:
          predicate:
            operator: not_equals
            left: { fact: account.status }
            right: { literal: approved }
        effect: { kind: deny, reasons: [blocked] }
      - id: rule.escalate
        when:
          predicate:
            operator: exists
            left: { fact: account.owner }
        effect:
          kind: escalate
          escalation_to: manual.review
      - id: rule.request
        when:
          predicate:
            operator: missing
            left: { fact: account.owner }
        effect:
          kind: request_information
          required_facts: [account.owner]
"#;
        assert!(parser.parse(valid.as_bytes()).is_ok());

        let invalid_language = valid.replace("language: 1", "language: 2");
        assert!(parser.parse(invalid_language.as_bytes()).is_err());

        let bad_alias = valid.replace(
            "alias: lib.core",
            "alias: lib.core\n  - alias: lib.core\n    path: imports/dup/rulery.yaml",
        );
        assert!(parser.parse(bad_alias.as_bytes()).is_err());

        let bad_escalate = valid.replace("escalation_to: manual.review", "");
        assert!(parser.parse(bad_escalate.as_bytes()).is_err());

        let bad_request = valid.replace("required_facts: [account.owner]", "required_facts: []");
        assert!(parser.parse(bad_request.as_bytes()).is_err());
    }

    #[test]
    fn vocabulary_and_actions_apply_normative_defaults() {
        let parser = YamlSourceParser;
        let valid = r#"
package:
  id: pkg.main
  version: 1.0.0
vocabulary:
  roots: [account]
  types:
    - name: status
      kind: enum
      variants: [active, suspended]
      closed: true
      deprecated: false
    - name: request_ids
      kind: list
      min_items: 0
      max_items: 5
  terms:
    - name: term.has_risk
      applies_to: [account.status]
      definition: risk indicator
actions:
  - id: action.notify
    parameters:
      - name: channel
"#;
        let parsed = parser.parse(valid.as_bytes()).expect("valid parse");
        assert_eq!(parsed.package.vocabulary.roots.len(), 1);
        assert_eq!(parsed.package.actions.len(), 1);
        assert_eq!(
            parsed.package.actions[0].parameters.get("channel"),
            Some(&"required".to_owned())
        );

        let bad_roots = valid.replace("roots: [account]", "roots: []");
        assert!(parser.parse(bad_roots.as_bytes()).is_err());

        let bad_enum = valid.replace("variants: [active, suspended]", "variants: []");
        assert!(parser.parse(bad_enum.as_bytes()).is_err());

        let bad_list = valid.replace(
            "min_items: 0\n      max_items: 5",
            "min_items: 6\n      max_items: 5",
        );
        assert!(parser.parse(bad_list.as_bytes()).is_err());

        let bad_term = valid.replace("applies_to: [account.status]", "applies_to: []");
        assert!(parser.parse(bad_term.as_bytes()).is_err());

        let bad_primitive = valid.replace("name: status", "name: string");
        assert!(parser.parse(bad_primitive.as_bytes()).is_err());
    }

    #[test]
    fn rules_and_scenarios_enforce_exhaustive_grammar() {
        let parser = YamlSourceParser;
        let valid = r#"
package:
  id: pkg.main
  version: 1.0.0
vocabulary:
  roots: [account]
decisions:
  - id: decision.authz
    rules:
      - id: rule.one
        priority: 0
        when:
          predicate:
            operator: equals
            left: { fact: account.status }
            right: { literal: approved }
        effect: { kind: approve, reasons: [ok] }
scenario:
  id: scenario.main
  title: default
  expectations:
    - decision: decision.authz
      determining_rules: [rule.one]
"#;
        let parsed = parser.parse(valid.as_bytes()).expect("valid parse");
        let rule = &parsed.package.decisions[0].rules[0];
        assert_eq!(rule.span.source(), rule.when.span().source());
        assert_eq!(rule.span.source(), rule.effect.span().source());
        assert!(parsed.package.scenario.is_some());

        let invalid_form = valid.replace(
            "when:\n          predicate:",
            "when:\n          predicate:\n            operator: equals\n            left: { fact: account.status }\n            right: { literal: approved }\n          all:\n            - predicate:",
        );
        assert!(parser.parse(invalid_form.as_bytes()).is_err());

        let missing_binary = valid.replace("right: { literal: approved }", "");
        assert!(parser.parse(missing_binary.as_bytes()).is_err());

        let unary_with_right = valid.replace("operator: equals", "operator: exists");
        assert!(parser.parse(unary_with_right.as_bytes()).is_err());

        let bad_operand = valid.replace(
            "left: { fact: account.status }",
            "left: { fact: account.status, reserved: today }",
        );
        assert!(parser.parse(bad_operand.as_bytes()).is_err());

        let null_literal =
            valid.replace("right: { literal: approved }", "right: { literal: null }");
        let parsed_null = parser
            .parse(null_literal.as_bytes())
            .expect("null literal parse");
        let SourceCondition::Predicate(predicate) = &parsed_null.package.decisions[0].rules[0].when
        else {
            panic!("expected predicate");
        };
        let Some(SourceOperand::Literal(value)) = predicate.right.as_ref() else {
            panic!("expected right literal");
        };
        assert_eq!(value, "null");
    }
}
