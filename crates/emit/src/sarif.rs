//! Minimal deterministic SARIF 2.1.0 rendering.

/// SARIF source location.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SarifLocation {
    /// Artifact path.
    pub path: String,
    /// One-based line.
    pub line: u32,
    /// One-based column.
    pub column: u32,
}

/// Diagnostic projected to SARIF.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SarifDiagnostic {
    /// Stable rule code.
    pub code: String,
    /// Message text.
    pub message: String,
    /// Source location.
    pub location: SarifLocation,
}

/// SARIF renderer.
#[derive(Clone, Copy, Debug, Default)]
pub struct SarifRenderer;

impl SarifRenderer {
    /// Renders exactly one SARIF 2.1.0 log.
    #[must_use]
    pub fn render(&self, diagnostics: &[SarifDiagnostic]) -> serde_json::Value {
        let mut diagnostics = diagnostics.to_vec();
        diagnostics.sort_by(|left, right| {
            left.code
                .cmp(&right.code)
                .then_with(|| left.location.path.cmp(&right.location.path))
                .then_with(|| left.location.line.cmp(&right.location.line))
                .then_with(|| left.location.column.cmp(&right.location.column))
        });
        let results = diagnostics
            .into_iter()
            .map(|diagnostic| {
                serde_json::json!({
                    "ruleId": diagnostic.code,
                    "message": { "text": diagnostic.message },
                    "locations": [{
                        "physicalLocation": {
                            "artifactLocation": { "uri": diagnostic.location.path },
                            "region": {
                                "startLine": diagnostic.location.line,
                                "startColumn": diagnostic.location.column
                            }
                        }
                    }]
                })
            })
            .collect::<Vec<_>>();
        serde_json::json!({
            "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
            "version": "2.1.0",
            "runs": [{
                "tool": { "driver": { "name": "rulery" } },
                "results": results
            }]
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rulery_contracts::{
        ContentHash, PackageId, SourceFile, SourceId, SourceKey, SourceMap, SourcePath,
    };

    use crate::DecisionTableEnvelope;

    use super::*;
    use crate::{
        ColumnRole, DecisionCell, DecisionColumn, DecisionProjection, ProjectionCapabilities,
        ProjectionFeature, ProjectionRule, build_decision_table, validate_projection,
    };

    #[test]
    fn projections_reject_semantic_loss() {
        let span = span();
        let rule = ProjectionRule {
            rule: rulery_contracts::QualifiedRuleId::new(
                PackageId::new("pkg.main").expect("package"),
                rulery_contracts::RuleId::new("rule.main").expect("rule"),
            ),
            cells: vec![DecisionCell::Present],
            outcome: rulery_contracts::OutcomeKind::Escalate,
            span,
            truth_states: vec![rulery_engine::Truth::Unknown, rulery_engine::Truth::Invalid],
            has_actions: true,
            has_reasons: true,
        };
        let projection = DecisionProjection {
            decision: rulery_contracts::DecisionId::new("decision.main").expect("decision"),
            columns: vec![DecisionColumn {
                id: rulery_contracts::StableId::new("condition").expect("column"),
                label: "Condition".to_owned(),
                role: ColumnRole::Condition,
            }],
            rules: vec![rule.clone()],
        };
        let table = build_decision_table(ContentHash::from_bytes([1; 32]), vec![projection])
            .expect("table");
        assert_eq!(table.payload().columns.len(), 1);
        assert_eq!(table.payload().rows.len(), 1);
        assert_eq!(table.payload().source_references.len(), 1);
        let envelope = DecisionTableEnvelope::V1(table.payload().clone());
        let value = serde_json::to_value(envelope).expect("envelope");
        assert_eq!(value["schema"], "rulery.decision-table/v1");
        assert!(matches!(
            build_decision_table(ContentHash::from_bytes([1; 32]), Vec::new()),
            Err(crate::DecisionTableError::DecisionCount)
        ));

        let losses = validate_projection(
            &[rule],
            ProjectionCapabilities {
                escalation: false,
                information_request: false,
                uncertainty: false,
                actions: false,
                reasons: false,
            },
        );
        assert_eq!(
            losses.iter().map(|loss| loss.feature).collect::<Vec<_>>(),
            vec![
                ProjectionFeature::Escalation,
                ProjectionFeature::Unknown,
                ProjectionFeature::Invalid,
                ProjectionFeature::Action,
                ProjectionFeature::Reason,
            ]
        );
        assert_eq!(
            losses.iter().map(|loss| loss.code).collect::<Vec<_>>(),
            vec!["RUL401", "RUL400", "RUL400", "RUL400", "RUL400"]
        );

        let sarif = SarifRenderer.render(&[SarifDiagnostic {
            code: "RUL400".to_owned(),
            message: "lossy export".to_owned(),
            location: SarifLocation {
                path: "rules/main.yaml".to_owned(),
                line: 3,
                column: 5,
            },
        }]);
        assert_eq!(sarif["version"], "2.1.0");
        assert_eq!(sarif["runs"][0]["results"][0]["ruleId"], "RUL400");
        assert_eq!(
            sarif["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["region"]["startLine"],
            3
        );
        assert_ne!(
            table.payload().rows[0].outcome,
            rulery_contracts::OutcomeKind::Deny
        );
    }

    fn span() -> rulery_contracts::Span {
        let mut map = SourceMap::new();
        map.insert(
            SourceKey::new(1),
            SourceFile::new(
                SourceId::new("source.main").expect("source"),
                SourcePath::new("rules/main.yaml").expect("path"),
                Arc::<str>::from("abc"),
            ),
        )
        .expect("insert");
        map.span(SourceKey::new(1), 0, 1).expect("span")
    }
}
