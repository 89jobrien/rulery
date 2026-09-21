//! Checked intermediate representation and compiled package wire model for Rulery.
//!
//! Types in this crate are produced only after syntax, vocabulary, symbol, and type validation.
//! [`CompiledPackage`] retains resolved vocabulary, source provenance, integrity inputs, and
//! deterministic content hashes required by evaluation and audit tooling.

#![forbid(unsafe_code)]

mod expr;
mod package;
mod wire;

pub use expr::{Expr, ExprOperand, Operator, Predicate, ReservedOperand};
pub use package::{
    CompilationInput, CompiledAction, CompiledActionParameter, CompiledDecision, CompiledPackage,
    CompiledPackageDraft, CompiledPackageV1, CompiledRule, PackageBuildError, PackageIntegritySet,
};
pub use rulery_vocabulary::{
    FieldDeclaration, FieldPresence, ResolvedRoot, ResolvedType, ResolvedVocabulary,
    TypeDeclaration,
};
pub use wire::CompiledPackageEnvelope;
