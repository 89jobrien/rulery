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

    /// The canonical tool-library explanation, which the specification names as a required
    /// renderer snapshot.
    fn canonical_explanation() -> HumanExplanation {
        HumanExplanation {
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
                "true  member.training.valid-until is before today (2026-09-16)".to_owned(),
                "true  tool.category equals power-tool".to_owned(),
            ],
            required_facts: Vec::new(),
            invalid_facts: Vec::new(),
            superseded_rules: Vec::new(),
        }
    }

    #[test]
    fn human_explain_matches_canonical_tool_library_text() {
        insta::assert_snapshot!(HumanRenderer.explain(&canonical_explanation()));
    }

    /// The explanation is a wire artifact, so it must end in exactly one newline and never
    /// normalize away a section that a reader depends on.
    #[test]
    fn human_explain_has_one_trailing_newline_and_every_section() {
        let rendered = HumanRenderer.explain(&canonical_explanation());
        assert!(
            rendered.ends_with("timezone database identity is present in JSON output\n"),
            "the explanation must close with its evidence section and a single newline: {rendered:?}"
        );
        assert!(
            !rendered.ends_with("\n\n"),
            "the explanation must not gain a trailing blank line"
        );
        for section in [
            "Decision: ",
            "Rulebook: ",
            "Decision ID: ",
            "Evaluated at: ",
            "Policy date: ",
            "Reason:",
            "Determining rule:",
            "Conditions:",
            "Required facts:",
            "Invalid facts:",
            "Superseded rules:",
            "Evidence:",
        ] {
            assert!(
                rendered.contains(section),
                "the explanation is missing its `{section}` section"
            );
        }
    }

    #[test]
    fn markdown_truth_table_names_every_value_it_is_given() {
        let markdown = crate::MarkdownRenderer.render_truths(&[
            ("missing", rulery_engine::Truth::Unknown),
            ("malformed", rulery_engine::Truth::Invalid),
        ]);
        insta::assert_snapshot!(markdown);
        assert!(!markdown.contains("failed"), "{markdown}");
    }
}
