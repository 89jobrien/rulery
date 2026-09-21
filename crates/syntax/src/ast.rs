//! Format-neutral authored source declarations.

use std::collections::BTreeMap;

use rulery_contracts::{
    DecisionId, FactPath, PackageId, QualifiedRuleId, SourceBundle, SourceFile, SourceId,
    SourceKey, SourceMap, SourcePath, Span, StableId, Version, VersionRequirement,
};
use thiserror::Error;

/// Parsed source package and retained source spans.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourcePackage {
    /// Package metadata.
    pub metadata: SourceMetadata,
    /// Semantic policy defaults and strategies.
    pub semantics: SourceSemantics,
    /// Imported package declarations.
    pub imports: Vec<SourceImport>,
    /// Declared decisions.
    pub decisions: Vec<SourceDecision>,
    /// Vocabulary declarations.
    pub vocabulary: SourceVocabulary,
    /// Action declarations.
    pub actions: Vec<SourceAction>,
    /// Optional scenario in the same source package.
    pub scenario: Option<SourceScenario>,
}

/// Source metadata fields.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceMetadata {
    /// Package identity.
    pub package_id: PackageId,
    /// Validated semantic version.
    pub version: Version,
    /// Optional display title.
    pub title: Option<String>,
}

/// Semantic defaults from authored source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceSemantics {
    /// Timezone name.
    pub timezone: String,
    /// Missing-fact strategy label.
    pub missing: String,
    /// Invalid-fact strategy label.
    pub invalid: String,
    /// Precedence strategy label.
    pub precedence: String,
}

/// Imported package declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceImport {
    /// Declared imported package identity.
    pub package: PackageId,
    /// Declared semantic-version requirement.
    pub version: VersionRequirement,
    /// Optional imported package alias.
    pub alias: Option<StableId>,
    /// Local source location.
    pub path: SourcePath,
}

/// Vocabulary declaration root.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SourceVocabulary {
    /// Declared root fact paths.
    pub roots: Vec<FactPath>,
    /// Operational terms.
    pub terms: Vec<String>,
}

/// Authored decision with one or more rules.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceDecision {
    /// Decision identity.
    pub id: DecisionId,
    /// Authored rules.
    pub rules: Vec<SourceRule>,
}

/// One authored rule.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceRule {
    /// Rule identity.
    pub id: StableId,
    /// Conditional expression.
    pub when: SourceCondition,
    /// Rule outcome.
    pub effect: SourceEffect,
    /// Rule span.
    pub span: Span,
}

/// Condition expression over facts and literals.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceCondition {
    /// All conditions must hold.
    All {
        /// Nested conditions.
        conditions: Vec<SourceCondition>,
        /// Source span.
        span: Span,
    },
    /// Any condition may hold.
    Any {
        /// Nested conditions.
        conditions: Vec<SourceCondition>,
        /// Source span.
        span: Span,
    },
    /// Logical negation.
    Not {
        /// Child condition.
        condition: Box<SourceCondition>,
        /// Source span.
        span: Span,
    },
    /// Predicate condition.
    Predicate(SourcePredicate),
}

impl SourceCondition {
    /// Returns this condition source span.
    #[must_use]
    pub const fn span(&self) -> Span {
        match self {
            Self::All { span, .. } | Self::Any { span, .. } | Self::Not { span, .. } => *span,
            Self::Predicate(predicate) => predicate.span,
        }
    }
}

/// Leaf predicate expression.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourcePredicate {
    /// Operator.
    pub operator: SourceOperator,
    /// Left operand.
    pub left: SourceOperand,
    /// Optional right operand.
    pub right: Option<SourceOperand>,
    /// Source span.
    pub span: Span,
}

/// Complete v0.1 source operator set.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(missing_docs)]
pub enum SourceOperator {
    Exists,
    Missing,
    Equals,
    NotEquals,
    LessThan,
    LessOrEqual,
    GreaterThan,
    GreaterOrEqual,
    IsOneOf,
    Contains,
    StartsWith,
    EndsWith,
    Matches,
    Before,
    After,
    Between,
    OnOrBefore,
    OnOrAfter,
    IsTrue,
    IsFalse,
}

impl SourceOperator {
    /// Returns all supported source operators.
    #[must_use]
    pub const fn all() -> [Self; 20] {
        [
            Self::Exists,
            Self::Missing,
            Self::Equals,
            Self::NotEquals,
            Self::LessThan,
            Self::LessOrEqual,
            Self::GreaterThan,
            Self::GreaterOrEqual,
            Self::IsOneOf,
            Self::Contains,
            Self::StartsWith,
            Self::EndsWith,
            Self::Matches,
            Self::Before,
            Self::After,
            Self::Between,
            Self::OnOrBefore,
            Self::OnOrAfter,
            Self::IsTrue,
            Self::IsFalse,
        ]
    }
}

/// Operand tagged at source level.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceOperand {
    /// Literal source value.
    Literal(String),
    /// Fact path lookup.
    Fact(FactPath),
    /// Reserved symbolic token.
    Reserved(String),
}

/// Authored rule effect.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceEffect {
    /// Approve decision.
    Approve {
        /// Rule reason codes.
        reasons: Vec<String>,
        /// Source span.
        span: Span,
    },
    /// Deny decision.
    Deny {
        /// Rule reason codes.
        reasons: Vec<String>,
        /// Source span.
        span: Span,
    },
    /// Escalate decision.
    Escalate {
        /// Escalation destination.
        to: String,
        /// Source span.
        span: Span,
    },
    /// Request more information.
    RequestInformation {
        /// Required facts.
        facts: Vec<FactPath>,
        /// Source span.
        span: Span,
    },
}

impl SourceEffect {
    /// Returns this effect source span.
    #[must_use]
    pub const fn span(&self) -> Span {
        match self {
            Self::Approve { span, .. }
            | Self::Deny { span, .. }
            | Self::Escalate { span, .. }
            | Self::RequestInformation { span, .. } => *span,
        }
    }
}

/// Action declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceAction {
    /// Action identity.
    pub id: StableId,
    /// Action parameters by name.
    pub parameters: BTreeMap<String, String>,
    /// Source span.
    pub span: Span,
}

/// Scenario declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceScenario {
    /// Scenario identity.
    pub id: StableId,
    /// Scenario title.
    pub title: String,
    /// Expected decisions.
    pub expectations: Vec<SourceExpectedDecision>,
    /// Source span.
    pub span: Span,
}

/// One expected decision in a scenario.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceExpectedDecision {
    /// Target decision.
    pub decision: DecisionId,
    /// Expected determining rules.
    pub determining_rules: Vec<QualifiedRuleId>,
    /// Source span.
    pub span: Span,
}

/// Parse result plus loaded source package.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedPackage {
    /// Parsed package model.
    pub package: SourcePackage,
    /// Parsed scenario declarations.
    pub scenarios: Vec<SourceScenario>,
    /// Package-local source map.
    pub source_map: SourceMap,
}

/// Source parser abstraction.
pub trait SourceParser {
    /// Parses source bytes into a validated source package model.
    ///
    /// # Errors
    ///
    /// Returns [`SourceParseError`] when syntax, shape, or resource validation fails.
    fn parse(&self, input: &[u8]) -> Result<ParsedPackage, SourceParseError>;

    /// Parses a deterministically ordered source bundle.
    ///
    /// # Errors
    ///
    /// Returns [`SourceParseError`] when the required manifest cannot be parsed.
    fn parse_bundle(&self, bundle: &SourceBundle) -> Result<ParsedPackage, SourceParseError> {
        let Some(manifest) = bundle
            .documents()
            .iter()
            .find(|document| document.path().as_str() == "rulery.yaml")
        else {
            return self.parse(&[]);
        };
        let mut parsed = self.parse(manifest.content().as_bytes())?;
        let mut source_map = SourceMap::new();
        source_map
            .insert(
                SourceKey::new(1),
                SourceFile::new(
                    SourceId::new("source.rulery").map_err(|_| SourceParseError {
                        kind: SourceParseErrorKind::Shape,
                        message: "invalid manifest source id".to_owned(),
                        span: parsed.package.actions.first().map_or_else(
                            || {
                                parsed.package.scenario.as_ref().map_or_else(
                                    || {
                                        parsed
                                            .source_map
                                            .span(SourceKey::new(1), 0, 0)
                                            .expect("parser source map")
                                    },
                                    |scenario| scenario.span,
                                )
                            },
                            |action| action.span,
                        ),
                    })?,
                    manifest.path().clone(),
                    std::sync::Arc::<str>::from(manifest.content()),
                ),
            )
            .map_err(|_| SourceParseError {
                kind: SourceParseErrorKind::Shape,
                message: "duplicate manifest source key".to_owned(),
                span: parsed
                    .source_map
                    .span(SourceKey::new(1), 0, 0)
                    .expect("parser source map"),
            })?;
        let mut next_key = 2_u32;
        for document in bundle
            .documents()
            .iter()
            .filter(|document| document.path().as_str() != "rulery.yaml")
        {
            source_map
                .insert(
                    SourceKey::new(next_key),
                    SourceFile::new(
                        SourceId::new(format!("source.bundle.{next_key}"))
                            .expect("generated source id is valid"),
                        document.path().clone(),
                        std::sync::Arc::<str>::from(document.content()),
                    ),
                )
                .expect("generated source key is unique");
            next_key = next_key.checked_add(1).expect("source bundle key overflow");
        }
        parsed.source_map = source_map;
        Ok(parsed)
    }
}

/// Source parse error.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("{kind:?} parse error: {message}")]
pub struct SourceParseError {
    /// Error class.
    pub kind: SourceParseErrorKind,
    /// Human-readable message.
    pub message: String,
    /// Source span.
    pub span: Span,
}

/// Parse error category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceParseErrorKind {
    /// Surface syntax failure.
    Syntax,
    /// Shape validation failure.
    Shape,
    /// Resource or complexity limit.
    ResourceLimit,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::str::FromStr;
    use std::sync::Arc;

    use rulery_contracts::{RuleId, SourceFile, SourceId, SourceKey, SourceMap, SourcePath};

    use super::*;

    #[test]
    fn source_ast_represents_every_authored_form() {
        let map = source_map();
        let s1 = map
            .span(SourceKey::new(1), 0, 1)
            .expect("valid primary span");
        let s2 = map
            .span(SourceKey::new(1), 1, 2)
            .expect("valid secondary span");

        let all_ops = SourceOperator::all();
        assert_eq!(all_ops.len(), 20);

        let literal = SourceOperand::Literal("literal".to_owned());
        let fact = SourceOperand::Fact(FactPath::from_str("account.state").expect("fact path"));
        let reserved = SourceOperand::Reserved("today".to_owned());

        let predicate = SourcePredicate {
            operator: SourceOperator::Equals,
            left: fact.clone(),
            right: Some(literal.clone()),
            span: s1,
        };
        let condition = SourceCondition::All {
            conditions: vec![
                SourceCondition::Any {
                    conditions: vec![SourceCondition::Predicate(predicate.clone())],
                    span: s1,
                },
                SourceCondition::Not {
                    condition: Box::new(SourceCondition::Predicate(SourcePredicate {
                        operator: SourceOperator::NotEquals,
                        left: fact,
                        right: Some(reserved),
                        span: s2,
                    })),
                    span: s2,
                },
            ],
            span: s1,
        };
        assert_eq!(condition.span(), s1);
        assert_eq!(predicate.span, s1);

        let approve = SourceEffect::Approve {
            reasons: vec!["ok".to_owned()],
            span: s1,
        };
        let deny = SourceEffect::Deny {
            reasons: vec!["no".to_owned()],
            span: s1,
        };
        let escalate = SourceEffect::Escalate {
            to: "review".to_owned(),
            span: s2,
        };
        let request = SourceEffect::RequestInformation {
            facts: vec![FactPath::from_str("account.id").expect("fact")],
            span: s2,
        };
        assert_eq!(approve.span(), s1);
        assert_eq!(deny.span(), s1);
        assert_eq!(escalate.span(), s2);
        assert_eq!(request.span(), s2);

        let decision = SourceDecision {
            id: DecisionId::new("decision.authz").expect("decision id"),
            rules: vec![SourceRule {
                id: StableId::new("rule.allow").expect("rule id"),
                when: condition,
                effect: approve,
                span: s1,
            }],
        };
        assert_eq!(decision.rules[0].span, s1);
        assert_eq!(decision.rules[0].effect.span(), s1);

        let scenario = SourceScenario {
            id: StableId::new("scenario.basic").expect("scenario id"),
            title: "basic".to_owned(),
            expectations: vec![SourceExpectedDecision {
                decision: DecisionId::new("decision.authz").expect("decision"),
                determining_rules: vec![QualifiedRuleId::new(
                    PackageId::new("pkg.main").expect("package"),
                    RuleId::new("rule.allow").expect("rule"),
                )],
                span: s1,
            }],
            span: s2,
        };
        assert_eq!(scenario.span, s2);
        assert_eq!(scenario.expectations[0].span, s1);

        let package = SourcePackage {
            metadata: SourceMetadata {
                package_id: PackageId::new("pkg.main").expect("package id"),
                version: Version::new("1.0.0").expect("version"),
                title: Some("main".to_owned()),
            },
            semantics: SourceSemantics {
                timezone: "UTC".to_owned(),
                missing: "unknown".to_owned(),
                invalid: "reject".to_owned(),
                precedence: "specificity".to_owned(),
            },
            imports: vec![SourceImport {
                package: PackageId::new("lib.core").expect("package"),
                version: VersionRequirement::new("*").expect("requirement"),
                alias: Some(StableId::new("lib.core").expect("alias")),
                path: SourcePath::new("imports/core/rulery.yaml").expect("path"),
            }],
            decisions: vec![decision],
            vocabulary: SourceVocabulary {
                roots: vec![FactPath::from_str("account").expect("root")],
                terms: vec!["term.is_verified".to_owned()],
            },
            actions: vec![SourceAction {
                id: StableId::new("action.notify").expect("action id"),
                parameters: BTreeMap::from([("channel".to_owned(), "email".to_owned())]),
                span: s1,
            }],
            scenario: Some(scenario),
        };

        let parsed = ParsedPackage {
            scenarios: package.scenario.iter().cloned().collect(),
            source_map: map,
            package,
        };
        assert_eq!(parsed.package.actions[0].span, s1);
        assert_eq!(parsed.package.scenario.expect("scenario").span, s2);
    }

    fn source_map() -> SourceMap {
        let mut map = SourceMap::new();
        map.insert(
            SourceKey::new(1),
            SourceFile::new(
                SourceId::new("src.main").expect("source id"),
                SourcePath::new("rulery.yaml").expect("source path"),
                Arc::<str>::from("abcdef"),
            ),
        )
        .expect("insert source");
        map
    }
}
