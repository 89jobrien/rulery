//! Strict, spanned YAML parser for complete authored source bundles.
#![allow(clippy::too_many_lines, clippy::wildcard_imports)]

mod dto;
mod span;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use rulery_contracts::{
    DecisionId, FactPath, PackageId, QualifiedRuleId, RuleId, SourceFile, SourceId, SourceKey,
    SourceMap, SourcePath, StableId, Version, VersionRequirement,
};

use self::dto::*;
use crate::ast::*;

const PRIMITIVES: &[&str] = &[
    "boolean",
    "integer",
    "decimal",
    "text",
    "date",
    "date-time",
    "duration",
];

/// YAML parser for the v0.1 authored package language.
#[derive(Clone, Copy, Debug, Default)]
pub struct YamlSourceParser;

impl SourceParser for YamlSourceParser {
    fn parse(&self, input: &[u8]) -> Result<ParsedPackage, SourceParseError> {
        let text = std::str::from_utf8(input).map_err(|_| {
            error_with_text(
                "rulery.yaml",
                "",
                SourceParseErrorKind::Syntax,
                "input is not valid UTF-8",
                "",
            )
        })?;
        Err(error_with_text(
            "rulery.yaml",
            text,
            SourceParseErrorKind::Shape,
            "a complete source bundle is required",
            "rulery.yaml",
        ))
    }

    fn parse_bundle(
        &self,
        bundle: &rulery_contracts::SourceBundle,
    ) -> Result<ParsedPackage, SourceParseError> {
        let map = map_for_bundle(bundle);
        let documents = bundle.documents();
        let manifest = document(documents, "rulery.yaml", &map)?;
        let vocabulary = document(documents, "vocabulary.yaml", &map)?;
        let actions = document(documents, "actions.yaml", &map)?;
        let manifest_dto: ManifestDto = decode(manifest.0, manifest.1, "manifest")?;
        let vocabulary_dto: VocabularyDto = decode(vocabulary.0, vocabulary.1, "vocabulary")?;
        let actions_dto: ActionsDto = decode(actions.0, actions.1, "actions")?;
        reject_extra_documents(documents, &map)?;

        let metadata = build_metadata(&map, manifest.0, manifest.1, manifest_dto.package)?;
        let semantics = build_semantics(&map, manifest.0, manifest.1, manifest_dto.semantics)?;
        let imports = build_imports(&map, manifest.0, manifest.1, manifest_dto.imports)?;
        let mut decisions = build_decisions(&map, manifest.0, manifest.1, manifest_dto.decisions)?;
        let vocabulary = build_vocabulary(&map, vocabulary.0, vocabulary.1, vocabulary_dto)?;
        let actions = build_actions(&map, actions.0, actions.1, actions_dto)?;
        let mut scenarios = Vec::new();

        let mut decision_indexes = BTreeMap::new();
        for (index, decision) in decisions.iter().enumerate() {
            if decision_indexes
                .insert(decision.id.as_str().to_owned(), index)
                .is_some()
            {
                return Err(err(
                    &map,
                    manifest.0,
                    manifest.1,
                    "decisions",
                    "decision IDs must be unique",
                ));
            }
        }
        let mut rule_ids = BTreeSet::new();
        for item in documents
            .iter()
            .filter(|item| item.path().as_str().starts_with("rules/"))
        {
            let (key, text) =
                locate(&map, item.path().as_str()).expect("bundle document has source map entry");
            let rules: RulesDto = decode(key, text, "rule file")?;
            let index = *decision_indexes.get(&rules.decision).ok_or_else(|| {
                err(
                    &map,
                    key,
                    text,
                    "decision",
                    "rule file names an undeclared decision",
                )
            })?;
            for rule in rules.rules {
                let built = build_rule(&map, key, text, rule)?;
                if !rule_ids.insert(built.id.as_str().to_owned()) {
                    return Err(err(
                        &map,
                        key,
                        text,
                        "id",
                        "rule IDs must be unique across the package",
                    ));
                }
                decisions[index].rules.push(built);
            }
        }
        let mut scenario_ids = BTreeSet::new();
        for item in documents
            .iter()
            .filter(|item| item.path().as_str().starts_with("scenarios/"))
        {
            let (key, text) =
                locate(&map, item.path().as_str()).expect("bundle document has source map entry");
            let scenario: ScenarioDto = decode(key, text, "scenario")?;
            let built = build_scenario(&map, key, text, &metadata.package_id, scenario)?;
            if !scenario_ids.insert(built.id.as_str().to_owned()) {
                return Err(err(&map, key, text, "id", "scenario IDs must be unique"));
            }
            scenarios.push(built);
        }
        Ok(ParsedPackage {
            package: SourcePackage {
                metadata,
                semantics,
                imports,
                decisions,
                vocabulary,
                actions,
                scenario: scenarios.first().cloned(),
            },
            scenarios,
            source_map: map,
        })
    }
}

fn document<'a>(
    documents: &'a [rulery_contracts::SourceDocument],
    path: &str,
    map: &'a SourceMap,
) -> Result<(SourceKey, &'a str), SourceParseError> {
    documents
        .iter()
        .find(|item| item.path().as_str() == path)
        .and_then(|item| locate(map, item.path().as_str()))
        .ok_or_else(|| {
            error_with_text(
                path,
                "",
                SourceParseErrorKind::Shape,
                &format!("required document `{path}` is missing"),
                "",
            )
        })
}

fn locate<'a>(map: &'a SourceMap, path: &str) -> Option<(SourceKey, &'a str)> {
    map.iter()
        .find(|(_, file)| file.path().as_str() == path)
        .map(|(key, file)| (key, file.content()))
}

fn reject_extra_documents(
    documents: &[rulery_contracts::SourceDocument],
    map: &SourceMap,
) -> Result<(), SourceParseError> {
    for item in documents {
        let path = item.path().as_str();
        if path != "rulery.yaml"
            && path != "vocabulary.yaml"
            && path != "actions.yaml"
            && !path.starts_with("rules/")
            && !path.starts_with("scenarios/")
        {
            let (key, text) = locate(map, path).expect("source map contains document");
            return Err(err(map, key, text, path, "unexpected authored document"));
        }
        if (path.starts_with("rules/") || path.starts_with("scenarios/"))
            && !Path::new(path)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("yaml"))
        {
            let (key, text) = locate(map, path).expect("source map contains document");
            return Err(err(
                map,
                key,
                text,
                path,
                "authored directory documents must be YAML",
            ));
        }
    }
    Ok(())
}

fn decode<T: serde::de::DeserializeOwned>(
    key: SourceKey,
    text: &str,
    name: &str,
) -> Result<T, SourceParseError> {
    let map = one_file_map(key, text);
    reject_forbidden(&map, key, text)?;
    serde_yaml::from_str(text)
        .map_err(|_| err(&map, key, text, name, &format!("{name} shape is invalid")))
}

fn one_file_map(key: SourceKey, text: &str) -> SourceMap {
    let mut map = SourceMap::new();
    map.insert(
        key,
        SourceFile::new(
            SourceId::new(format!("source.{}", key.get())).expect("valid generated id"),
            SourcePath::new("rulery.yaml").expect("valid synthetic path"),
            Arc::from(text),
        ),
    )
    .expect("unique source key");
    map
}

fn build_metadata(
    map: &SourceMap,
    key: SourceKey,
    text: &str,
    dto: PackageDto,
) -> Result<SourceMetadata, SourceParseError> {
    if dto.language_version != 1 || empty(&dto.display_name) {
        return Err(err(
            map,
            key,
            text,
            "package",
            "package metadata is invalid",
        ));
    }
    Ok(SourceMetadata {
        package_id: PackageId::new(dto.id)
            .map_err(|_| err(map, key, text, "id", "package.id is invalid"))?,
        display_name: dto.display_name,
        version: Version::new(dto.version)
            .map_err(|_| err(map, key, text, "version", "package.version is invalid"))?,
        language_version: dto.language_version,
        description: nonempty_opt(dto.description, map, key, text, "description")?,
        authors: dto
            .authors
            .into_iter()
            .map(|a| {
                Ok(SourceAuthor {
                    name: required(a.name, map, key, text, "authors")?,
                    contact: nonempty_opt(a.contact, map, key, text, "contact")?,
                })
            })
            .collect::<Result<_, _>>()?,
        tags: stable_set(dto.tags, map, key, text, "tags")?,
    })
}

fn build_semantics(
    map: &SourceMap,
    key: SourceKey,
    text: &str,
    dto: SemanticsDto,
) -> Result<SourceSemantics, SourceParseError> {
    if empty(&dto.timezone) || !matches!(dto.expiry.as_str(), "inclusive" | "exclusive") {
        return Err(err(map, key, text, "semantics", "semantics is invalid"));
    }
    let missing = strategy(
        dto.missing_facts,
        &[
            "preserve_unknown",
            "closed_world_false",
            "request_information",
            "escalate",
        ],
        map,
        key,
        text,
    )?;
    let invalid = strategy(
        dto.invalid_facts,
        &["reject_evaluation", "preserve_invalid", "escalate"],
        map,
        key,
        text,
    )?;
    if !matches!(
        dto.precedence.kind.as_str(),
        "safety_first" | "priority_first" | "explicit"
    ) {
        return Err(err(
            map,
            key,
            text,
            "precedence",
            "precedence kind is invalid",
        ));
    }
    if dto.precedence.kind == "explicit"
        && (dto.precedence.primary.is_none() || dto.precedence.outcome_ranks.len() != 4)
    {
        return Err(err(
            map,
            key,
            text,
            "precedence",
            "explicit precedence is incomplete",
        ));
    }
    Ok(SourceSemantics {
        timezone: dto.timezone,
        expiry: dto.expiry,
        missing_facts: missing,
        invalid_facts: invalid,
        precedence: SourcePrecedence {
            kind: dto.precedence.kind,
            primary: dto.precedence.primary,
            outcome_ranks: dto.precedence.outcome_ranks,
        },
    })
}

fn strategy(
    dto: StrategyDto,
    allowed: &[&str],
    map: &SourceMap,
    key: SourceKey,
    text: &str,
) -> Result<SourceStrategy, SourceParseError> {
    if !allowed.contains(&dto.kind.as_str())
        || (dto.kind == "escalate") != dto.destination.is_some()
    {
        return Err(err(map, key, text, "kind", "strategy is invalid"));
    }
    Ok(SourceStrategy {
        kind: dto.kind,
        destination: dto.destination,
    })
}

fn build_imports(
    map: &SourceMap,
    key: SourceKey,
    text: &str,
    imports: Vec<ImportDto>,
) -> Result<Vec<SourceImport>, SourceParseError> {
    let mut aliases = BTreeSet::new();
    imports
        .into_iter()
        .map(|i| {
            let alias = i
                .alias
                .map(|value| {
                    StableId::new(value)
                        .map_err(|_| err(map, key, text, "alias", "import alias is invalid"))
                })
                .transpose()?;
            if let Some(value) = &alias
                && !aliases.insert(value.as_str().to_owned())
            {
                return Err(err(
                    map,
                    key,
                    text,
                    "alias",
                    "import aliases must be unique",
                ));
            }
            Ok(SourceImport {
                package: PackageId::new(i.package)
                    .map_err(|_| err(map, key, text, "package", "import package is invalid"))?,
                version: VersionRequirement::new(i.version)
                    .map_err(|_| err(map, key, text, "version", "import version is invalid"))?,
                path: SourcePath::new(i.path)
                    .map_err(|_| err(map, key, text, "path", "import path is invalid"))?,
                alias,
            })
        })
        .collect()
}

fn build_decisions(
    map: &SourceMap,
    key: SourceKey,
    text: &str,
    values: Vec<DecisionDto>,
) -> Result<Vec<SourceDecision>, SourceParseError> {
    if values.is_empty() {
        return Err(err(
            map,
            key,
            text,
            "decisions",
            "decisions must not be empty",
        ));
    }
    values
        .into_iter()
        .map(|d| {
            Ok(SourceDecision {
                id: DecisionId::new(d.id)
                    .map_err(|_| err(map, key, text, "id", "decision id is invalid"))?,
                title: required(d.title, map, key, text, "title")?,
                asks: required(d.asks, map, key, text, "asks")?,
                input_roots: stable_set_nonempty(d.input_roots, map, key, text, "input_roots")?,
                default: build_outcome(map, key, text, d.default)?,
                rules: Vec::new(),
                span: span(map, key, text, "decisions"),
            })
        })
        .collect()
}

fn build_vocabulary(
    map: &SourceMap,
    key: SourceKey,
    text: &str,
    dto: VocabularyDto,
) -> Result<SourceVocabulary, SourceParseError> {
    if dto.roots.is_empty() {
        return Err(err(
            map,
            key,
            text,
            "roots",
            "vocabulary roots must not be empty",
        ));
    }
    let roots = dto
        .roots
        .into_iter()
        .map(|(id, r)| {
            Ok((
                stable(id, map, key, text, "roots")?,
                SourceRoot {
                    type_id: required(r.type_id, map, key, text, "type")?,
                    description: nonempty_opt(r.description, map, key, text, "description")?,
                    span: span(map, key, text, "roots"),
                },
            ))
        })
        .collect::<Result<_, _>>()?;
    let types = dto
        .types
        .into_iter()
        .map(|(id, t)| {
            if PRIMITIVES.contains(&id.as_str())
                || !matches!(t.kind.as_str(), "enum" | "record" | "list")
                || (t.kind == "enum" && t.variants.is_empty())
                || (t.kind == "list" && t.items.is_none())
                || t.min_items.zip(t.max_items).is_some_and(|(a, b)| a > b)
            {
                return Err(err(map, key, text, "types", "type declaration is invalid"));
            }
            let variants = t
                .variants
                .into_iter()
                .map(|(n, v)| {
                    Ok((
                        stable(n, map, key, text, "variants")?,
                        SourceVariant {
                            display_name: required(v.display_name, map, key, text, "display_name")?,
                            description: nonempty_opt(
                                v.description,
                                map,
                                key,
                                text,
                                "description",
                            )?,
                            deprecated: v.deprecated,
                            span: span(map, key, text, "variants"),
                        },
                    ))
                })
                .collect::<Result<_, _>>()?;
            let fields = t
                .fields
                .into_iter()
                .map(|(n, f)| {
                    if !matches!(f.presence.as_str(), "required" | "optional" | "derived") {
                        return Err(err(map, key, text, "presence", "field presence is invalid"));
                    }
                    Ok((
                        stable(n, map, key, text, "fields")?,
                        SourceField {
                            type_id: required(f.type_id, map, key, text, "type")?,
                            presence: f.presence,
                            description: nonempty_opt(
                                f.description,
                                map,
                                key,
                                text,
                                "description",
                            )?,
                            span: span(map, key, text, "fields"),
                        },
                    ))
                })
                .collect::<Result<_, _>>()?;
            Ok((
                stable(id, map, key, text, "types")?,
                SourceType {
                    kind: t.kind,
                    variants,
                    closed: t.closed,
                    fields,
                    items: t.items,
                    min_items: t.min_items,
                    max_items: t.max_items,
                    span: span(map, key, text, "types"),
                },
            ))
        })
        .collect::<Result<_, _>>()?;
    let terms = dto
        .terms
        .into_iter()
        .map(|(id, t)| {
            Ok((
                stable(id, map, key, text, "terms")?,
                SourceTerm {
                    display_name: required(t.display_name, map, key, text, "display_name")?,
                    definition: required(t.definition, map, key, text, "definition")?,
                    applies_to: fact_set_nonempty(t.applies_to, map, key, text, "applies_to")?,
                    examples: t.examples,
                    counterexamples: t.counterexamples,
                    span: span(map, key, text, "terms"),
                },
            ))
        })
        .collect::<Result<_, _>>()?;
    Ok(SourceVocabulary {
        roots,
        types,
        terms,
    })
}

fn build_actions(
    map: &SourceMap,
    key: SourceKey,
    text: &str,
    dto: ActionsDto,
) -> Result<Vec<SourceAction>, SourceParseError> {
    dto.actions
        .into_iter()
        .map(|(id, a)| {
            let parameters = a
                .parameters
                .into_iter()
                .map(|(id, p)| {
                    Ok((
                        stable(id, map, key, text, "parameters")?,
                        SourceActionParameter {
                            type_id: required(p.type_id, map, key, text, "type")?,
                            required: p.required,
                            description: nonempty_opt(
                                p.description,
                                map,
                                key,
                                text,
                                "description",
                            )?,
                            span: span(map, key, text, "parameters"),
                        },
                    ))
                })
                .collect::<Result<_, _>>()?;
            Ok(SourceAction {
                id: stable(id, map, key, text, "actions")?,
                display_name: required(a.display_name, map, key, text, "display_name")?,
                description: nonempty_opt(a.description, map, key, text, "description")?,
                parameters,
                span: span(map, key, text, "actions"),
            })
        })
        .collect()
}

fn build_rule(
    map: &SourceMap,
    key: SourceKey,
    text: &str,
    r: RuleDto,
) -> Result<SourceRule, SourceParseError> {
    if r.explicit_override && r.rationale.as_deref().is_none_or(str::is_empty) {
        return Err(err(
            map,
            key,
            text,
            "rationale",
            "override requires rationale",
        ));
    }
    Ok(SourceRule {
        id: stable(r.id, map, key, text, "id")?,
        title: nonempty_opt(r.title, map, key, text, "title")?,
        priority: r.priority,
        when: condition(map, key, text, &r.when)?,
        effect: build_outcome(map, key, text, r.effect)?,
        explicit_override: r.explicit_override,
        rationale: r.rationale,
        span: span(map, key, text, "rules"),
    })
}

fn condition(
    map: &SourceMap,
    key: SourceKey,
    text: &str,
    node: &serde_yaml::Value,
) -> Result<SourceCondition, SourceParseError> {
    let mapping = node
        .as_mapping()
        .ok_or_else(|| err(map, key, text, "when", "condition must be a mapping"))?;
    let get = |name: &str| mapping.get(serde_yaml::Value::String(name.to_owned()));
    let forms = ["all", "any", "not"]
        .into_iter()
        .filter(|name| get(name).is_some())
        .count();
    if forms > 1 || (forms == 1 && mapping.len() != 1) {
        return Err(err(
            map,
            key,
            text,
            "when",
            "condition must have exactly one form",
        ));
    }
    let at = span(map, key, text, "when");
    if let Some(v) = get("all") {
        return Ok(SourceCondition::All {
            conditions: v
                .as_sequence()
                .ok_or_else(|| err(map, key, text, "all", "all must be a sequence"))?
                .iter()
                .map(|v| condition(map, key, text, v))
                .collect::<Result<_, _>>()?,
            span: at,
        });
    }
    if let Some(v) = get("any") {
        return Ok(SourceCondition::Any {
            conditions: v
                .as_sequence()
                .ok_or_else(|| err(map, key, text, "any", "any must be a sequence"))?
                .iter()
                .map(|v| condition(map, key, text, v))
                .collect::<Result<_, _>>()?,
            span: at,
        });
    }
    if let Some(v) = get("not") {
        return Ok(SourceCondition::Not {
            condition: Box::new(condition(map, key, text, v)?),
            span: at,
        });
    }
    if forms == 0
        && mapping
            .keys()
            .any(|key| !matches!(key.as_str(), Some("fact" | "operator" | "value")))
    {
        return Err(err(map, key, text, "when", "predicate fields are invalid"));
    }
    let fact = string_field(mapping, "fact", map, key, text)?;
    let op = string_field(mapping, "operator", map, key, text)?;
    let operator =
        operator(&op).ok_or_else(|| err(map, key, text, "operator", "operator is invalid"))?;
    let value = get("value")
        .map(|v| operand(map, key, text, v))
        .transpose()?;
    if is_unary(&operator) != value.is_none() {
        return Err(err(
            map,
            key,
            text,
            "value",
            "operator value arity is invalid",
        ));
    }
    Ok(SourceCondition::Predicate(SourcePredicate {
        fact: FactPath::from_str(&fact)
            .map_err(|_| err(map, key, text, "fact", "fact path is invalid"))?,
        operator,
        value,
        span: at,
    }))
}

fn operator(value: &str) -> Option<SourceOperator> {
    Some(match value {
        "equal" => SourceOperator::Equal,
        "not_equal" => SourceOperator::NotEqual,
        "less_than" => SourceOperator::LessThan,
        "less_than_or_equal" => SourceOperator::LessThanOrEqual,
        "greater_than" => SourceOperator::GreaterThan,
        "greater_than_or_equal" => SourceOperator::GreaterThanOrEqual,
        "contains" => SourceOperator::Contains,
        "not_contains" => SourceOperator::NotContains,
        "starts_with" => SourceOperator::StartsWith,
        "ends_with" => SourceOperator::EndsWith,
        "is_one_of" => SourceOperator::IsOneOf,
        "is_absent" => SourceOperator::IsAbsent,
        "is_present" => SourceOperator::IsPresent,
        "is_valid" => SourceOperator::IsValid,
        "is_invalid" => SourceOperator::IsInvalid,
        "before" => SourceOperator::Before,
        "on_or_after" => SourceOperator::OnOrAfter,
        "is_expired" => SourceOperator::IsExpired,
        "is_unexpired" => SourceOperator::IsUnexpired,
        _ => return None,
    })
}
fn is_unary(op: &SourceOperator) -> bool {
    matches!(
        op,
        SourceOperator::IsAbsent
            | SourceOperator::IsPresent
            | SourceOperator::IsValid
            | SourceOperator::IsInvalid
            | SourceOperator::IsExpired
            | SourceOperator::IsUnexpired
    )
}

fn build_outcome(
    map: &SourceMap,
    key: SourceKey,
    text: &str,
    o: OutcomeDto,
) -> Result<SourceOutcome, SourceParseError> {
    if !matches!(
        o.kind.as_str(),
        "approve" | "deny" | "escalate" | "request_information"
    ) || o.reasons.is_empty()
        || (o.kind == "escalate") != o.destination.is_some()
        || (o.kind == "request_information") == o.required_facts.is_empty()
    {
        return Err(err(map, key, text, "effect", "outcome is invalid"));
    }
    Ok(SourceOutcome {
        kind: o.kind,
        reasons: o
            .reasons
            .into_iter()
            .map(|r| {
                Ok(SourceReason {
                    code: stable(r.code, map, key, text, "code")?,
                    message: required(r.message, map, key, text, "message")?,
                    detail: nonempty_opt(r.detail, map, key, text, "detail")?,
                    span: span(map, key, text, "reasons"),
                })
            })
            .collect::<Result<_, _>>()?,
        actions: o
            .actions
            .into_iter()
            .map(|a| {
                Ok(SourceActionInvocation {
                    action: stable(a.action, map, key, text, "action")?,
                    arguments: a
                        .arguments
                        .into_iter()
                        .map(|(name, value)| {
                            Ok((
                                stable(name, map, key, text, "arguments")?,
                                operand(map, key, text, &value)?,
                            ))
                        })
                        .collect::<Result<_, _>>()?,
                    span: span(map, key, text, "actions"),
                })
            })
            .collect::<Result<_, _>>()?,
        destination: o.destination,
        required_facts: fact_set(o.required_facts, map, key, text, "required_facts")?,
        span: span(map, key, text, "effect"),
    })
}

fn build_scenario(
    map: &SourceMap,
    key: SourceKey,
    text: &str,
    _package: &PackageId,
    s: ScenarioDto,
) -> Result<SourceScenario, SourceParseError> {
    let determining_rules = s
        .expect
        .determining_rules
        .into_iter()
        .map(|r| {
            Ok(QualifiedRuleId::new(
                PackageId::new(r.package).map_err(|_| {
                    err(
                        map,
                        key,
                        text,
                        "package",
                        "qualified rule package is invalid",
                    )
                })?,
                RuleId::new(r.rule)
                    .map_err(|_| err(map, key, text, "rule", "qualified rule is invalid"))?,
            ))
        })
        .collect::<Result<_, _>>()?;
    Ok(SourceScenario {
        id: stable(s.id, map, key, text, "id")?,
        title: required(s.title, map, key, text, "title")?,
        description: nonempty_opt(s.description, map, key, text, "description")?,
        decision: DecisionId::new(s.decision)
            .map_err(|_| err(map, key, text, "decision", "scenario decision is invalid"))?,
        at: required(s.at, map, key, text, "at")?,
        given: s
            .given
            .into_iter()
            .map(|(name, value)| {
                Ok((
                    stable(name, map, key, text, "given")?,
                    operand(map, key, text, &value)?,
                ))
            })
            .collect::<Result<_, _>>()?,
        expect: SourceExpectedDecision {
            outcome: s.expect.outcome,
            determining_rules,
            required_facts: fact_set(s.expect.required_facts, map, key, text, "required_facts")?,
            reason_codes: stable_set(s.expect.reason_codes, map, key, text, "reason_codes")?,
            span: span(map, key, text, "expect"),
        },
        tags: stable_set(s.tags, map, key, text, "tags")?,
        span: span(map, key, text, "scenario"),
    })
}

fn operand(
    map: &SourceMap,
    key: SourceKey,
    text: &str,
    value: &serde_yaml::Value,
) -> Result<SourceOperand, SourceParseError> {
    if let Some(m) = value.as_mapping()
        && m.len() == 1
    {
        for (k, v) in m {
            match k.as_str() {
                Some("fact") => {
                    return FactPath::from_str(v.as_str().ok_or_else(|| {
                        err(map, key, text, "fact", "fact operand must be text")
                    })?)
                    .map(SourceOperand::Fact)
                    .map_err(|_| err(map, key, text, "fact", "fact operand is invalid"));
                }
                Some("reserved") => {
                    let v = v.as_str().ok_or_else(|| {
                        err(map, key, text, "reserved", "reserved operand must be text")
                    })?;
                    if !matches!(v, "today" | "now") {
                        return Err(err(
                            map,
                            key,
                            text,
                            "reserved",
                            "reserved operand is invalid",
                        ));
                    }
                    return Ok(SourceOperand::Reserved(v.to_owned()));
                }
                Some("literal") => {
                    return authored(map, key, text, "literal", v).map(SourceOperand::Literal);
                }
                _ => {}
            }
        }
    }
    authored(map, key, text, "operand", value).map(SourceOperand::Literal)
}

/// Converts one authored YAML value into a structure-preserving [`SourceValue`].
fn authored(
    map: &SourceMap,
    key: SourceKey,
    text: &str,
    field: &str,
    value: &serde_yaml::Value,
) -> Result<SourceValue, SourceParseError> {
    if value.is_null() {
        return Ok(SourceValue::Null);
    }
    if let Some(sequence) = value.as_sequence() {
        return sequence
            .iter()
            .map(|item| authored(map, key, text, field, item))
            .collect::<Result<Vec<_>, _>>()
            .map(SourceValue::Sequence);
    }
    if let Some(mapping) = value.as_mapping() {
        return mapping
            .iter()
            .map(|(name, item)| {
                let name = name.as_str().ok_or_else(|| {
                    err(map, key, text, field, "literal mapping keys must be text")
                })?;
                let name = StableId::new(name)
                    .map_err(|_| err(map, key, text, field, "literal mapping key is invalid"))?;
                authored(map, key, text, field, item).map(|item| (name, item))
            })
            .collect::<Result<BTreeMap<_, _>, SourceParseError>>()
            .map(SourceValue::Mapping);
    }
    let scalar = match value {
        serde_yaml::Value::Bool(inner) => inner.to_string(),
        serde_yaml::Value::Number(inner) => inner.to_string(),
        serde_yaml::Value::String(inner) => inner.clone(),
        _ => {
            return Err(err(
                map,
                key,
                text,
                field,
                "literal must be a scalar, sequence, or mapping",
            ));
        }
    };
    Ok(SourceValue::Scalar(scalar))
}

fn stable(
    value: String,
    map: &SourceMap,
    key: SourceKey,
    text: &str,
    field: &str,
) -> Result<StableId, SourceParseError> {
    StableId::new(value).map_err(|_| err(map, key, text, field, "stable ID is invalid"))
}
fn stable_set(
    values: Vec<String>,
    map: &SourceMap,
    key: SourceKey,
    text: &str,
    field: &str,
) -> Result<BTreeSet<StableId>, SourceParseError> {
    values
        .into_iter()
        .map(|v| stable(v, map, key, text, field))
        .collect()
}
fn stable_set_nonempty(
    values: Vec<String>,
    map: &SourceMap,
    key: SourceKey,
    text: &str,
    field: &str,
) -> Result<BTreeSet<StableId>, SourceParseError> {
    let set = stable_set(values, map, key, text, field)?;
    if set.is_empty() {
        Err(err(map, key, text, field, "set must not be empty"))
    } else {
        Ok(set)
    }
}
fn fact_set(
    values: Vec<String>,
    map: &SourceMap,
    key: SourceKey,
    text: &str,
    field: &str,
) -> Result<BTreeSet<FactPath>, SourceParseError> {
    values
        .into_iter()
        .map(|v| {
            FactPath::from_str(&v).map_err(|_| err(map, key, text, field, "fact path is invalid"))
        })
        .collect()
}
fn fact_set_nonempty(
    values: Vec<String>,
    map: &SourceMap,
    key: SourceKey,
    text: &str,
    field: &str,
) -> Result<BTreeSet<FactPath>, SourceParseError> {
    let set = fact_set(values, map, key, text, field)?;
    if set.is_empty() {
        Err(err(map, key, text, field, "set must not be empty"))
    } else {
        Ok(set)
    }
}
fn required(
    value: String,
    map: &SourceMap,
    key: SourceKey,
    text: &str,
    field: &str,
) -> Result<String, SourceParseError> {
    if empty(&value) {
        Err(err(map, key, text, field, "text must not be empty"))
    } else {
        Ok(value)
    }
}
fn nonempty_opt(
    value: Option<String>,
    map: &SourceMap,
    key: SourceKey,
    text: &str,
    field: &str,
) -> Result<Option<String>, SourceParseError> {
    value
        .map(|v| required(v, map, key, text, field))
        .transpose()
}
fn empty(value: &str) -> bool {
    value.trim().is_empty()
}
fn string_field(
    mapv: &serde_yaml::Mapping,
    name: &str,
    map: &SourceMap,
    key: SourceKey,
    text: &str,
) -> Result<String, SourceParseError> {
    mapv.get(serde_yaml::Value::String(name.to_owned()))
        .and_then(serde_yaml::Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| err(map, key, text, name, &format!("{name} is required text")))
}
fn span(map: &SourceMap, key: SourceKey, text: &str, needle: &str) -> rulery_contracts::Span {
    span::span_for(map, key, text, needle)
}
fn err(
    map: &SourceMap,
    key: SourceKey,
    text: &str,
    needle: &str,
    message: &str,
) -> SourceParseError {
    SourceParseError {
        kind: SourceParseErrorKind::Shape,
        message: message.to_owned(),
        span: span(map, key, text, needle),
    }
}
fn error_with_text(
    path: &str,
    text: &str,
    kind: SourceParseErrorKind,
    message: &str,
    needle: &str,
) -> SourceParseError {
    let mut map = SourceMap::new();
    map.insert(
        SourceKey::new(1),
        SourceFile::new(
            SourceId::new("source.rulery").expect("valid source ID"),
            SourcePath::new(path)
                .unwrap_or_else(|_| SourcePath::new("rulery.yaml").expect("valid path")),
            Arc::from(text),
        ),
    )
    .expect("unique source");
    SourceParseError {
        kind,
        message: message.to_owned(),
        span: span(&map, SourceKey::new(1), text, needle),
    }
}
fn reject_forbidden(map: &SourceMap, key: SourceKey, text: &str) -> Result<(), SourceParseError> {
    for (needle, message) in [
        ("&", "YAML anchors are forbidden"),
        ("*", "YAML aliases are forbidden"),
        ("!", "YAML tags are forbidden"),
    ] {
        if contains_yaml_marker(text, needle) {
            return Err(err(map, key, text, needle, message));
        }
    }
    if text
        .lines()
        .any(|line| line.trim_start().starts_with("<<:"))
    {
        return Err(err(map, key, text, "<<:", "YAML merge keys are forbidden"));
    }
    Ok(())
}

fn contains_yaml_marker(text: &str, marker: &str) -> bool {
    text.match_indices(marker).any(|(index, _)| {
        text[..index].chars().next_back().is_none_or(|previous| {
            previous.is_whitespace() || matches!(previous, ':' | '[' | '{' | ',')
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rulery_contracts::{SourceBundle, SourceDocument, SourcePath};
    use std::sync::Arc;
    fn document(path: &str, content: &str) -> SourceDocument {
        SourceDocument::new(SourcePath::new(path).expect("path"), Arc::from(content))
    }
    fn bundle(actions: &str) -> SourceBundle {
        SourceBundle::new(vec![
            document(
                "rulery.yaml",
                include_str!("../../../../examples/tool-library/rulery.yaml"),
            ),
            document(
                "vocabulary.yaml",
                include_str!("../../../../examples/tool-library/vocabulary.yaml"),
            ),
            document("actions.yaml", actions),
            document(
                "rules/checkout.yaml",
                include_str!("../../../../examples/tool-library/rules/checkout.yaml"),
            ),
            document(
                "scenarios/expired-training-is-denied.yaml",
                include_str!(
                    "../../../../examples/tool-library/scenarios/expired-training-is-denied.yaml"
                ),
            ),
        ])
        .expect("bundle")
    }
    #[test]
    fn parser_merges_complete_authored_package_bundle() {
        let parsed = YamlSourceParser
            .parse_bundle(&bundle(include_str!(
                "../../../../examples/tool-library/actions.yaml"
            )))
            .expect("complete authored package parses");
        assert_eq!(parsed.package.decisions.len(), 1);
        assert_eq!(parsed.package.decisions[0].rules.len(), 4);
        assert_eq!(parsed.package.actions.len(), 2);
        assert_eq!(parsed.scenarios.len(), 1);
        assert_eq!(parsed.source_map.len(), 5);
        assert_eq!(parsed.package.decisions[0].rules[0].span.source().get(), 3);
        assert_eq!(parsed.package.actions[0].span.source().get(), 1);
    }
    #[test]
    fn parser_rejects_malformed_external_document() {
        let error = YamlSourceParser
            .parse_bundle(&bundle(
                "actions:\n  invalid:\n    display_name: Bad\n    unknown: true\n",
            ))
            .expect_err("unknown action field");
        assert_eq!(error.kind, SourceParseErrorKind::Shape);
    }
    #[test]
    fn parser_rejects_duplicate_external_declarations() {
        let rules = format!(
            "{}\n  - id: deny-expired-training\n    priority: 1\n    when: {{ fact: member.account-status, operator: equal, value: active }}\n    effect: {{ kind: deny, reasons: [{{ code: duplicate, message: Duplicate }}] }}\n",
            include_str!("../../../../examples/tool-library/rules/checkout.yaml")
        );
        let source = SourceBundle::new(vec![
            document(
                "rulery.yaml",
                include_str!("../../../../examples/tool-library/rulery.yaml"),
            ),
            document(
                "vocabulary.yaml",
                include_str!("../../../../examples/tool-library/vocabulary.yaml"),
            ),
            document(
                "actions.yaml",
                include_str!("../../../../examples/tool-library/actions.yaml"),
            ),
            document("rules/checkout.yaml", &rules),
            document(
                "scenarios/expired-training-is-denied.yaml",
                include_str!(
                    "../../../../examples/tool-library/scenarios/expired-training-is-denied.yaml"
                ),
            ),
        ])
        .expect("bundle");
        let error = YamlSourceParser
            .parse_bundle(&source)
            .expect_err("duplicate rule id");
        assert_eq!(error.kind, SourceParseErrorKind::Shape);
    }
}
