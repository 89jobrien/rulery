//! Structured diagnostics for Rulery.

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
