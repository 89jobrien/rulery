//! Diagnostic summarization for exit policy and machine artifacts.

use std::collections::BTreeMap;

use rulery::analysis::AnalysisReport;
use rulery::compiler::CompilerDiagnostic;
use rulery::contracts::{ContentHash, LanguageVersion, SourceMap, Span};
use rulery::diagnostics::{
    Diagnostic, DiagnosticCode, DiagnosticDefinition, DiagnosticEvidence, DiagnosticLabel,
    EvidenceRequirement, FindingConfidence, ProvenanceEvidence, Severity, diagnostic_definition,
};

use rulery::emit::{SarifDiagnostic, SarifLocation};

use crate::{HostError, WarningConfidence, WarningFinding};

/// Producer identity recorded on CLI-assembled diagnostic reports.
pub const CLI_PRODUCER: &str = "rulery-cli";

/// One-based SARIF position used when a diagnostic retains no source label.
const SARIF_FALLBACK_POSITION: (u32, u32) = (1, 1);

impl From<FindingConfidence> for WarningConfidence {
    fn from(confidence: FindingConfidence) -> Self {
        match confidence {
            FindingConfidence::Proven => Self::Proven,
            FindingConfidence::Witnessed => Self::Witnessed,
            FindingConfidence::Heuristic => Self::Heuristic,
            FindingConfidence::Inconclusive => Self::Inconclusive,
        }
    }
}

/// Deterministic summary of one command's diagnostic set.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Findings {
    /// The constructible registry diagnostics, in canonical report order.
    pub diagnostics: Vec<Diagnostic>,
    /// Whether the set contains an unsuppressed Error-severity diagnostic.
    pub has_error: bool,
    /// One promotion input per Warning-severity diagnostic, in canonical order.
    pub warnings: Vec<WarningFinding>,
    /// Canonical human lines.
    pub lines: Vec<String>,
    /// SARIF projection of every diagnostic.
    pub sarif: Vec<SarifDiagnostic>,
}

impl Findings {
    /// Summarizes already-built registry diagnostics.
    ///
    /// # Errors
    ///
    /// Returns [`HostError::Internal`] when a diagnostic's registry-derived confidence cannot be
    /// read back from its strict wire form, which is a serialization invariant failure.
    pub fn from_registry(
        diagnostics: &[Diagnostic],
        source_map: &SourceMap,
    ) -> Result<Self, HostError> {
        let mut findings = Self {
            diagnostics: diagnostics.to_vec(),
            ..Self::default()
        };
        for diagnostic in diagnostics {
            findings.has_error |= diagnostic.severity() == Severity::Error;
            if diagnostic.severity() == Severity::Warning {
                findings.warnings.push(WarningFinding {
                    suppressed: false,
                    confidence: confidence_of(diagnostic)?,
                });
            }
            findings.lines.push(line_of(diagnostic));
            findings.sarif.push(sarif_of(diagnostic, source_map));
        }
        Ok(findings)
    }

    /// Summarizes plain reasons that have no constructible registry diagnostic.
    #[must_use]
    pub fn from_reasons(reasons: &[String]) -> Self {
        Self {
            has_error: !reasons.is_empty(),
            lines: reasons.to_vec(),
            ..Self::default()
        }
    }

    /// Summarizes the diagnostics an analysis or semantic-diff report retained.
    ///
    /// # Errors
    ///
    /// Returns [`HostError::Internal`] when the retained report's diagnostics cannot be read back
    /// through the strict wire form the analyzer serialized.
    pub fn from_analysis(
        report: &AnalysisReport,
        source_map: &SourceMap,
    ) -> Result<Self, HostError> {
        let retained = serde_json::to_value(&report.payload().diagnostics)
            .map_err(|error| HostError::Internal(error.to_string()))?;
        let entries = retained
            .get("diagnostics")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| {
                HostError::Internal("analysis report retained no diagnostic collection".to_owned())
            })?;
        let diagnostics = entries
            .iter()
            .map(|entry| {
                serde_json::from_value::<Diagnostic>(entry.clone())
                    .map_err(|error| HostError::Internal(error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Self::from_registry(&diagnostics, source_map)
    }
}

/// Builds the registry diagnostics a compiler run described.
///
/// Compiler diagnostics carry the compiler crate's own provenance record, whose shape differs from
/// the diagnostics crate's `ProvenanceEvidence`; the assembly hash of the root source bundle and
/// the compiler's declared identity supply the two fields that record does not carry.
///
/// # Errors
///
/// Returns [`HostError::Internal`] when the registry rejects a diagnostic the compiler produced.
pub fn build_registry(
    entries: &[CompilerDiagnostic],
    root_hash: ContentHash,
) -> Result<Vec<Diagnostic>, HostError> {
    entries
        .iter()
        .map(|entry| {
            let evidence = requires(&entry.code, &EvidenceRequirement::Provenance).then(|| {
                provenance_evidence(
                    root_hash,
                    &entry.provenance.compiler_identity,
                    entry.provenance.language_version,
                )
            });
            let evidence = evidence.transpose()?;
            registry_diagnostic(entry.code.clone(), entry.message.clone(), evidence)
        })
        .collect()
}

/// Builds the diagnostics crate's provenance evidence for one compiler run.
///
/// `ProvenanceEvidence` keeps its fields private and exposes no constructor, so the value is
/// produced through its strict wire form. Every field is taken from a real compiler input: the
/// assembled root source-bundle hash, the version segment of the compiler's own declared identity,
/// and the language version the compiler reported.
fn provenance_evidence(
    root_hash: ContentHash,
    compiler_identity: &str,
    language_version: LanguageVersion,
) -> Result<DiagnosticEvidence, HostError> {
    let compiler_version = compiler_identity
        .rsplit('/')
        .next()
        .unwrap_or(compiler_identity);
    let wire = serde_json::json!({
        "package_hash": root_hash,
        "compiler_version": compiler_version,
        "language_version": language_version,
    });
    serde_json::from_value::<ProvenanceEvidence>(wire)
        .map(DiagnosticEvidence::Provenance)
        .map_err(|error| HostError::Internal(error.to_string()))
}

/// Builds one registry diagnostic for a condition the CLI detects itself.
///
/// # Errors
///
/// Returns [`HostError::Internal`] when the registry rejects the code or its evidence.
pub fn policy_diagnostic(
    code: &'static str,
    message: impl Into<String>,
    properties: BTreeMap<String, String>,
) -> Result<Diagnostic, HostError> {
    let code = DiagnosticCode::new(code).map_err(|error| HostError::Internal(error.to_string()))?;
    let evidence = requires(&code, &EvidenceRequirement::Properties)
        .then_some(DiagnosticEvidence::Properties(properties));
    registry_diagnostic(code, message.into(), evidence)
}

/// Builds one registry diagnostic and reports registry rejection as an internal failure.
fn registry_diagnostic(
    code: DiagnosticCode,
    message: String,
    evidence: Option<DiagnosticEvidence>,
) -> Result<Diagnostic, HostError> {
    let mut builder = Diagnostic::builder(code, message)
        .map_err(|error| HostError::Internal(error.to_string()))?;
    if let Some(evidence) = evidence {
        builder = builder.push_evidence(evidence);
    }
    builder
        .build()
        .map_err(|error| HostError::Internal(error.to_string()))
}

/// Returns whether the registry requires exactly the given evidence for a code.
fn requires(code: &DiagnosticCode, required: &EvidenceRequirement) -> bool {
    diagnostic_definition(code)
        .is_some_and(|definition: &DiagnosticDefinition| definition.evidence() == required)
}

/// Returns the canonical human line of one diagnostic.
fn line_of(diagnostic: &Diagnostic) -> String {
    format!("{}: {}", diagnostic.code().as_str(), diagnostic.message())
}

/// Reads the registry-derived confidence of one warning for `--deny-warnings` promotion.
///
/// [`Diagnostic`] and `DiagnosticProperties` both keep their fields private and neither exposes a
/// confidence accessor, so the strict wire form is the only public view of the confidence the
/// registry and builder agreed on.
fn confidence_of(diagnostic: &Diagnostic) -> Result<WarningConfidence, HostError> {
    let value =
        serde_json::to_value(diagnostic).map_err(|error| HostError::Internal(error.to_string()))?;
    let confidence = value
        .get("properties")
        .and_then(|properties| properties.get("confidence"))
        .cloned()
        .ok_or_else(|| {
            HostError::Internal("diagnostic confidence is missing from its wire form".to_owned())
        })?;
    serde_json::from_value::<FindingConfidence>(confidence)
        .map(WarningConfidence::from)
        .map_err(|error| HostError::Internal(error.to_string()))
}

/// Projects one diagnostic into the minimal SARIF result shape.
fn sarif_of(diagnostic: &Diagnostic, source_map: &SourceMap) -> SarifDiagnostic {
    SarifDiagnostic {
        code: diagnostic.code().as_str().to_owned(),
        message: diagnostic.message().to_owned(),
        location: location_of(first_label_span(diagnostic), source_map),
    }
}

/// Returns the one-based SARIF location of a diagnostic's first source label.
fn location_of(span: Option<Span>, source_map: &SourceMap) -> SarifLocation {
    let Some(span) = span else {
        return SarifLocation {
            path: String::new(),
            line: SARIF_FALLBACK_POSITION.0,
            column: SARIF_FALLBACK_POSITION.1,
        };
    };
    let (line, column) = source_map
        .line_column(span.source(), span.start())
        .unwrap_or(SARIF_FALLBACK_POSITION);
    SarifLocation {
        path: source_map
            .get(span.source())
            .map_or_else(String::new, |file| file.path().as_str().to_owned()),
        line,
        column,
    }
}

/// Returns the span a diagnostic's first source label records.
///
/// `Diagnostic` keeps `labels` private and exposes no label accessor, so the strict wire form is the
/// only public view of a diagnostic's source span. A diagnostic without labels is a legitimate
/// absence rather than a failure, so decoding is best effort here.
fn first_label_span(diagnostic: &Diagnostic) -> Option<Span> {
    serde_json::to_value(diagnostic)
        .ok()
        .and_then(|value| {
            value
                .get("labels")
                .and_then(|labels| labels.get(0))
                .cloned()
        })
        .and_then(|label| serde_json::from_value::<DiagnosticLabel>(label).ok())
        .map(|label| label.span())
}
