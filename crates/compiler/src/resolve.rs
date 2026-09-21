//! Symbol resolution helpers.

use std::collections::BTreeSet;
use std::fmt;

use rulery_contracts::{ActionId, DecisionId, FactPath, PackageId, StableId, TypeId};

use crate::CompilationInput;

/// Resolved symbol indices used by validation.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ResolvedSymbols {
    /// Known decision identities.
    pub decisions: BTreeSet<DecisionId>,
    /// Known action identities.
    pub actions: BTreeSet<ActionId>,
    /// Known type identities.
    pub types: BTreeSet<TypeId>,
    /// Known fact paths.
    pub fact_paths: BTreeSet<FactPath>,
    /// Known operational terms.
    pub terms: BTreeSet<StableId>,
    /// Imported package identities.
    pub imports: BTreeSet<PackageId>,
}

/// Symbol-resolution error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SymbolError {
    /// Compiler identity was empty.
    EmptyCompilerIdentity,
}

impl fmt::Display for SymbolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyCompilerIdentity => f.write_str("compiler identity must not be empty"),
        }
    }
}

impl std::error::Error for SymbolError {}

/// Resolves symbol sets from compiler input.
///
/// # Errors
///
/// Returns [`SymbolError::EmptyCompilerIdentity`] when compiler identity is blank.
pub fn resolve_symbols(input: &CompilationInput) -> Result<ResolvedSymbols, SymbolError> {
    if input.compiler_identity.trim().is_empty() {
        return Err(SymbolError::EmptyCompilerIdentity);
    }

    Ok(ResolvedSymbols {
        decisions: input.decisions.iter().cloned().collect(),
        actions: input.actions.iter().cloned().collect(),
        types: input.types.iter().cloned().collect(),
        fact_paths: input.available_fact_paths.iter().cloned().collect(),
        terms: input.available_terms.iter().cloned().collect(),
        imports: input.imported_packages.iter().cloned().collect(),
    })
}
