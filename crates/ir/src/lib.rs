//! Checked intermediate representation for Rulery.

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
