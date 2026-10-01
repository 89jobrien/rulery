//! Deterministic vocabulary resolution.

use std::collections::{BTreeMap, BTreeSet};

use rulery_contracts::{FactPath, TypeId};
use thiserror::Error;

use crate::model::{ResolvedType, ResolvedVocabulary, TypeDeclaration, VocabularyInput};

const BUILT_INS: &[&str] = &[
    "bool", "string", "int", "decimal", "date", "datetime", "duration",
];

/// Vocabulary resolution failure.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ResolveError {
    /// Attempted to redeclare a built-in type.
    #[error("built-in type `{0}` cannot be redeclared")]
    BuiltInRedeclared(String),
    /// Type identity appears multiple times.
    #[error("type `{0}` is declared multiple times")]
    DuplicateType(String),
    /// Declaration references an unknown type.
    #[error("unknown type `{0}`")]
    UnknownType(String),
    /// Declaration graph has a cycle.
    #[error("cyclic type reference at `{0}`")]
    CyclicType(String),
    /// Operational term path does not exist under declared roots.
    #[error("operational term path `{0}` is not declared")]
    UnknownTermPath(String),
    /// One root path is a prefix of another, so the shorter root always wins.
    #[error("root `{shadowing}` shadows root `{shadowed}`: no root may be a prefix of another")]
    ShadowedRoot {
        /// The shorter root, which wins path resolution for every shared path.
        shadowing: FactPath,
        /// The longer root, whose declaration never takes effect.
        shadowed: FactPath,
    },
}

/// Resolves vocabulary declarations into deterministic sorted maps.
///
/// # Errors
///
/// Returns [`ResolveError`] when declarations violate built-in, uniqueness, reference, cycle, or
/// term-path constraints.
pub fn resolve_vocabulary(input: VocabularyInput) -> Result<ResolvedVocabulary, ResolveError> {
    let mut roots = BTreeMap::new();
    let mut root_prefixes: Vec<FactPath> = Vec::new();
    for root in input.roots {
        // Reject a root nested under another before anything consumes it. Path resolution takes the
        // first declared root that prefixes a path, and a BTreeMap orders the shorter path first, so
        // the nested root would never apply. Rejecting here attributes the mistake to the vocabulary
        // that contains it, instead of letting it surface much later as an analysis that is
        // inconclusive for reasons the author cannot see.
        for existing in &root_prefixes {
            if root.path.segments().starts_with(existing.segments()) {
                return Err(ResolveError::ShadowedRoot {
                    shadowing: existing.clone(),
                    shadowed: root.path.clone(),
                });
            }
        }
        root_prefixes.push(root.path.clone());
        roots.insert(root.path.clone(), root);
    }

    let mut types = BTreeMap::<TypeId, ResolvedType>::new();
    for declaration in input.types {
        let id = match &declaration {
            TypeDeclaration::Primitive => continue,
            TypeDeclaration::Enum { id, .. }
            | TypeDeclaration::Record { id, .. }
            | TypeDeclaration::List { id, .. }
            | TypeDeclaration::Alias { id, .. } => id.clone(),
        };
        if BUILT_INS.contains(&id.as_str()) {
            return Err(ResolveError::BuiltInRedeclared(id.as_str().to_owned()));
        }
        if types.contains_key(&id) {
            return Err(ResolveError::DuplicateType(id.as_str().to_owned()));
        }
        types.insert(id.clone(), ResolvedType { id, declaration });
    }

    for declaration in types.values() {
        for reference in references(&declaration.declaration) {
            if !types.contains_key(reference)
                && !BUILT_INS
                    .iter()
                    .any(|built_in| *built_in == reference.as_str())
            {
                return Err(ResolveError::UnknownType(reference.as_str().to_owned()));
            }
        }
    }

    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    for id in types.keys() {
        dfs_cycle(id, &types, &mut visiting, &mut visited)?;
    }

    let mut terms = BTreeMap::new();
    for term in input.terms {
        if !root_prefixes
            .iter()
            .any(|root| is_under(&term.applies_to, root))
        {
            return Err(ResolveError::UnknownTermPath(term.applies_to.to_string()));
        }
        terms.insert(term.id.clone(), term);
    }

    Ok(ResolvedVocabulary {
        roots,
        types,
        terms,
    })
}

fn references(declaration: &TypeDeclaration) -> Vec<&TypeId> {
    match declaration {
        TypeDeclaration::Primitive | TypeDeclaration::Enum { .. } => Vec::new(),
        TypeDeclaration::Record { fields, .. } => {
            fields.values().map(|field| &field.type_id).collect()
        }
        TypeDeclaration::List { element, .. } => vec![element],
        TypeDeclaration::Alias { target, .. } => vec![target],
    }
}

fn dfs_cycle(
    current: &TypeId,
    types: &BTreeMap<TypeId, ResolvedType>,
    visiting: &mut BTreeSet<TypeId>,
    visited: &mut BTreeSet<TypeId>,
) -> Result<(), ResolveError> {
    if visited.contains(current) {
        return Ok(());
    }
    if !visiting.insert(current.clone()) {
        return Err(ResolveError::CyclicType(current.as_str().to_owned()));
    }

    if let Some(declaration) = types.get(current) {
        for reference in references(&declaration.declaration) {
            if types.contains_key(reference) {
                dfs_cycle(reference, types, visiting, visited)?;
            }
        }
    }

    visiting.remove(current);
    visited.insert(current.clone());
    Ok(())
}

fn is_under(path: &FactPath, root: &FactPath) -> bool {
    let path_segments = path.segments();
    let root_segments = root.segments();
    path_segments.len() >= root_segments.len()
        && root_segments
            .iter()
            .zip(path_segments.iter())
            .all(|(left, right)| left == right)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::str::FromStr;

    use rulery_contracts::{FactPath, StableId, TypeId};

    use crate::model::{
        EnumVariant, FieldDeclaration, FieldPresence, OperationalTerm, ResolvedRoot,
        TypeDeclaration, VocabularyInput,
    };

    use super::*;

    #[test]
    fn a_root_may_not_shadow_another_root() {
        // `member` and `member.name` are distinct map keys, so nothing catches them by
        // construction. Left alone they reach the analyzer as a path that resolves against the
        // *shorter* root, so the declaration silently stops meaning what it says.
        let input = VocabularyInput {
            roots: vec![
                ResolvedRoot {
                    path: FactPath::from_str("member").expect("path"),
                    type_id: TypeId::new("text").expect("type"),
                },
                ResolvedRoot {
                    path: FactPath::from_str("member.name").expect("path"),
                    type_id: TypeId::new("text").expect("type"),
                },
            ],
            ..VocabularyInput::default()
        };
        assert_eq!(
            resolve_vocabulary(input),
            Err(ResolveError::ShadowedRoot {
                shadowing: FactPath::from_str("member").expect("path"),
                shadowed: FactPath::from_str("member.name").expect("path"),
            })
        );
    }

    #[test]
    fn sibling_roots_are_not_shadowing() {
        // Sharing a first segment is not itself shadowing. Two paths of equal length can share one
        // segment without either being a prefix of the other, and those roots are both meaningful:
        // a sibling root exposes one nested path without declaring its parent.
        for paths in [
            vec!["account", "ticket"],
            vec!["member.name", "member.status"],
        ] {
            let input = VocabularyInput {
                roots: paths
                    .iter()
                    .map(|path| ResolvedRoot {
                        path: FactPath::from_str(path).expect("path"),
                        type_id: TypeId::new("text").expect("type"),
                    })
                    .collect(),
                ..VocabularyInput::default()
            };
            assert!(
                resolve_vocabulary(input).is_ok(),
                "{paths:?} must resolve: neither path is a prefix of the other"
            );
        }
    }

    #[test]
    fn vocabulary_resolution_is_typed_and_deterministic() {
        let input = VocabularyInput {
            roots: vec![ResolvedRoot {
                path: FactPath::from_str("account").expect("path"),
                type_id: TypeId::new("type.account").expect("type"),
            }],
            types: vec![
                TypeDeclaration::Record {
                    id: TypeId::new("type.account").expect("type"),
                    fields: BTreeMap::from([(
                        StableId::new("account.status").expect("field"),
                        FieldDeclaration {
                            type_id: TypeId::new("type.status").expect("type"),
                            presence: FieldPresence::Required,
                            derived: false,
                        },
                    )]),
                    closed: true,
                },
                TypeDeclaration::Enum {
                    id: TypeId::new("type.status").expect("type"),
                    variants: vec![EnumVariant {
                        symbol: StableId::new("status.active").expect("variant"),
                    }],
                },
            ],
            terms: vec![OperationalTerm {
                id: StableId::new("term.account_active").expect("term"),
                applies_to: FactPath::from_str("account.status").expect("path"),
            }],
        };

        let resolved = resolve_vocabulary(input).expect("resolved");
        assert!(
            resolved
                .roots
                .keys()
                .next()
                .expect("root")
                .to_string()
                .starts_with("account")
        );
        assert!(!resolved.types.is_empty());
        assert_eq!(resolved.terms.len(), 1);

        let built_in = VocabularyInput {
            roots: vec![],
            types: vec![TypeDeclaration::Alias {
                id: TypeId::new("string").expect("built-in"),
                target: TypeId::new("string").expect("built-in"),
            }],
            terms: vec![],
        };
        assert!(resolve_vocabulary(built_in).is_err());

        let unknown_ref = VocabularyInput {
            roots: vec![],
            types: vec![TypeDeclaration::List {
                id: TypeId::new("type.list").expect("type"),
                element: TypeId::new("type.unknown").expect("type"),
                min_items: None,
                max_items: None,
            }],
            terms: vec![],
        };
        assert!(resolve_vocabulary(unknown_ref).is_err());

        let bad_term = VocabularyInput {
            roots: vec![ResolvedRoot {
                path: FactPath::from_str("account").expect("path"),
                type_id: TypeId::new("type.account").expect("type"),
            }],
            types: vec![],
            terms: vec![OperationalTerm {
                id: StableId::new("term.bad").expect("term"),
                applies_to: FactPath::from_str("other.path").expect("path"),
            }],
        };
        assert!(resolve_vocabulary(bad_term).is_err());
    }
}
