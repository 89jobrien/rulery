//! Human and machine renderers for Rulery artifacts.

#![forbid(unsafe_code)]

mod decision_table;
mod human;
mod json;
mod markdown;
mod sarif;
mod wire;

pub use decision_table::{
    ColumnRole, DecisionCell, DecisionColumn, DecisionProjection, DecisionRow, DecisionTable,
    DecisionTableError, DecisionTableV1, ProjectionCapabilities, ProjectionFeature, ProjectionLoss,
    ProjectionRule, build_decision_table, validate_projection,
};
pub use human::{HumanExplanation, HumanRenderer};
pub use json::{ArtifactRenderer, JsonRenderError, JsonRenderer};
pub use markdown::MarkdownRenderer;
pub use sarif::{SarifDiagnostic, SarifLocation, SarifRenderer};
pub use wire::DecisionTableEnvelope;
