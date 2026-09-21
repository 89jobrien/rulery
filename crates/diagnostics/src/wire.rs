//! Versioned diagnostic report wire contracts.

use serde::{Deserialize, Serialize};

use rulery_contracts::LanguageVersion;

use crate::{Diagnostic, DiagnosticBuildError};

/// Diagnostic report payload schema version 1.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticReportV1 {
    diagnostics: Vec<Diagnostic>,
    producer_identity: String,
    language_version: LanguageVersion,
}

/// Validated diagnostic report with deterministic ordering.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticReport {
    payload: DiagnosticReportV1,
}

impl DiagnosticReport {
    /// Creates a diagnostic report with deterministic diagnostic ordering.
    ///
    /// # Errors
    ///
    /// Returns [`DiagnosticBuildError`] when the producer identity is empty.
    pub fn new(
        mut diagnostics: Vec<Diagnostic>,
        producer_identity: impl Into<String>,
        language_version: LanguageVersion,
    ) -> Result<Self, DiagnosticBuildError> {
        let producer_identity = producer_identity.into();
        if producer_identity.trim().is_empty() {
            return Err(DiagnosticBuildError::new(
                "producer_identity",
                "producer identity must not be empty",
            ));
        }
        diagnostics.sort_by(|left, right| {
            left.code()
                .as_str()
                .cmp(right.code().as_str())
                .then_with(|| left.message().cmp(right.message()))
                .then_with(|| left.title().cmp(right.title()))
        });
        Ok(Self {
            payload: DiagnosticReportV1 {
                diagnostics,
                producer_identity,
                language_version,
            },
        })
    }

    /// Returns the validated version 1 payload.
    #[must_use]
    pub fn payload(&self) -> &DiagnosticReportV1 {
        &self.payload
    }
}

/// Versioned diagnostic report envelope.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "schema", content = "payload", deny_unknown_fields)]
pub enum DiagnosticReportEnvelope {
    /// Diagnostic report schema version 1.
    #[serde(rename = "rulery.diagnostic-report/v1")]
    V1(DiagnosticReportV1),
}

impl From<DiagnosticReport> for DiagnosticReportEnvelope {
    fn from(report: DiagnosticReport) -> Self {
        Self::V1(report.payload)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::{DiagnosticCode, DiagnosticEvidence};

    use super::*;

    #[test]
    fn diagnostic_report_envelope_is_strict_v1() {
        let diagnostic = Diagnostic::builder(
            DiagnosticCode::new(DiagnosticCode::SOURCE_RESOURCE_LIMIT).expect("code"),
            "source text exceeds limits",
        )
        .expect("builder")
        .push_evidence(DiagnosticEvidence::Properties(BTreeMap::from([(
            "bytes".to_owned(),
            "2048".to_owned(),
        )])))
        .build()
        .expect("diagnostic");

        let language = LanguageVersion::new(1).expect("language");
        let report = DiagnosticReport::new(vec![diagnostic], "rulery.diagnostics", language)
            .expect("report");
        let envelope = DiagnosticReportEnvelope::from(report);

        let encoded = serde_json::to_value(&envelope).expect("serialize");
        assert_eq!(
            encoded.get("schema").and_then(serde_json::Value::as_str),
            Some("rulery.diagnostic-report/v1")
        );
        let payload = encoded.get("payload").expect("payload");
        assert!(payload.get("diagnostics").is_some());
        assert!(payload.get("producer_identity").is_some());
        assert!(payload.get("language_version").is_some());

        let diagnostics = payload
            .get("diagnostics")
            .and_then(serde_json::Value::as_array)
            .expect("diagnostics array");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].get("labels").is_some());
        assert!(diagnostics[0].get("notes").is_some());
        assert!(diagnostics[0].get("help").is_some());
        assert!(diagnostics[0].get("fixes").is_some());
        assert!(diagnostics[0].get("impact").is_none());

        let no_diagnostics = DiagnosticReportEnvelope::from(
            DiagnosticReport::new(Vec::new(), "rulery.diagnostics", language).expect("report"),
        );
        let empty_payload = serde_json::to_value(no_diagnostics).expect("serialize");
        assert_eq!(
            empty_payload
                .get("payload")
                .and_then(|value| value.get("diagnostics"))
                .and_then(serde_json::Value::as_array)
                .map(std::vec::Vec::len),
            Some(0)
        );

        let unknown_field = r#"
        {
          "schema": "rulery.diagnostic-report/v1",
          "payload": {
            "diagnostics": [],
            "producer_identity": "rulery.diagnostics",
            "language_version": 1,
            "unexpected": true
          }
        }
        "#;
        assert!(serde_json::from_str::<DiagnosticReportEnvelope>(unknown_field).is_err());

        let missing_required = r#"
        {
          "schema": "rulery.diagnostic-report/v1",
          "payload": {
            "diagnostics": [],
            "language_version": 1
          }
        }
        "#;
        assert!(serde_json::from_str::<DiagnosticReportEnvelope>(missing_required).is_err());

        let unknown_schema = r#"
        {
          "schema": "rulery.diagnostic-report/v2",
          "payload": {
            "diagnostics": [],
            "producer_identity": "rulery.diagnostics",
            "language_version": 1
          }
        }
        "#;
        assert!(serde_json::from_str::<DiagnosticReportEnvelope>(unknown_schema).is_err());
    }
}
