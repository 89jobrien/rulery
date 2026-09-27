//! Strict versioned analysis report contracts.

use rulery_contracts::{ContentHash, DecisionId};
use rulery_diagnostics::DiagnosticReportV1;
use rulery_ir::CompiledPackage;
use serde::{Deserialize, Serialize};

use crate::{
    AnalysisCompleteness, AnalysisOptions, CoverageReport, PolicyDiff, RuleOverlap,
    RuleReachability, WitnessCase,
};

/// Coverage report associated with a decision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionCoverage {
    /// Decision identity.
    pub decision: DecisionId,
    /// Exact finite coverage.
    pub report: CoverageReport,
}

/// Complete analysis report payload version 1.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysisReportV1 {
    /// Analyzed package hash.
    pub package_hash: ContentHash,
    /// Effective analysis options.
    pub options: AnalysisOptions,
    /// Aggregate completeness.
    pub completeness: AnalysisCompleteness,
    /// Structured diagnostics.
    pub diagnostics: DiagnosticReportV1,
    /// Rule reachability in rule order.
    pub reachability: Vec<RuleReachability>,
    /// Pairwise interactions in pair order.
    pub overlaps: Vec<RuleOverlap>,
    /// Coverage in decision order.
    pub coverage: Vec<DecisionCoverage>,
    /// Witnesses in canonical hash order.
    pub witnesses: Vec<WitnessCase>,
    /// Semantic diffs in decision order.
    pub semantic_diffs: Vec<PolicyDiff>,
}

/// Validated deterministic analysis report.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnalysisReport {
    payload: AnalysisReportV1,
}

impl AnalysisReport {
    /// Canonicalizes every report section.
    #[must_use]
    pub fn new(mut payload: AnalysisReportV1) -> Self {
        payload
            .reachability
            .sort_by(|left, right| left.rule.cmp(&right.rule));
        payload.overlaps.sort_by(|left, right| {
            left.left
                .cmp(&right.left)
                .then_with(|| left.right.cmp(&right.right))
        });
        payload
            .coverage
            .sort_by(|left, right| left.decision.cmp(&right.decision));
        payload.witnesses.sort_by_key(|witness| witness.hash);
        payload
            .semantic_diffs
            .sort_by(|left, right| left.decision.cmp(&right.decision));
        Self { payload }
    }

    /// Returns the canonical v1 payload.
    #[must_use]
    pub const fn payload(&self) -> &AnalysisReportV1 {
        &self.payload
    }
}

/// Static analysis application port.
pub trait PolicyAnalyzer: Send + Sync {
    /// Analyzes one compiled package.
    fn analyze(&self, package: &CompiledPackage, options: &AnalysisOptions) -> AnalysisReport;
}

/// Strict versioned analysis report envelope.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "schema", content = "payload", deny_unknown_fields)]
pub enum AnalysisReportEnvelope {
    /// Analysis report schema version 1.
    #[serde(rename = "rulery.analysis-report/v1")]
    V1(AnalysisReportV1),
}

#[cfg(test)]
mod tests {
    use rulery_contracts::{LanguageVersion, PackageId, QualifiedRuleId, RuleId};
    use rulery_diagnostics::DiagnosticReport;

    use crate::{ProofCertificate, ReachabilityStatus};

    use super::*;

    #[test]
    fn analysis_report_envelope_contains_all_sections() {
        let diagnostics = DiagnosticReport::new(Vec::new(), "rulery.analysis", LanguageVersion::V1)
            .expect("diagnostics");
        let rule_a = qualified("rule.a");
        let rule_b = qualified("rule.b");
        let report = AnalysisReport::new(AnalysisReportV1 {
            package_hash: ContentHash::from_bytes([1; 32]),
            options: AnalysisOptions::default(),
            completeness: AnalysisCompleteness::Complete,
            diagnostics: diagnostics.payload().clone(),
            reachability: vec![
                RuleReachability {
                    rule: rule_b,
                    status: ReachabilityStatus::Unreachable,
                    witness: None,
                    proof: Some(ProofCertificate {
                        method: "proof-b".to_owned(),
                        constraints_hash: ContentHash::from_bytes([2; 32]),
                    }),
                },
                RuleReachability {
                    rule: rule_a,
                    status: ReachabilityStatus::Unreachable,
                    witness: None,
                    proof: Some(ProofCertificate {
                        method: "proof-a".to_owned(),
                        constraints_hash: ContentHash::from_bytes([3; 32]),
                    }),
                },
            ],
            overlaps: Vec::new(),
            coverage: Vec::new(),
            witnesses: Vec::new(),
            semantic_diffs: Vec::new(),
        });
        assert_eq!(
            report.payload().reachability[0].rule.rule().as_str(),
            "rule.a"
        );
        let envelope = AnalysisReportEnvelope::V1(report.payload().clone());
        let value = serde_json::to_value(&envelope).expect("serialize");
        assert_eq!(value["schema"], "rulery.analysis-report/v1");
        for field in [
            "package_hash",
            "options",
            "completeness",
            "diagnostics",
            "reachability",
            "overlaps",
            "coverage",
            "witnesses",
            "semantic_diffs",
        ] {
            assert!(value["payload"].get(field).is_some(), "missing {field}");
        }

        let mut missing = value.clone();
        missing["payload"]
            .as_object_mut()
            .expect("payload")
            .remove("witnesses");
        assert!(serde_json::from_value::<AnalysisReportEnvelope>(missing).is_err());
        let mut unknown = value;
        unknown["payload"]["unexpected"] = serde_json::Value::Bool(true);
        assert!(serde_json::from_value::<AnalysisReportEnvelope>(unknown).is_err());
    }

    fn qualified(rule: &str) -> QualifiedRuleId {
        QualifiedRuleId::new(
            PackageId::new("pkg.main").expect("package"),
            RuleId::new(rule).expect("rule"),
        )
    }
}
