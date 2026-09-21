//! Executable rulebook scenarios for Rulery.

#![forbid(unsafe_code)]

mod compile;
mod run;
mod wire;

pub use compile::{
    CompiledScenario, ExpectedDecision, ScenarioCompilationOutput, ScenarioCompiler,
    ScenarioDiagnostic, ScenarioSource, SourceExpectedDecision,
};
pub use run::{ActualDecision, EvaluationResult, ScenarioEvaluator, ScenarioRunner};
pub use wire::{
    ScenarioExpectationField, ScenarioFailure, ScenarioResult, ScenarioResultEnvelope,
    ScenarioResultError, ScenarioResultV1, ScenarioStatus,
};
