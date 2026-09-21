//! Public facade for the Rulery rule compiler and evaluator.
//!
//! This crate composes source loading, package assembly, compilation, evaluation, analysis, and
//! scenario workflows while re-exporting the focused workspace crates. Start with
//! [`RuleryFacade`] when embedding the full workflow, or use modules such as [`contracts`],
//! [`engine`], and [`analysis`] for lower-level integration. Package evaluation is deterministic
//! when the evaluation port supplies explicit clock/timezone inputs; runtime actions remain
//! declarative values.

#![forbid(unsafe_code)]

mod assembly;
#[path = "macros.rs"]
mod declarative_macros;
mod facade;
mod tool_library;
pub use declarative_macros::{FactBuildError, ScenarioBuildError};
pub use tool_library::{ToolLibraryExplain, tool_library_explain};

pub use assembly::{
    AssemblyCompilationInput, AssemblyError, AssemblyIntegritySet, LockMode, PackageAssembler,
    PackageAssembly, PackageAssemblyService,
};
pub use facade::{
    CompileWorkflowOutput, FacadeError, FacadeEvaluationError, FacadePorts, RuleryFacade,
    WorkflowFacade,
};

/// Hidden macro expansion surface.
#[doc(hidden)]
pub mod __private {
    pub use crate::declarative_macros::{assert_decision_contract, build_scenario};
    pub use rulery_contracts::*;
    pub use rulery_diagnostics::*;
    pub use rulery_scenarios::*;
    pub use std::collections::BTreeMap;
}

pub use rulery_analysis as analysis;
pub use rulery_compiler as compiler;
pub use rulery_contracts as contracts;
pub use rulery_diagnostics as diagnostics;
pub use rulery_emit as emit;
pub use rulery_engine as engine;
pub use rulery_ir as ir;
#[cfg(feature = "macros")]
pub use rulery_macros as macros;
#[cfg(feature = "macros")]
pub use rulery_macros::RuleFacts;
pub use rulery_scenarios as scenarios;
pub use rulery_store as store;
pub use rulery_syntax as syntax;
pub use rulery_vocabulary as vocabulary;
