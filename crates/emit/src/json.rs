//! Deterministic JSON artifact rendering without I/O ownership.

use serde::Serialize;
use thiserror::Error;

/// Renderer port for one typed artifact.
pub trait ArtifactRenderer<T> {
    /// Render error.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Renders exactly one typed artifact.
    ///
    /// # Errors
    ///
    /// Returns `Self::Error` when serialization or top-level validation fails.
    fn render(&self, artifact: &T) -> Result<Vec<u8>, Self::Error>;
}

/// Strict deterministic JSON renderer.
#[derive(Clone, Copy, Debug, Default)]
pub struct JsonRenderer;

impl<T: Serialize> ArtifactRenderer<T> for JsonRenderer {
    type Error = JsonRenderError;

    fn render(&self, artifact: &T) -> Result<Vec<u8>, Self::Error> {
        let value = serde_json::to_value(artifact).map_err(JsonRenderError::Serialization)?;
        if !value.is_object() {
            return Err(JsonRenderError::NonObjectArtifact);
        }
        serde_json::to_vec(&value).map_err(JsonRenderError::Serialization)
    }
}

impl JsonRenderer {
    /// Renders the explicitly documented scenario-result collection array.
    ///
    /// # Errors
    ///
    /// Returns [`JsonRenderError`] when serialization fails.
    pub fn render_scenario_results<T: Serialize>(
        &self,
        results: &[T],
    ) -> Result<Vec<u8>, JsonRenderError> {
        serde_json::to_vec(results).map_err(JsonRenderError::Serialization)
    }
}

/// JSON rendering failure.
#[derive(Debug, Error)]
pub enum JsonRenderError {
    /// Serialization failed.
    #[error("JSON serialization failed: {0}")]
    Serialization(serde_json::Error),
    /// Ordinary artifact rendering requires one top-level object.
    #[error("typed JSON artifact must serialize to one object")]
    NonObjectArtifact,
}

#[cfg(test)]
mod tests {
    use serde::Serialize;

    use super::*;

    #[derive(Serialize)]
    struct TestEnvelope<'a> {
        schema: &'a str,
        payload: TestPayload<'a>,
    }

    #[derive(Serialize)]
    struct TestPayload<'a> {
        hash: &'a str,
        outcome: Option<&'a str>,
        conflict: Option<&'a str>,
        instant: &'a str,
        decimal: &'a str,
    }

    #[test]
    fn json_renderer_emits_one_strict_typed_artifact() {
        let renderer = JsonRenderer;
        let schemas = [
            "rulery.compiled-package/v1",
            "rulery.diagnostic-report/v1",
            "rulery.decision-trace/v1",
            "rulery.scenario-result/v1",
            "rulery.analysis-report/v1",
            "rulery.decision-table/v1",
            "rulery.lock/v1",
        ];
        for schema in schemas {
            let artifact = TestEnvelope {
                schema,
                payload: TestPayload {
                    hash: "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    outcome: None,
                    conflict: None,
                    instant: "2026-09-16T16:00:00.000000000Z",
                    decimal: "12.340",
                },
            };
            let first = renderer.render(&artifact).expect("render");
            let second = renderer.render(&artifact).expect("render again");
            assert_eq!(first, second);
            assert_eq!(first.iter().filter(|byte| **byte == b'{').count(), 2);
            assert!(!first.contains(&b'\n'));
            let value: serde_json::Value = serde_json::from_slice(&first).expect("json");
            assert_eq!(value["schema"], schema);
            assert!(value["payload"]["outcome"].is_null());
            assert!(value["payload"]["conflict"].is_null());
            assert_eq!(value["payload"]["decimal"], "12.340");
        }

        let scenarios = [TestEnvelope {
            schema: "rulery.scenario-result/v1",
            payload: TestPayload {
                hash: "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                outcome: Some("approve"),
                conflict: None,
                instant: "2026-09-16T16:00:00.000000000Z",
                decimal: "1.0",
            },
        }];
        let array = renderer
            .render_scenario_results(&scenarios)
            .expect("explicit scenario array");
        assert!(serde_json::from_slice::<Vec<serde_json::Value>>(&array).is_ok());
        assert!(matches!(
            renderer.render(&vec![1, 2, 3]),
            Err(JsonRenderError::NonObjectArtifact)
        ));
    }
}
