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
        ActionId, ContentHash, DecisionId, FactPath, LanguageVersion, PackageId, QualifiedRuleId,
        RuleId, SourceFile, SourceId, SourceKey, SourceMap, SourcePath, StableId, Value, Version,
    };
    use rulery_vocabulary::{
        EnumVariant, FieldDeclaration, FieldPresence, OperationalTerm, ResolvedRoot, ResolvedType,
        ResolvedVocabulary, TypeDeclaration,
    };

    use crate::{
        CompiledAction, CompiledActionParameter, CompiledDecision, CompiledPackage,
        CompiledPackageDraft, Expr, ExprOperand, Operator, PackageBuildError, PackageIntegritySet,
        Predicate, ReservedOperand,
    };

    use super::*;

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
            crate::CompiledRule::new(
                RuleId::new("rule.alpha").expect("rule alpha"),
                QualifiedRuleId::new(
                    package_id.clone(),
                    RuleId::new("rule.alpha").expect("rule alpha"),
                ),
                all_expr,
                span1,
                7,
            ),
            crate::CompiledRule::new(
                RuleId::new("rule.beta").expect("rule beta"),
                QualifiedRuleId::new(
                    package_id.clone(),
                    RuleId::new("rule.beta").expect("rule beta"),
                ),
                any_expr,
                span1,
                5,
            ),
            crate::CompiledRule::new(
                RuleId::new("rule.gamma").expect("rule gamma"),
                QualifiedRuleId::new(
                    package_id.clone(),
                    RuleId::new("rule.gamma").expect("rule gamma"),
                ),
                not_expr,
                span2,
                4,
            ),
            crate::CompiledRule::new(
                RuleId::new("rule.delta").expect("rule delta"),
                QualifiedRuleId::new(
                    package_id.clone(),
                    RuleId::new("rule.delta").expect("rule delta"),
                ),
                predicate_expr,
                span1,
                8,
            ),
            crate::CompiledRule::new(
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

        let decision = CompiledDecision::new(decision_id.clone(), rules).expect("decision");
        let duplicate_rule = crate::CompiledRule::new(
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
            vec![duplicate_rule.clone(), duplicate_rule],
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
}
