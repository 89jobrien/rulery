//! Resolved vocabulary and structural fact validation for Rulery.
//!
//! Resolution checks type references, alias cycles, roots, and operational terms deterministically.
//! Fact validation preserves the distinction between absent, explicit null, malformed, and valid
//! evidence so the engine can apply policy strategies without losing provenance.

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
