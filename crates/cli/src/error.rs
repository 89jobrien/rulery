//! Typed classification of command failures that precede artifact production.

use rulery::diagnostics::Diagnostic;
use thiserror::Error;

/// Registry diagnostics recorded for one rejected command.
///
/// [`DiagnosticReportV1`](rulery::diagnostics::DiagnosticReportV1) keeps its fields private to the
/// diagnostics crate, so a rejection can only be reported as a `DiagnosticReportEnvelope` when every
/// diagnostic is constructible through [`Diagnostic::builder`]. This type therefore keeps the
/// constructible diagnostics separate from the plain reasons for conditions whose registry entry
/// requires evidence the CLI cannot produce.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DiagnosticFailure {
    /// Constructible registry diagnostics in canonical report order.
    pub diagnostics: Vec<Diagnostic>,
    /// Human reasons for conditions with no constructible registry diagnostic.
    pub reasons: Vec<String>,
}

impl DiagnosticFailure {
    /// Builds a failure carrying only plain reasons.
    #[must_use]
    pub fn from_reasons(reasons: Vec<String>) -> Self {
        Self {
            diagnostics: Vec::new(),
            reasons,
        }
    }

    /// Builds a failure carrying only constructible registry diagnostics.
    #[must_use]
    pub fn from_diagnostics(diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            diagnostics,
            reasons: Vec::new(),
        }
    }

    /// Returns one canonical human line per recorded condition.
    ///
    /// Registry diagnostics are rendered as `code: message` so a human report and the JSON
    /// envelope describe the same conditions in the same order.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        self.diagnostics
            .iter()
            .map(|diagnostic| format!("{}: {}", diagnostic.code().as_str(), diagnostic.message()))
            .chain(self.reasons.iter().cloned())
            .collect()
    }

    /// Returns the single-line summary used by the error `Display` implementation.
    #[must_use]
    pub fn message(&self) -> String {
        self.lines().join("; ")
    }
}

/// A command failure classified against the normative exit-condition matrix.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum HostError {
    /// A command argument value or the requested format cannot produce a runnable request.
    #[error("invalid invocation: {0}")]
    Invocation(String),
    /// A required read, write, atomic rename, or local path resolution failed.
    #[error("{0}")]
    Io(String),
    /// An internal invariant, a serialization invariant, or an unexpected adapter result failed.
    #[error("{0}")]
    Internal(String),
    /// Source loading, parsing, compilation, or lock policy rejected the package.
    #[error("{}", .0.message())]
    Rejected(DiagnosticFailure),
}

impl HostError {
    /// Wraps a rejected command carrying only plain reasons.
    #[must_use]
    pub fn rejected(reason: impl Into<String>) -> Self {
        Self::Rejected(DiagnosticFailure::from_reasons(vec![reason.into()]))
    }

    /// Wraps a rejected command carrying constructible registry diagnostics.
    #[must_use]
    pub fn rejected_with(diagnostics: Vec<Diagnostic>) -> Self {
        Self::Rejected(DiagnosticFailure::from_diagnostics(diagnostics))
    }
}
