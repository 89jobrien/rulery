//! Structured, registry-backed diagnostics for Rulery.
//!
//! The fixed v0.1 registry defines severity, producer, suppression, and evidence requirements for
//! each code. [`DiagnosticBuilder`] validates those requirements and produces deterministic labels,
//! evidence, notes, and non-overlapping suggested fixes. [`DiagnosticReportEnvelope`] is the strict
//! machine-readable boundary.

#![forbid(unsafe_code)]

mod code;
mod model;
mod registry;
mod wire;

pub use code::{
    DiagnosticCode, DiagnosticCodeError, DiagnosticFamily, DiagnosticProducer, EvidenceRequirement,
    Severity, Suppressibility,
};
pub use model::{
    AnalysisLimitEvidence, AnalysisPhase, BehaviorChangeEvidence, Diagnostic, DiagnosticBuildError,
    DiagnosticBuilder, DiagnosticEvidence, DiagnosticLabel, DiagnosticNote, DiagnosticProperties,
    DiagnosticSubjects, FindingConfidence, FixApplicability, HelpItem, HelpPriority, LabelStyle,
    NoteKind, ProvenanceEvidence, SuggestedFix, TextEdit,
};
pub use registry::{DiagnosticDefinition, diagnostic_definition, diagnostic_definitions};
pub use wire::{DiagnosticReport, DiagnosticReportEnvelope, DiagnosticReportV1};
