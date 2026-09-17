//! Structured diagnostics for Rulery.

#![forbid(unsafe_code)]

mod code;
mod registry;

pub use code::{
    DiagnosticCode, DiagnosticCodeError, DiagnosticFamily, DiagnosticProducer, EvidenceRequirement,
    Severity, Suppressibility,
};
pub use registry::{DiagnosticDefinition, diagnostic_definition, diagnostic_definitions};
