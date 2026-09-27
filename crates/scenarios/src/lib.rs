//! Typed compilation and exact execution of Rulery scenarios.
//!
//! Root-authored scenarios validate decision, rule, and fact references against a compiled package;
//! reason codes, tags, and the fixed evaluation instant are retained as authored. [`ScenarioRunner`]
//! compares the outcome scalar plus determining-rule, required-fact, and reason-code sets, returning
//! a strict `rulery.scenario-result/v1` payload.

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
