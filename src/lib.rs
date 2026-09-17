//! Public facade for the Rulery rule compiler and evaluator.

#![forbid(unsafe_code)]

pub use rulery_analysis as analysis;
pub use rulery_compiler as compiler;
pub use rulery_contracts as contracts;
pub use rulery_diagnostics as diagnostics;
pub use rulery_emit as emit;
pub use rulery_engine as engine;
pub use rulery_ir as ir;
#[cfg(feature = "macros")]
pub use rulery_macros as macros;
pub use rulery_scenarios as scenarios;
pub use rulery_store as store;
pub use rulery_syntax as syntax;
pub use rulery_vocabulary as vocabulary;
