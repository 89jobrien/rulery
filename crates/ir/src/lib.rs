//! Checked intermediate representation and compiled package wire model for Rulery.
//!
//! Types in this crate are produced only after syntax, vocabulary, symbol, and type validation.
//! [`CompiledPackage`] retains resolved vocabulary, source provenance, integrity inputs, and
//! deterministic content hashes required by evaluation and audit tooling.

#![forbid(unsafe_code)]

mod condition;
mod expr;
mod package;
mod wire;

pub use condition::{canonical_condition, referenced_fact_paths};
pub use expr::{Expr, ExprOperand, Operator, Predicate, ReservedOperand};
pub use package::{
    CompilationInput, CompiledAction, CompiledActionParameter, CompiledDecision, CompiledEffect,
    CompiledPackage, CompiledPackageDraft, CompiledPackageV1, CompiledRule, DecisionPrecedence,
    DecisionSemantics, DecisionSemanticsError, ExpiryPolicy, InvalidFactStrategy,
    MissingFactStrategy, PackageBuildError, PackageIntegritySet, PrecedenceDimension,
};
pub use rulery_vocabulary::{
    FieldDeclaration, FieldPresence, ResolvedRoot, ResolvedType, ResolvedVocabulary,
    TypeDeclaration,
};
pub use wire::CompiledPackageEnvelope;
