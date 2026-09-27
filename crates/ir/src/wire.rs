//! Versioned compiled package wire envelopes.

use serde::{Deserialize, Serialize};

use crate::CompiledPackageV1;

/// Versioned compiled package envelope.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "schema", content = "payload", deny_unknown_fields)]
pub enum CompiledPackageEnvelope {
    /// Compiled package schema version 1.
    #[serde(rename = "rulery.compiled-package/v1")]
    V1(CompiledPackageV1),
}

impl CompiledPackageEnvelope {
    /// Returns the read-only payload.
    #[must_use]
    pub const fn payload(&self) -> &CompiledPackageV1 {
        match self {
            Self::V1(payload) => payload,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::str::FromStr;
    use std::sync::Arc;

    use rulery_contracts::{
        ActionId, ContentHash, DecisionId, FactPath, LanguageVersion, OutcomeTemplate, PackageId,
        PolicyTimeZone, QualifiedRuleId, Reason, ReasonCode, Reasons, RuleId, SourceFile, SourceId,
        SourceKey, SourceMap, SourcePath, StableId, Value, Version,
    };
    use rulery_vocabulary::{
        EnumVariant, FieldDeclaration, FieldPresence, OperationalTerm, ResolvedRoot, ResolvedType,
        ResolvedVocabulary, TypeDeclaration,
    };

    use crate::{
        CompiledAction, CompiledActionParameter, CompiledDecision, CompiledEffect, CompiledPackage,
        CompiledPackageDraft, DecisionSemantics, Expr, ExprOperand, Operator, PackageBuildError,
        PackageIntegritySet, Predicate, ReservedOperand,
    };

    use super::*;

    #[allow(clippy::too_many_lines)]
    #[test]
    fn compiled_rule_semantics_round_trip_through_v1_wire() {
        let source_map = source_map();
        let span = source_map.span(SourceKey::new(1), 1, 7).expect("span");
        let package_id = PackageId::new("pkg.main").expect("package");
        let rule_id = RuleId::new("rule.review").expect("rule");
        let effect: CompiledEffect = serde_json::from_value(serde_json::json!({
            "outcome": {
                "kind": "request_information",
                "required_facts": ["account.contact.email"],
                "reasons": [{
                    "code": "missing-contact",
                    "message": "A contact email is required.",
                    "detail": "Account review cannot continue without it."
                }],
                "actions": [{
                    "action": "action.request-contact",
                    "arguments": {"channel": {"kind": "text", "value": "email"}}
                }]
            }
        }))
        .expect("effect");
        let default: CompiledEffect = serde_json::from_value(serde_json::json!({
            "outcome": {
                "kind": "deny",
                "reasons": [{
                    "code": "no-matching-rule",
                    "message": "No rule approved this account.",
                    "detail": null
                }],
                "actions": []
            }
        }))
        .expect("default");
        let semantics: DecisionSemantics = serde_json::from_value(serde_json::json!({
            "missing_facts": {"kind": "request_information"},
            "invalid_facts": {"kind": "preserve_invalid"},
            "precedence": {"kind": "priority_first"},
            "timezone": "America/New_York",
            "expiry": "inclusive"
        }))
        .expect("semantics");
        let rule = crate::CompiledRule::new(
            rule_id.clone(),
            QualifiedRuleId::new(package_id.clone(), rule_id.clone()),
            Some("Review contact details".to_owned()),
            250,
            Expr::Constant { value: true, span },
            effect.clone(),
            Some("Contact review is mandatory.".to_owned()),
            span,
            9,
        );
        let decision = CompiledDecision::new(
            DecisionId::new("decision.review").expect("decision"),
            semantics.clone(),
            default.clone(),
            vec![rule],
            span,
        )
        .expect("decision");
        let compiled = CompiledPackage::new(
            CompiledPackageDraft {
                package_id,
                package_version: Version::new("1.2.3").expect("version"),
                language_version: LanguageVersion::V1,
                compiler_identity: "rulery.compiler".to_owned(),
                decisions: vec![decision],
                actions: Vec::new(),
                integrity: PackageIntegritySet::new(crate::CompilationInput::new(
                    digest(1),
                    digest(2),
                    None,
                )),
            },
            source_map.clone(),
            vocabulary(),
        )
        .expect("package");

        let envelope = CompiledPackageEnvelope::V1(compiled.payload().clone());
        let encoded = serde_json::to_value(&envelope).expect("encode");
        let round_trip: CompiledPackageEnvelope =
            serde_json::from_value(encoded.clone()).expect("decode");
        assert_eq!(round_trip, envelope);
        assert_eq!(
            round_trip.payload().package_hash(),
            compiled.payload().package_hash()
        );

        let mut unknown_effect_field = encoded;
        let wire_effect = &mut unknown_effect_field["payload"]["decisions"]["decision.review"]["rules"]
            ["rule.review"]["effect"];
        wire_effect["unexpected"] = serde_json::Value::Bool(true);
        assert!(serde_json::from_value::<CompiledPackageEnvelope>(unknown_effect_field).is_err());

        let decision = round_trip
            .payload()
            .decisions()
            .get(&DecisionId::new("decision.review").expect("decision"))
            .expect("decision");
        let rule = decision.rules().get(&rule_id).expect("rule");
        assert_eq!(rule.priority(), 250);
        assert_eq!(rule.specificity(), 9);
        assert_eq!(rule.span(), span);
        assert_eq!(rule.title(), Some("Review contact details"));
        assert_eq!(rule.rationale(), Some("Contact review is mandatory."));
        assert_eq!(rule.effect(), &effect);
        assert_eq!(rule.outcome(), effect.outcome());
        assert_eq!(decision.default(), &default);
        assert_eq!(decision.semantics(), &semantics);
    }

    #[allow(clippy::too_many_lines)]
    #[test]
    fn compiled_package_wrapper_rejects_invalid_payloads() {
        let source_map = source_map();
        let span1 = source_map.span(SourceKey::new(1), 0, 4).expect("span1");
        let span2 = source_map.span(SourceKey::new(1), 4, 8).expect("span2");

        let all_expr = Expr::All {
            expressions: vec![Expr::Constant {
                value: true,
                span: span1,
            }],
            span: span1,
        };
        let any_expr = Expr::Any {
            expressions: vec![Expr::Predicate(Predicate {
                operator: Operator::Exists,
                left: ExprOperand::Fact(FactPath::from_str("account.id").expect("fact path")),
                right: None,
                span: span1,
            })],
            span: span1,
        };
        let not_expr = Expr::Not {
            expression: Box::new(Expr::Predicate(Predicate {
                operator: Operator::IsFalse,
                left: ExprOperand::Fact(
                    FactPath::from_str("account.is-active").expect("fact path"),
                ),
                right: None,
                span: span2,
            })),
            span: span2,
        };
        let predicate_expr = Expr::Predicate(Predicate {
            operator: Operator::Equals,
            left: ExprOperand::Fact(FactPath::from_str("account.tier").expect("fact path")),
            right: Some(ExprOperand::Literal(Value::Text("gold".to_owned()))),
            span: span1,
        });
        let reserved_expr = Expr::Predicate(Predicate {
            operator: Operator::Before,
            left: ExprOperand::Fact(FactPath::from_str("account.expires-at").expect("fact path")),
            right: Some(ExprOperand::Reserved(ReservedOperand::Now)),
            span: span2,
        });

        let package_id = PackageId::new("pkg.main").expect("package");
        let decision_id = DecisionId::new("decision.authz").expect("decision");
        let package_version = Version::new("1.2.3").expect("version");
        let rules = vec![
            compiled_rule(
                RuleId::new("rule.alpha").expect("rule alpha"),
                QualifiedRuleId::new(
                    package_id.clone(),
                    RuleId::new("rule.alpha").expect("rule alpha"),
                ),
                all_expr,
                span1,
                7,
            ),
            compiled_rule(
                RuleId::new("rule.beta").expect("rule beta"),
                QualifiedRuleId::new(
                    package_id.clone(),
                    RuleId::new("rule.beta").expect("rule beta"),
                ),
                any_expr,
                span1,
                5,
            ),
            compiled_rule(
                RuleId::new("rule.gamma").expect("rule gamma"),
                QualifiedRuleId::new(
                    package_id.clone(),
                    RuleId::new("rule.gamma").expect("rule gamma"),
                ),
                not_expr,
                span2,
                4,
            ),
            compiled_rule(
                RuleId::new("rule.delta").expect("rule delta"),
                QualifiedRuleId::new(
                    package_id.clone(),
                    RuleId::new("rule.delta").expect("rule delta"),
                ),
                predicate_expr,
                span1,
                8,
            ),
            compiled_rule(
                RuleId::new("rule.epsilon").expect("rule epsilon"),
                QualifiedRuleId::new(
                    package_id.clone(),
                    RuleId::new("rule.epsilon").expect("rule epsilon"),
                ),
                reserved_expr,
                span2,
                6,
            ),
        ];

        let decision =
            CompiledDecision::new(decision_id.clone(), semantics(), effect(), rules, span1)
                .expect("decision");
        let duplicate_rule = compiled_rule(
            RuleId::new("rule.alpha").expect("rule alpha"),
            QualifiedRuleId::new(
                package_id.clone(),
                RuleId::new("rule.alpha").expect("rule alpha"),
            ),
            Expr::Constant {
                value: false,
                span: span1,
            },
            span1,
            1,
        );
        let duplicate_rule_result = CompiledDecision::new(
            decision_id.clone(),
            semantics(),
            effect(),
            vec![duplicate_rule.clone(), duplicate_rule],
            span1,
        );
        assert!(matches!(
            duplicate_rule_result,
            Err(PackageBuildError::DuplicateRule { .. })
        ));

        let action = CompiledAction::new(
            ActionId::new("action.notify").expect("action"),
            BTreeMap::from([
                (
                    StableId::new("channel").expect("channel"),
                    CompiledActionParameter::new(true),
                ),
                (
                    StableId::new("template").expect("template"),
                    CompiledActionParameter::new(false),
                ),
            ]),
        );

        let duplicate_action_result = CompiledPackage::new(
            CompiledPackageDraft {
                package_id: package_id.clone(),
                package_version: package_version.clone(),
                language_version: LanguageVersion::V1,
                compiler_identity: "rulery.compiler".to_owned(),
                decisions: vec![decision.clone()],
                actions: vec![action.clone(), action.clone()],
                integrity: PackageIntegritySet::new(crate::CompilationInput::new(
                    digest(1),
                    digest(2),
                    Some(digest(3)),
                )),
            },
            source_map.clone(),
            vocabulary(),
        );
        assert!(matches!(
            duplicate_action_result,
            Err(PackageBuildError::DuplicateAction { .. })
        ));

        let duplicate_decision_result = CompiledPackage::new(
            CompiledPackageDraft {
                package_id: package_id.clone(),
                package_version: package_version.clone(),
                language_version: LanguageVersion::V1,
                compiler_identity: "rulery.compiler".to_owned(),
                decisions: vec![decision.clone(), decision.clone()],
                actions: vec![action.clone()],
                integrity: PackageIntegritySet::new(crate::CompilationInput::new(
                    digest(1),
                    digest(2),
                    Some(digest(3)),
                )),
            },
            source_map.clone(),
            vocabulary(),
        );
        assert!(matches!(
            duplicate_decision_result,
            Err(PackageBuildError::DuplicateDecision { .. })
        ));

        let compiled = CompiledPackage::new(
            CompiledPackageDraft {
                package_id: package_id.clone(),
                package_version: package_version.clone(),
                language_version: LanguageVersion::V1,
                compiler_identity: "rulery.compiler".to_owned(),
                decisions: vec![decision.clone()],
                actions: vec![action],
                integrity: PackageIntegritySet::new(crate::CompilationInput::new(
                    digest(10),
                    digest(20),
                    Some(digest(30)),
                )),
            },
            source_map.clone(),
            vocabulary(),
        )
        .expect("compiled package");

        assert_eq!(compiled.payload().package_id(), &package_id);
        assert_eq!(compiled.payload().package_version(), &package_version);
        assert_eq!(compiled.payload().language_version(), LanguageVersion::V1);
        assert_eq!(compiled.payload().compiler_identity(), "rulery.compiler");
        assert_eq!(compiled.payload().decisions().len(), 1);
        assert_eq!(compiled.payload().actions().len(), 1);
        assert_eq!(compiled.source_map(), &source_map);
        assert!(!compiled.vocabulary().roots.is_empty());
        assert_eq!(
            compiled
                .payload()
                .decisions()
                .get(&decision_id)
                .expect("decision")
                .rules()
                .len(),
            5
        );
        let retained_rule = compiled
            .payload()
            .decisions()
            .get(&decision_id)
            .expect("decision")
            .rules()
            .get(&RuleId::new("rule.delta").expect("rule delta"))
            .expect("rule");
        assert_eq!(retained_rule.span(), span1);
        assert_eq!(retained_rule.condition().span(), span1);

        let envelope = CompiledPackageEnvelope::V1(compiled.payload().clone());
        let encoded = serde_json::to_value(&envelope).expect("serialize");
        assert_eq!(
            encoded.get("schema").and_then(serde_json::Value::as_str),
            Some("rulery.compiled-package/v1")
        );
        assert!(
            encoded
                .get("payload")
                .and_then(|payload| payload.get("integrity"))
                .is_some()
        );
        assert!(
            encoded
                .get("payload")
                .and_then(|payload| payload.get("package_hash"))
                .is_some()
        );

        let unknown_field = r#"
        {
          "schema": "rulery.compiled-package/v1",
          "payload": {
            "package_id": "pkg.main",
            "package_version": "1.2.3",
            "language_version": 1,
            "compiler_identity": "rulery.compiler",
            "decisions": {},
            "actions": {},
            "integrity": {
              "input": {
                "source_bundle_hash": "blake3:1111111111111111111111111111111111111111111111111111111111111111",
                "vocabulary_hash": "blake3:2222222222222222222222222222222222222222222222222222222222222222",
                "lock_hash": null
              }
            },
            "package_hash": "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "unexpected": true
          }
        }
        "#;
        assert!(serde_json::from_str::<CompiledPackageEnvelope>(unknown_field).is_err());

        let mut tampered = encoded;
        tampered["payload"]["package_hash"] = serde_json::Value::String(
            "blake3:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff".to_owned(),
        );
        let tampered_envelope: CompiledPackageEnvelope =
            serde_json::from_value(tampered).expect("tampered decode");
        let tampered_payload = tampered_envelope.payload().clone();
        let mismatch = CompiledPackage::from_payload(tampered_payload, &source_map, &vocabulary());
        assert!(matches!(
            mismatch,
            Err(PackageBuildError::MismatchedContentHash { .. })
        ));
    }

    fn vocabulary() -> ResolvedVocabulary {
        let root = FactPath::from_str("account").expect("root path");
        let root_type = rulery_contracts::TypeId::new("type.account").expect("type id");
        let status_type = rulery_contracts::TypeId::new("type.status").expect("status type");

        ResolvedVocabulary {
            roots: BTreeMap::from([(
                root.clone(),
                ResolvedRoot {
                    path: root,
                    type_id: root_type.clone(),
                },
            )]),
            types: BTreeMap::from([
                (
                    root_type.clone(),
                    ResolvedType {
                        id: root_type,
                        declaration: TypeDeclaration::Record {
                            id: rulery_contracts::TypeId::new("type.account").expect("record type"),
                            fields: BTreeMap::from([(
                                StableId::new("status").expect("field"),
                                FieldDeclaration {
                                    type_id: status_type.clone(),
                                    presence: FieldPresence::Required,
                                    derived: false,
                                },
                            )]),
                            closed: true,
                        },
                    },
                ),
                (
                    status_type.clone(),
                    ResolvedType {
                        id: status_type,
                        declaration: TypeDeclaration::Enum {
                            id: rulery_contracts::TypeId::new("type.status").expect("enum type"),
                            variants: vec![EnumVariant {
                                symbol: StableId::new("gold").expect("variant"),
                            }],
                        },
                    },
                ),
            ]),
            terms: BTreeMap::from([(
                StableId::new("term.vip").expect("term"),
                OperationalTerm {
                    id: StableId::new("term.vip").expect("term"),
                    applies_to: FactPath::from_str("account.status").expect("fact path"),
                },
            )]),
        }
    }

    fn source_map() -> SourceMap {
        let mut map = SourceMap::new();
        map.insert(
            SourceKey::new(1),
            SourceFile::new(
                SourceId::new("src.main").expect("source id"),
                SourcePath::new("rulery.yaml").expect("source path"),
                Arc::<str>::from("abcdefgh"),
            ),
        )
        .expect("source map insert");
        map
    }

    fn digest(byte: u8) -> ContentHash {
        ContentHash::from_bytes([byte; 32])
    }

    fn compiled_rule(
        id: RuleId,
        qualified_id: QualifiedRuleId,
        condition: Expr,
        span: rulery_contracts::Span,
        specificity: u32,
    ) -> crate::CompiledRule {
        crate::CompiledRule::new(
            id,
            qualified_id,
            Some("Fixture rule".to_owned()),
            0,
            condition,
            effect(),
            Some("Exercises v1 wire encoding.".to_owned()),
            span,
            specificity,
        )
    }

    fn effect() -> CompiledEffect {
        let reasons = Reasons::new(vec![
            Reason::new(
                ReasonCode::new("fixture-effect").expect("reason code"),
                "Fixture effect.",
            )
            .expect("reason"),
        ])
        .expect("reasons");
        CompiledEffect::new(OutcomeTemplate::approve(reasons, Vec::new()))
    }

    fn semantics() -> DecisionSemantics {
        DecisionSemantics::new(
            crate::MissingFactStrategy::PreserveUnknown,
            crate::InvalidFactStrategy::PreserveInvalid,
            crate::DecisionPrecedence::PriorityFirst,
            PolicyTimeZone::new("UTC").expect("timezone"),
            crate::ExpiryPolicy::Inclusive,
        )
        .expect("semantics")
    }
}
