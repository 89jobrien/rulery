//! Resolved vocabulary and value validation for Rulery.

#![forbid(unsafe_code)]

mod model;
mod resolve;
mod validate;

pub use model::{
    EnumVariant, FieldDeclaration, FieldPresence, OperationalTerm, ResolvedRoot, ResolvedType,
    ResolvedVocabulary, TypeDeclaration, VocabularyInput,
};
pub use resolve::{ResolveError, resolve_vocabulary};
pub use validate::{FactLookupState, ValidatedFacts, ValidationFailure, validate_case_facts};
