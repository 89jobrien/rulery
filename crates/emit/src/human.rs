//! Stable human-readable artifact rendering.

use rulery_contracts::{
    DecisionId, OutcomeKind, PackageId, PolicyDate, PolicyTimeZone, QualifiedRuleId, UtcInstant,
    Version,
};

/// Complete presentation data for one human decision explanation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HumanExplanation {
    /// Selected outcome kind.
    pub outcome: OutcomeKind,
    /// Rulebook package identity.
    pub package: PackageId,
    /// Rulebook version.
    pub version: Version,
    /// Decision identity.
    pub decision: DecisionId,
    /// Exact UTC evaluation instant.
    pub evaluated_at: UtcInstant,
    /// Policy-local date.
    pub policy_date: PolicyDate,
    /// Policy timezone.
    pub timezone: PolicyTimeZone,
    /// Canonical rendered reason lines.
    pub reasons: Vec<String>,
    /// Determining rules.
    pub determining_rules: Vec<QualifiedRuleId>,
    /// Canonical condition explanation lines.
    pub conditions: Vec<String>,
    /// Required fact summaries.
    pub required_facts: Vec<String>,
    /// Invalid fact summaries.
    pub invalid_facts: Vec<String>,
    /// Superseded rule summaries.
    pub superseded_rules: Vec<String>,
}

/// Stable human renderer.
#[derive(Clone, Copy, Debug, Default)]
pub struct HumanRenderer;

impl HumanRenderer {
    /// Renders a byte-stable decision explanation.
    #[must_use]
    pub fn explain(&self, explanation: &HumanExplanation) -> String {
        let evaluated_at = explanation.evaluated_at.to_rfc3339();
        let mut output = format!(
            "Decision: {}\nRulebook: {}@{}\nDecision ID: {}\nEvaluated at: {}\nPolicy date: {} ({})\n\n",
            outcome_label(explanation.outcome),
            explanation.package,
            explanation.version,
            explanation.decision,
            evaluated_at,
            explanation.policy_date,
            explanation.timezone.as_str(),
        );
        push_section(
            &mut output,
            if explanation.reasons.len() == 1 {
                "Reason"
            } else {
                "Reasons"
            },
            &explanation.reasons,
        );
        let determining = explanation
            .determining_rules
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        push_section(
            &mut output,
            if determining.len() == 1 {
                "Determining rule"
            } else {
                "Determining rules"
            },
            &determining,
        );
        push_section(&mut output, "Conditions", &explanation.conditions);
        push_summary(&mut output, "Required facts", &explanation.required_facts);
        push_summary(&mut output, "Invalid facts", &explanation.invalid_facts);
        push_summary(
            &mut output,
            "Superseded rules",
            &explanation.superseded_rules,
        );
        output.push_str(
            "\nEvidence:\n  canonical package, facts, and trace hashes are present in JSON output\n  timezone database identity is present in JSON output\n",
        );
        output
    }

    /// Renders stable line-oriented diagnostics.
    #[must_use]
    pub fn diagnostics(&self, lines: &[String]) -> String {
        render_lines(lines)
    }

    /// Renders stable line-oriented analysis findings.
    #[must_use]
    pub fn analysis(&self, lines: &[String]) -> String {
        render_lines(lines)
    }

    /// Renders stable line-oriented scenario results.
    #[must_use]
    pub fn scenarios(&self, lines: &[String]) -> String {
        render_lines(lines)
    }
}

fn render_lines(lines: &[String]) -> String {
    let mut rendered = lines.join("\n");
    if !rendered.is_empty() {
        rendered.push('\n');
    }
    rendered
}

const fn outcome_label(outcome: OutcomeKind) -> &'static str {
    match outcome {
        OutcomeKind::Approve => "APPROVE",
        OutcomeKind::Deny => "DENY",
        OutcomeKind::Escalate => "ESCALATE",
        OutcomeKind::RequestInformation => "REQUEST INFORMATION",
    }
}

fn push_section(output: &mut String, label: &str, lines: &[String]) {
    output.push_str(label);
    output.push_str(":\n");
    for line in lines {
        output.push_str("  ");
        output.push_str(line);
        output.push('\n');
    }
    output.push('\n');
}

fn push_summary(output: &mut String, label: &str, lines: &[String]) {
    output.push_str(label);
    if lines.is_empty() {
        output.push_str(": none\n");
    } else {
        output.push_str(":\n");
        for line in lines {
            output.push_str("  ");
            output.push_str(line);
            output.push('\n');
        }
    }
}

#[cfg(test)]
mod tests {
    use rulery_contracts::{PackageId, QualifiedRuleId, RuleId};

    use super::*;

    #[test]
    fn human_explain_matches_canonical_tool_library_text() {
        let explanation = HumanExplanation {
            outcome: OutcomeKind::Deny,
            package: PackageId::new("community-tool-library").expect("package"),
            version: Version::new("0.1.0").expect("version"),
            decision: DecisionId::new("checkout").expect("decision"),
            evaluated_at: UtcInstant::new(1_789_574_400_000_000_000).expect("instant"),
            policy_date: PolicyDate::parse("2026-09-16").expect("date"),
            timezone: PolicyTimeZone::new("America/New_York").expect("timezone"),
            reasons: vec!["[expired-training] Power-tool training has expired.".to_owned()],
            determining_rules: vec![QualifiedRuleId::new(
                PackageId::new("community-tool-library").expect("package"),
                RuleId::new("deny-expired-training").expect("rule"),
            )],
            conditions: vec![
                "true  tool.category equal power-tool".to_owned(),
                "true  member.training.valid-until is expired".to_owned(),
                "true  inclusive expiry: 2026-09-15 is earlier than 2026-09-16".to_owned(),
            ],
            required_facts: Vec::new(),
            invalid_facts: Vec::new(),
            superseded_rules: Vec::new(),
        };
        let expected = "Decision: DENY\nRulebook: community-tool-library@0.1.0\nDecision ID: checkout\nEvaluated at: 2026-09-16T16:00:00.000000000Z\nPolicy date: 2026-09-16 (America/New_York)\n\nReason:\n  [expired-training] Power-tool training has expired.\n\nDetermining rule:\n  community-tool-library::deny-expired-training\n\nConditions:\n  true  tool.category equal power-tool\n  true  member.training.valid-until is expired\n  true  inclusive expiry: 2026-09-15 is earlier than 2026-09-16\n\nRequired facts: none\nInvalid facts: none\nSuperseded rules: none\n\nEvidence:\n  canonical package, facts, and trace hashes are present in JSON output\n  timezone database identity is present in JSON output\n";
        assert_eq!(HumanRenderer.explain(&explanation), expected);

        let markdown = crate::MarkdownRenderer.render_truths(&[
            ("missing", rulery_engine::Truth::Unknown),
            ("malformed", rulery_engine::Truth::Invalid),
        ]);
        assert!(markdown.contains("Unknown"));
        assert!(markdown.contains("Invalid"));
        assert!(!markdown.contains("failed"));
    }
}
