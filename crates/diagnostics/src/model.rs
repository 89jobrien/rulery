//! Diagnostic model, evidence, and validated fixes.

use std::collections::BTreeMap;

use rulery_contracts::{
    ContentHash, DecisionId, EvaluationId, FactPath, LanguageVersion, OutcomeKind, PackageId,
    QualifiedRuleId, ScenarioId, SourceKey, Span, StableId, Version,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    DiagnosticCode, DiagnosticDefinition, DiagnosticProducer, EvidenceRequirement, Severity,
    Suppressibility, diagnostic_definition,
};

/// One span annotation in a diagnostic message.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticLabel {
    span: Span,
    style: LabelStyle,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

impl DiagnosticLabel {
    /// Creates a label attached to a source span.
    ///
    /// # Errors
    ///
    /// Returns [`DiagnosticBuildError`] when the optional message is empty.
    pub fn new(
        span: Span,
        style: LabelStyle,
        message: Option<String>,
    ) -> Result<Self, DiagnosticBuildError> {
        if message
            .as_ref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(DiagnosticBuildError::new(
                "label.message",
                "label message must not be empty",
            ));
        }
        Ok(Self {
            span,
            style,
            message,
        })
    }

    /// Returns the labeled source span.
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }
}

/// Label importance and rendering intent.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum LabelStyle {
    /// Primary error location.
    Primary,
    /// Secondary supporting location.
    Secondary,
    /// Context-only location.
    Context,
}

/// One explanatory note for a diagnostic.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticNote {
    kind: NoteKind,
    message: String,
}

impl DiagnosticNote {
    /// Creates a note with non-empty message text.
    ///
    /// # Errors
    ///
    /// Returns [`DiagnosticBuildError`] when the message is empty.
    pub fn new(kind: NoteKind, message: impl Into<String>) -> Result<Self, DiagnosticBuildError> {
        let message = message.into();
        if message.trim().is_empty() {
            return Err(DiagnosticBuildError::new(
                "note.message",
                "note message must not be empty",
            ));
        }
        Ok(Self { kind, message })
    }
}

/// Note category.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum NoteKind {
    /// Why this diagnostic exists.
    Rationale,
    /// Semantic interpretation details.
    Semantics,
    /// Known limitation.
    Limitation,
    /// Compatibility concern.
    Compatibility,
    /// Security concern.
    Security,
    /// Privacy concern.
    Privacy,
    /// Fairness concern.
    Fairness,
    /// Performance concern.
    Performance,
}

/// Suggested remediation text.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HelpItem {
    priority: HelpPriority,
    message: String,
}

impl HelpItem {
    /// Creates a help item with non-empty message text.
    ///
    /// # Errors
    ///
    /// Returns [`DiagnosticBuildError`] when the message is empty.
    pub fn new(
        priority: HelpPriority,
        message: impl Into<String>,
    ) -> Result<Self, DiagnosticBuildError> {
        let message = message.into();
        if message.trim().is_empty() {
            return Err(DiagnosticBuildError::new(
                "help.message",
                "help message must not be empty",
            ));
        }
        Ok(Self { priority, message })
    }
}

/// Ranking for remediation guidance.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum HelpPriority {
    /// Best next step.
    Primary,
    /// Alternative approach.
    Alternative,
    /// Background learning guidance.
    Educational,
}

/// Evidence confidence used by analysis diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum FindingConfidence {
    /// Sound proof-backed finding.
    Proven,
    /// Replayable witness-backed finding.
    Witnessed,
    /// Heuristic finding.
    Heuristic,
    /// Inconclusive due to limits.
    Inconclusive,
}

/// Subject identifiers referenced by a diagnostic.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticSubjects {
    #[serde(skip_serializing_if = "Option::is_none")]
    package_id: Option<PackageId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    decision_id: Option<DecisionId>,
    rule_ids: Vec<QualifiedRuleId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scenario_id: Option<ScenarioId>,
    fact_paths: Vec<FactPath>,
}

/// Registry-constrained immutable properties.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticProperties {
    producer: DiagnosticProducer,
    suppressibility: Suppressibility,
    confidence: FindingConfidence,
    subjects: DiagnosticSubjects,
    tags: Vec<String>,
}

impl DiagnosticProperties {
    /// Returns the producing subsystem.
    #[must_use]
    pub const fn producer(&self) -> DiagnosticProducer {
        self.producer
    }

    /// Returns suppressibility policy.
    #[must_use]
    pub const fn suppressibility(&self) -> Suppressibility {
        self.suppressibility
    }
}

/// Structured supporting evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DiagnosticEvidence {
    /// Replayable witness evidence.
    Witness(WitnessEvidence),
    /// Trace-backed evidence.
    Trace(TraceEvidence),
    /// Before/after behavior evidence.
    BehaviorChange(BehaviorChangeEvidence),
    /// Build provenance evidence.
    Provenance(ProvenanceEvidence),
    /// Analysis budget evidence.
    AnalysisLimit(AnalysisLimitEvidence),
    /// Static proof evidence.
    Proof(ProofEvidence),
    /// Runtime conflict evidence.
    Conflict(ConflictEvidence),
    /// Generic properties evidence.
    Properties(BTreeMap<String, String>),
}

/// Replayable witness evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WitnessEvidence {
    witness_id: StableId,
    summary: String,
    facts: String,
    facts_hash: ContentHash,
    decision: DecisionId,
    matched_rules: Vec<QualifiedRuleId>,
    outcome_kinds: Vec<OutcomeKind>,
}

/// Evaluation trace evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceEvidence {
    evaluation_id: EvaluationId,
    summary: String,
    trace_hash: ContentHash,
}

/// Before/after behavior evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BehaviorChangeEvidence {
    before_outcome: OutcomeKind,
    after_outcome: OutcomeKind,
    before_rules: Vec<QualifiedRuleId>,
    after_rules: Vec<QualifiedRuleId>,
    witness_facts_hash: ContentHash,
}

/// Provenance evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceEvidence {
    package_hash: ContentHash,
    compiler_version: Version,
    language_version: LanguageVersion,
}

/// Proof evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProofEvidence {
    method: String,
    constraints_hash: ContentHash,
    subjects: DiagnosticSubjects,
}

/// Conflict evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConflictEvidence {
    evaluation_id: EvaluationId,
    trace_hash: ContentHash,
    semantic_key: Vec<String>,
    rules: Vec<QualifiedRuleId>,
    outcomes: Vec<OutcomeKind>,
}

/// Static analysis execution phase.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AnalysisPhase {
    /// Finite domain construction.
    DomainConstruction,
    /// Constraint solving.
    ConstraintSolving,
    /// Witness search.
    WitnessSearch,
    /// Coverage enumeration.
    CoverageEnumeration,
    /// Diff enumeration.
    DiffEnumeration,
}

/// Analysis budget-limit evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysisLimitEvidence {
    phase: AnalysisPhase,
    limit_name: String,
    configured_limit: u64,
    observed_value: u64,
    unresolved_paths: Vec<FactPath>,
}

/// Whether a fix can be safely applied automatically.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum FixApplicability {
    /// Syntax-safe in-place change.
    MachineApplicable,
    /// Requires human review.
    RequiresReview,
    /// Contains placeholders.
    HasPlaceholders,
}

/// One source edit in a suggested fix.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextEdit {
    span: Span,
    replacement: String,
}

impl TextEdit {
    /// Creates a text edit for a validated source span.
    #[must_use]
    pub fn new(span: Span, replacement: impl Into<String>) -> Self {
        Self {
            span,
            replacement: replacement.into(),
        }
    }

    /// Returns the edit span.
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }
}

/// One suggested fix composed of non-overlapping edits.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuggestedFix {
    id: StableId,
    title: String,
    applicability: FixApplicability,
    edits: Vec<TextEdit>,
}

impl SuggestedFix {
    /// Creates a suggested fix with overlap validation.
    ///
    /// # Errors
    ///
    /// Returns [`DiagnosticBuildError`] when the title is empty, no edits are provided, or edits
    /// overlap.
    pub fn new(
        id: StableId,
        title: impl Into<String>,
        applicability: FixApplicability,
        edits: Vec<TextEdit>,
    ) -> Result<Self, DiagnosticBuildError> {
        let title = title.into();
        if title.trim().is_empty() {
            return Err(DiagnosticBuildError::new(
                "fix.title",
                "fix title must not be empty",
            ));
        }
        if edits.is_empty() {
            return Err(DiagnosticBuildError::new(
                "fix.edits",
                "fix must contain at least one edit",
            ));
        }
        ensure_non_overlapping_edits(&edits)?;
        Ok(Self {
            id,
            title,
            applicability,
            edits,
        })
    }

    /// Returns edits in deterministic application order.
    #[must_use]
    pub fn ordered_edits(&self) -> Vec<TextEdit> {
        let mut edits = self.edits.clone();
        edits.sort_by(|left, right| {
            left.span()
                .source()
                .cmp(&right.span().source())
                .then_with(|| right.span().start().cmp(&left.span().start()))
                .then_with(|| right.span().end().cmp(&left.span().end()))
        });
        edits
    }

    /// Applies this fix to per-source mutable text buffers.
    ///
    /// # Errors
    ///
    /// Returns [`DiagnosticBuildError`] when a source key is missing or a span is out of bounds.
    pub fn apply_to_sources(
        &self,
        sources: &mut BTreeMap<SourceKey, String>,
    ) -> Result<(), DiagnosticBuildError> {
        for edit in self.ordered_edits() {
            let source = edit.span().source();
            let content = sources.get_mut(&source).ok_or_else(|| {
                DiagnosticBuildError::new("fix.edits", "fix edit references unknown source")
            })?;

            let start = usize::try_from(edit.span().start()).map_err(|_| {
                DiagnosticBuildError::new("fix.edits", "fix edit start is out of range")
            })?;
            let end = usize::try_from(edit.span().end()).map_err(|_| {
                DiagnosticBuildError::new("fix.edits", "fix edit end is out of range")
            })?;

            if start > end
                || end > content.len()
                || !content.is_char_boundary(start)
                || !content.is_char_boundary(end)
            {
                return Err(DiagnosticBuildError::new(
                    "fix.edits",
                    "fix edit span is invalid for source content",
                ));
            }

            content.replace_range(start..end, &edit.replacement);
        }
        Ok(())
    }
}

/// Fully validated immutable diagnostic.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Diagnostic {
    code: DiagnosticCode,
    severity: Severity,
    title: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    impact: Option<String>,
    labels: Vec<DiagnosticLabel>,
    notes: Vec<DiagnosticNote>,
    help: Vec<HelpItem>,
    evidence: Vec<DiagnosticEvidence>,
    fixes: Vec<SuggestedFix>,
    properties: DiagnosticProperties,
}

impl Diagnostic {
    /// Starts a validated diagnostic builder.
    ///
    /// # Errors
    ///
    /// Returns [`DiagnosticBuildError`] for unknown codes or empty message text.
    pub fn builder(
        code: DiagnosticCode,
        message: impl Into<String>,
    ) -> Result<DiagnosticBuilder, DiagnosticBuildError> {
        DiagnosticBuilder::new(code, message)
    }

    /// Returns the diagnostic code.
    #[must_use]
    pub fn code(&self) -> &DiagnosticCode {
        &self.code
    }

    /// Returns the registry-derived severity.
    #[must_use]
    pub const fn severity(&self) -> Severity {
        self.severity
    }

    /// Returns the registry-derived title.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns immutable registry-constrained properties.
    #[must_use]
    pub fn properties(&self) -> &DiagnosticProperties {
        &self.properties
    }

    /// Returns the diagnostic message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Stateful builder for [`Diagnostic`].
#[derive(Clone, Debug)]
pub struct DiagnosticBuilder {
    code: DiagnosticCode,
    message: String,
    impact: Option<String>,
    labels: Vec<DiagnosticLabel>,
    notes: Vec<DiagnosticNote>,
    help: Vec<HelpItem>,
    evidence: Vec<DiagnosticEvidence>,
    fixes: Vec<SuggestedFix>,
    confidence: FindingConfidence,
    subjects: DiagnosticSubjects,
    tags: Vec<String>,
}

impl DiagnosticBuilder {
    /// Creates a new builder.
    ///
    /// # Errors
    ///
    /// Returns [`DiagnosticBuildError`] for unknown diagnostic codes or empty message text.
    pub fn new(
        code: DiagnosticCode,
        message: impl Into<String>,
    ) -> Result<Self, DiagnosticBuildError> {
        let message = message.into();
        if message.trim().is_empty() {
            return Err(DiagnosticBuildError::new(
                "message",
                "diagnostic message must not be empty",
            ));
        }
        if diagnostic_definition(&code).is_none() {
            return Err(DiagnosticBuildError::new(
                "code",
                "diagnostic code is not part of the known registry",
            ));
        }
        Ok(Self {
            code,
            message,
            impact: None,
            labels: Vec::new(),
            notes: Vec::new(),
            help: Vec::new(),
            evidence: Vec::new(),
            fixes: Vec::new(),
            confidence: FindingConfidence::Heuristic,
            subjects: DiagnosticSubjects::default(),
            tags: Vec::new(),
        })
    }

    /// Sets impact text.
    #[must_use]
    pub fn impact(mut self, impact: impl Into<String>) -> Self {
        self.impact = Some(impact.into());
        self
    }

    /// Appends one label.
    #[must_use]
    pub fn push_label(mut self, label: DiagnosticLabel) -> Self {
        self.labels.push(label);
        self
    }

    /// Appends one note.
    #[must_use]
    pub fn push_note(mut self, note: DiagnosticNote) -> Self {
        self.notes.push(note);
        self
    }

    /// Appends one help item.
    #[must_use]
    pub fn push_help(mut self, help: HelpItem) -> Self {
        self.help.push(help);
        self
    }

    /// Appends one evidence item.
    #[must_use]
    pub fn push_evidence(mut self, evidence: DiagnosticEvidence) -> Self {
        self.evidence.push(evidence);
        self
    }

    /// Appends one suggested fix.
    #[must_use]
    pub fn push_fix(mut self, fix: SuggestedFix) -> Self {
        self.fixes.push(fix);
        self
    }

    /// Sets evidence confidence.
    #[must_use]
    pub fn confidence(mut self, confidence: FindingConfidence) -> Self {
        self.confidence = confidence;
        self
    }

    /// Sets diagnostic subjects.
    #[must_use]
    pub fn subjects(mut self, subjects: DiagnosticSubjects) -> Self {
        self.subjects = subjects;
        self
    }

    /// Sets diagnostic tags.
    #[must_use]
    pub fn tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Finalizes and validates a diagnostic.
    ///
    /// # Errors
    ///
    /// Returns [`DiagnosticBuildError`] when required evidence is missing or contradictory.
    pub fn build(self) -> Result<Diagnostic, DiagnosticBuildError> {
        let definition = diagnostic_definition(&self.code).ok_or_else(|| {
            DiagnosticBuildError::new("code", "diagnostic code is not part of the known registry")
        })?;

        ensure_required_evidence(definition, &self.evidence)?;
        if self.confidence == FindingConfidence::Inconclusive
            && !self
                .evidence
                .iter()
                .any(|entry| matches!(entry, DiagnosticEvidence::AnalysisLimit(_)))
        {
            return Err(DiagnosticBuildError::new(
                "confidence",
                "inconclusive confidence requires analysis-limit evidence",
            ));
        }

        Ok(Diagnostic {
            code: self.code,
            severity: definition.severity(),
            title: definition.title_value().to_owned(),
            message: self.message,
            impact: self.impact,
            labels: self.labels,
            notes: self.notes,
            help: self.help,
            evidence: self.evidence,
            fixes: self.fixes,
            properties: DiagnosticProperties {
                producer: definition.producer_value(),
                suppressibility: definition.suppressibility_value(),
                confidence: self.confidence,
                subjects: self.subjects,
                tags: self.tags,
            },
        })
    }
}

/// Validation error returned while constructing diagnostics or fixes.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("invalid {field}: {message}")]
pub struct DiagnosticBuildError {
    field: &'static str,
    message: String,
}

impl DiagnosticBuildError {
    pub(crate) fn new(field: &'static str, message: impl Into<String>) -> Self {
        Self {
            field,
            message: message.into(),
        }
    }
}

fn ensure_required_evidence(
    definition: &DiagnosticDefinition,
    evidence: &[DiagnosticEvidence],
) -> Result<(), DiagnosticBuildError> {
    match definition.evidence() {
        EvidenceRequirement::None => {
            if evidence.is_empty() {
                Ok(())
            } else {
                Err(DiagnosticBuildError::new(
                    "evidence",
                    "registry requirement is none, but evidence was provided",
                ))
            }
        }
        EvidenceRequirement::Properties => {
            if evidence
                .iter()
                .any(|entry| matches!(entry, DiagnosticEvidence::Properties(_)))
            {
                Ok(())
            } else {
                Err(DiagnosticBuildError::new(
                    "evidence",
                    "registry requirement is properties evidence",
                ))
            }
        }
        required if has_required_evidence(evidence, required) => Ok(()),
        _ => Err(DiagnosticBuildError::new(
            "evidence",
            "required evidence for this diagnostic code is missing",
        )),
    }
}

fn has_required_evidence(evidence: &[DiagnosticEvidence], required: &EvidenceRequirement) -> bool {
    match required {
        EvidenceRequirement::None => evidence.is_empty(),
        EvidenceRequirement::Provenance => evidence
            .iter()
            .any(|entry| matches!(entry, DiagnosticEvidence::Provenance(_))),
        EvidenceRequirement::Witness => evidence
            .iter()
            .any(|entry| matches!(entry, DiagnosticEvidence::Witness(_))),
        EvidenceRequirement::Trace => evidence
            .iter()
            .any(|entry| matches!(entry, DiagnosticEvidence::Trace(_))),
        EvidenceRequirement::BehaviorChange => evidence
            .iter()
            .any(|entry| matches!(entry, DiagnosticEvidence::BehaviorChange(_))),
        EvidenceRequirement::AnalysisLimit => evidence
            .iter()
            .any(|entry| matches!(entry, DiagnosticEvidence::AnalysisLimit(_))),
        EvidenceRequirement::Proof => evidence
            .iter()
            .any(|entry| matches!(entry, DiagnosticEvidence::Proof(_))),
        EvidenceRequirement::Conflict => evidence
            .iter()
            .any(|entry| matches!(entry, DiagnosticEvidence::Conflict(_))),
        EvidenceRequirement::Properties => evidence
            .iter()
            .any(|entry| matches!(entry, DiagnosticEvidence::Properties(_))),
        EvidenceRequirement::OneOf(requirements) => requirements
            .iter()
            .any(|requirement| has_required_evidence(evidence, requirement)),
    }
}

fn ensure_non_overlapping_edits(edits: &[TextEdit]) -> Result<(), DiagnosticBuildError> {
    for (index, left) in edits.iter().enumerate() {
        for right in edits.iter().skip(index + 1) {
            if overlaps(left.span(), right.span()) {
                return Err(DiagnosticBuildError::new(
                    "fix.edits",
                    "fix edits overlap within one source",
                ));
            }
        }
    }
    Ok(())
}

fn overlaps(left: Span, right: Span) -> bool {
    if left.source() != right.source() {
        return false;
    }

    let left_is_insert = left.start() == left.end();
    let right_is_insert = right.start() == right.end();
    if left_is_insert && right_is_insert {
        return left.start() == right.start();
    }

    left.start() < right.end() && right.start() < left.end()
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Arc};

    use rulery_contracts::{SourceFile, SourceId, SourceMap, SourcePath};

    use super::*;

    #[test]
    fn diagnostic_builder_enforces_registry_invariants() {
        let syntax = Diagnostic::builder(
            DiagnosticCode::new(DiagnosticCode::SYNTAX_INVALID).expect("valid code"),
            "syntax is invalid",
        )
        .expect("builder")
        .build()
        .expect("diagnostic");
        assert_eq!(syntax.title(), "Invalid source syntax");
        assert_eq!(syntax.severity(), Severity::Error);
        assert_eq!(syntax.properties().producer(), DiagnosticProducer::Parser);
        assert_eq!(
            syntax.properties().suppressibility(),
            Suppressibility::Never
        );

        let missing = Diagnostic::builder(
            DiagnosticCode::new(DiagnosticCode::DUPLICATE_DECLARATION).expect("valid code"),
            "duplicate symbol",
        )
        .expect("builder")
        .build();
        assert!(missing.is_err());

        let mut properties_payload = BTreeMap::new();
        properties_payload.insert("kind".to_owned(), "source-limit".to_owned());

        let none_rejects_evidence = Diagnostic::builder(
            DiagnosticCode::new(DiagnosticCode::SYNTAX_INVALID).expect("valid code"),
            "syntax is invalid",
        )
        .expect("builder")
        .push_evidence(DiagnosticEvidence::Properties(properties_payload.clone()))
        .build();
        assert!(none_rejects_evidence.is_err());

        let properties_missing = Diagnostic::builder(
            DiagnosticCode::new(DiagnosticCode::SOURCE_RESOURCE_LIMIT).expect("valid code"),
            "input exceeded parser budget",
        )
        .expect("builder")
        .build();
        assert!(properties_missing.is_err());

        let map = build_source_map();
        let empty_fix = SuggestedFix::new(
            StableId::new("fix.empty").expect("id"),
            "empty",
            FixApplicability::RequiresReview,
            Vec::new(),
        );
        assert!(empty_fix.is_err());

        let intersecting = SuggestedFix::new(
            StableId::new("fix.intersect").expect("id"),
            "intersecting",
            FixApplicability::RequiresReview,
            vec![
                TextEdit::new(span(&map, 1, 1, 4), "x"),
                TextEdit::new(span(&map, 1, 3, 5), "y"),
            ],
        );
        assert!(intersecting.is_err());

        let duplicate_insertions = SuggestedFix::new(
            StableId::new("fix.insert").expect("id"),
            "duplicate insertion",
            FixApplicability::RequiresReview,
            vec![
                TextEdit::new(span(&map, 1, 2, 2), "x"),
                TextEdit::new(span(&map, 1, 2, 2), "y"),
            ],
        );
        assert!(duplicate_insertions.is_err());

        let fix = SuggestedFix::new(
            StableId::new("fix.ordered").expect("id"),
            "ordered application",
            FixApplicability::MachineApplicable,
            vec![
                TextEdit::new(span(&map, 2, 0, 1), "Q"),
                TextEdit::new(span(&map, 1, 1, 2), "LONG"),
                TextEdit::new(span(&map, 1, 4, 5), "Z"),
            ],
        )
        .expect("valid fix");

        let ordered = fix.ordered_edits();
        assert_eq!(ordered[0].span().source().get(), 1);
        assert_eq!(ordered[0].span().start(), 4);
        assert_eq!(ordered[1].span().source().get(), 1);
        assert_eq!(ordered[1].span().start(), 1);
        assert_eq!(ordered[2].span().source().get(), 2);
        assert_eq!(ordered[2].span().start(), 0);

        let mut sources = BTreeMap::from([
            (SourceKey::new(1), "abcdef".to_owned()),
            (SourceKey::new(2), "uvwxyz".to_owned()),
        ]);
        fix.apply_to_sources(&mut sources).expect("apply");
        assert_eq!(
            sources.get(&SourceKey::new(1)).map(String::as_str),
            Some("aLONGcdZf")
        );
        assert_eq!(
            sources.get(&SourceKey::new(2)).map(String::as_str),
            Some("Qvwxyz")
        );
    }

    fn build_source_map() -> SourceMap {
        let mut map = SourceMap::new();
        map.insert(
            SourceKey::new(1),
            SourceFile::new(
                SourceId::new("src.a").expect("source id"),
                SourcePath::new("a.rule").expect("source path"),
                Arc::<str>::from("abcdef"),
            ),
        )
        .expect("insert source 1");
        map.insert(
            SourceKey::new(2),
            SourceFile::new(
                SourceId::new("src.b").expect("source id"),
                SourcePath::new("b.rule").expect("source path"),
                Arc::<str>::from("uvwxyz"),
            ),
        )
        .expect("insert source 2");
        map
    }

    fn span(map: &SourceMap, source: u32, start: u32, end: u32) -> Span {
        map.span(SourceKey::new(source), start, end)
            .expect("valid test span")
    }
}
