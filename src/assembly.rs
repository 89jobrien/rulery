//! Deterministic recursive source-package assembly.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rulery_contracts::{
    ContentHash, LanguageVersion, LockedImport, LockedRoot, NormalizedSourceLocation, PackageId,
    PackagePath, RulebookLock, SourceFile, SourceIntegrity, SourceKey, SourceMap, Span,
};
use rulery_diagnostics::DiagnosticCode;
use rulery_store::{ImportRef, PackageStore, StoreError};
use rulery_syntax::{
    ParsedPackage, SourceCondition, SourceEffect, SourceImport, SourceParseError, SourceParser,
    SourceScenario,
};
use thiserror::Error;

/// Lock handling policy selected for assembly.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LockMode {
    /// Resolve current inputs and propose a replacement lock when necessary.
    Update,
    /// Require exact agreement with the existing lock.
    Frozen,
}

/// Integrity values for the assembled package closure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssemblyIntegritySet {
    /// Root package source integrity.
    pub root: SourceIntegrity,
    /// Transitive import integrity keyed by package identity.
    pub imports: BTreeMap<PackageId, SourceIntegrity>,
}

/// Compiler-facing assembled source input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssemblyCompilationInput {
    /// Remapped root package.
    pub root: ParsedPackage,
    /// Remapped imported packages.
    pub imports: BTreeMap<PackageId, ParsedPackage>,
    /// Package-global source map.
    pub source_map: SourceMap,
    /// Complete source integrity closure.
    pub integrity: AssemblyIntegritySet,
}

/// Completed package assembly.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageAssembly {
    /// Compiler-facing input.
    pub input: AssemblyCompilationInput,
    /// Root-package scenarios only.
    pub source_scenarios: Vec<SourceScenario>,
    /// Deterministic depth-first import traversal order.
    pub traversal_order: Vec<PackageId>,
    /// Replacement lock proposed by update mode.
    pub proposed_lock: Option<RulebookLock>,
}

/// Recursive package assembler.
#[derive(Clone, Debug)]
pub struct PackageAssembler<S, P> {
    store: S,
    parser: P,
    max_depth: usize,
    max_packages: usize,
}

impl<S, P> PackageAssembler<S, P> {
    /// Creates an assembler with conservative graph limits.
    #[must_use]
    pub const fn new(store: S, parser: P) -> Self {
        Self {
            store,
            parser,
            max_depth: 64,
            max_packages: 1_024,
        }
    }

    /// Overrides graph limits.
    #[must_use]
    pub const fn with_limits(mut self, max_depth: usize, max_packages: usize) -> Self {
        self.max_depth = max_depth;
        self.max_packages = max_packages;
        self
    }
}

/// Package assembly application port.
pub trait PackageAssemblyService {
    /// Assembly error type.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Assembles one root package and its complete import closure.
    ///
    /// # Errors
    ///
    /// Returns `Self::Error` when loading, parsing, graph validation, or span remapping fails.
    fn assemble(
        &self,
        root: &PackagePath,
        lock_mode: LockMode,
    ) -> Result<PackageAssembly, Self::Error>;
}

impl<S, P> PackageAssemblyService for PackageAssembler<S, P>
where
    S: PackageStore + Send + Sync,
    P: SourceParser + Send + Sync,
{
    type Error = AssemblyError;

    fn assemble(
        &self,
        root: &PackagePath,
        lock_mode: LockMode,
    ) -> Result<PackageAssembly, Self::Error> {
        let loaded_lock = self.store.load_lock(root)?;
        let loaded = self.store.load_source(root)?;
        let parsed = self.parser.parse_bundle(loaded.bundle())?;
        let root_id = parsed.package.metadata.package_id.clone();
        let mut state = TraversalState::new(root.as_ref().to_path_buf(), self.max_packages);
        state.nodes.insert(
            root_id.clone(),
            PackageNode {
                parsed,
                integrity: *loaded.integrity(),
                location: root.as_ref().to_path_buf(),
            },
        );
        state.active.push(root_id.clone());
        self.visit_imports(root, &root_id, 0, loaded_lock.as_ref(), &mut state)?;
        state.active.pop();

        let expected_lock = build_expected_lock(&state.nodes, &root_id, &state.root_path)?;
        enforce_lock_mode(lock_mode, loaded_lock.as_ref(), &expected_lock)?;

        let (mut root_node, mut imports, merged_map, key_map) = remap_nodes(state.nodes, &root_id)?;
        remap_parsed_package(&mut root_node.parsed, &root_id, &merged_map, &key_map)?;
        for (package_id, node) in &mut imports {
            remap_parsed_package(&mut node.parsed, package_id, &merged_map, &key_map)?;
            node.parsed.package.scenario = None;
            node.parsed.scenarios.clear();
        }

        let source_scenarios = root_node.parsed.scenarios.clone();
        let integrity = AssemblyIntegritySet {
            root: root_node.integrity,
            imports: imports
                .iter()
                .map(|(id, node)| (id.clone(), node.integrity))
                .collect(),
        };
        let imports = imports
            .into_iter()
            .map(|(id, node)| (id, node.parsed))
            .collect();

        let proposed_lock = match lock_mode {
            LockMode::Update => Some(expected_lock),
            LockMode::Frozen => None,
        };
        Ok(PackageAssembly {
            input: AssemblyCompilationInput {
                root: root_node.parsed,
                imports,
                source_map: merged_map,
                integrity,
            },
            source_scenarios,
            traversal_order: state.traversal_order,
            proposed_lock,
        })
    }
}

impl<S, P> PackageAssembler<S, P>
where
    S: PackageStore,
    P: SourceParser,
{
    fn visit_imports(
        &self,
        importer_path: &PackagePath,
        importer_id: &PackageId,
        depth: usize,
        lock: Option<&RulebookLock>,
        state: &mut TraversalState,
    ) -> Result<(), AssemblyError> {
        if depth >= self.max_depth {
            return Err(AssemblyError::ResourceLimit {
                kind: "import depth",
                max: self.max_depth,
            });
        }
        let mut imports = state
            .nodes
            .get(importer_id)
            .expect("active package is retained")
            .parsed
            .package
            .imports
            .clone();
        imports.sort_by(|left, right| left.package.cmp(&right.package));

        for import in imports {
            if state.active.contains(&import.package) {
                let mut cycle = state.active.clone();
                cycle.push(import.package.clone());
                return Err(AssemblyError::ImportCycle { cycle });
            }

            let location = resolve_declared_location(&state.root_path, &import);
            let import_ref = ImportRef {
                package: import.package.clone(),
                version: import.version.clone(),
                path: import.path.as_str().to_owned(),
                alias: import.alias.clone(),
            };
            let loaded = self
                .store
                .resolve_import(importer_path, &import_ref, lock)?;
            let parsed = self.parser.parse_bundle(loaded.bundle())?;
            if parsed.package.metadata.package_id != import.package {
                return Err(AssemblyError::PackageIdMismatch {
                    declared: import.package,
                    actual: parsed.package.metadata.package_id,
                });
            }
            if !import.version.matches(&parsed.package.metadata.version) {
                return Err(AssemblyError::VersionMismatch {
                    package: import.package,
                    actual: parsed.package.metadata.version,
                });
            }

            if let Some(existing) = state.nodes.get(&import.package) {
                if existing.location == location && existing.integrity == *loaded.integrity() {
                    continue;
                }
                return Err(AssemblyError::PackageConflict {
                    package: import.package,
                });
            }
            if state.nodes.len() >= state.max_packages {
                return Err(AssemblyError::ResourceLimit {
                    kind: "package count",
                    max: state.max_packages,
                });
            }

            let package_id = import.package;
            state.nodes.insert(
                package_id.clone(),
                PackageNode {
                    parsed,
                    integrity: *loaded.integrity(),
                    location: location.clone(),
                },
            );
            state.traversal_order.push(package_id.clone());
            state.active.push(package_id.clone());
            let next_root = PackagePath::new(location)?;
            self.visit_imports(&next_root, &package_id, depth + 1, lock, state)?;
            state.active.pop();
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct PackageNode {
    parsed: ParsedPackage,
    integrity: SourceIntegrity,
    location: PathBuf,
}

struct TraversalState {
    root_path: PathBuf,
    max_packages: usize,
    nodes: BTreeMap<PackageId, PackageNode>,
    active: Vec<PackageId>,
    traversal_order: Vec<PackageId>,
}

impl TraversalState {
    fn new(root_path: PathBuf, max_packages: usize) -> Self {
        Self {
            root_path,
            max_packages,
            nodes: BTreeMap::new(),
            active: Vec::new(),
            traversal_order: Vec::new(),
        }
    }
}

fn build_expected_lock(
    nodes: &BTreeMap<PackageId, PackageNode>,
    root_id: &PackageId,
    root_path: &Path,
) -> Result<RulebookLock, AssemblyError> {
    let root = nodes
        .get(root_id)
        .ok_or_else(|| AssemblyError::MissingRoot(root_id.clone()))?;
    let locked_root = LockedRoot::new(
        root_id.clone(),
        root.parsed.package.metadata.version.clone(),
        NormalizedSourceLocation::new("root")?,
        integrity_hash(root.integrity),
    );
    let imports = nodes
        .iter()
        .filter(|(package_id, _)| *package_id != root_id)
        .map(|(package_id, node)| {
            let relative = node.location.strip_prefix(root_path).map_err(|_| {
                AssemblyError::PackageLocationOutsideRoot {
                    package: package_id.clone(),
                }
            })?;
            let source =
                relative
                    .to_str()
                    .ok_or_else(|| AssemblyError::NonUtf8PackageLocation {
                        package: package_id.clone(),
                    })?;
            Ok(LockedImport::new(
                package_id.clone(),
                node.parsed.package.metadata.version.clone(),
                NormalizedSourceLocation::new(source.to_owned())?,
                integrity_hash(node.integrity),
            ))
        })
        .collect::<Result<Vec<_>, AssemblyError>>()?;
    RulebookLock::new(locked_root, LanguageVersion::V1, imports)
        .map_err(|_| AssemblyError::LockConstruction)
}

fn enforce_lock_mode(
    mode: LockMode,
    loaded: Option<&RulebookLock>,
    expected: &RulebookLock,
) -> Result<(), AssemblyError> {
    match mode {
        LockMode::Update => Ok(()),
        LockMode::Frozen => {
            let loaded = loaded.ok_or(AssemblyError::LockfileMissing)?;
            if loaded == expected {
                return Ok(());
            }
            let loaded_payload = loaded.payload();
            let expected_payload = expected.payload();
            let loaded_by_package = loaded_payload
                .imports()
                .iter()
                .map(|entry| (entry.package(), entry))
                .collect::<BTreeMap<_, _>>();
            let expected_by_package = expected_payload
                .imports()
                .iter()
                .map(|entry| (entry.package(), entry))
                .collect::<BTreeMap<_, _>>();
            let import_hash_changed = loaded_by_package.iter().any(|(package, loaded)| {
                expected_by_package
                    .get(package)
                    .is_some_and(|expected| loaded.content_hash() != expected.content_hash())
            });
            if import_hash_changed {
                Err(AssemblyError::ImportHashChanged)
            } else {
                Err(AssemblyError::LockfileOutOfDate)
            }
        }
    }
}

fn integrity_hash(integrity: SourceIntegrity) -> ContentHash {
    ContentHash::from_bytes(*integrity.as_bytes())
}

type SourceKeyMap = BTreeMap<(PackageId, SourceKey), SourceKey>;

fn remap_nodes(
    mut nodes: BTreeMap<PackageId, PackageNode>,
    root_id: &PackageId,
) -> Result<
    (
        PackageNode,
        BTreeMap<PackageId, PackageNode>,
        SourceMap,
        SourceKeyMap,
    ),
    AssemblyError,
> {
    let mut merged = SourceMap::new();
    let mut key_map = SourceKeyMap::new();
    let mut next = 0_u32;
    for (package_id, node) in &nodes {
        let mut files = node.parsed.source_map.iter().collect::<Vec<_>>();
        files.sort_by(|(_, left), (_, right)| left.path().cmp(right.path()));
        for (old_key, file) in files {
            let new_key = SourceKey::new(next);
            next = next
                .checked_add(1)
                .ok_or(AssemblyError::SourceKeyOverflow)?;
            merged.insert(
                new_key,
                SourceFile::new(
                    file.id().clone(),
                    file.path().clone(),
                    std::sync::Arc::<str>::from(file.content()),
                ),
            )?;
            key_map.insert((package_id.clone(), old_key), new_key);
        }
    }
    let root = nodes
        .remove(root_id)
        .ok_or_else(|| AssemblyError::MissingRoot(root_id.clone()))?;
    Ok((root, nodes, merged, key_map))
}

fn remap_parsed_package(
    parsed: &mut ParsedPackage,
    package_id: &PackageId,
    merged: &SourceMap,
    keys: &SourceKeyMap,
) -> Result<(), AssemblyError> {
    for decision in &mut parsed.package.decisions {
        for rule in &mut decision.rules {
            rule.span = remap_span(rule.span, package_id, merged, keys)?;
            remap_condition(&mut rule.when, package_id, merged, keys)?;
            remap_effect(&mut rule.effect, package_id, merged, keys)?;
        }
    }
    for action in &mut parsed.package.actions {
        action.span = remap_span(action.span, package_id, merged, keys)?;
    }
    if let Some(scenario) = &mut parsed.package.scenario {
        remap_scenario(scenario, package_id, merged, keys)?;
    }
    for scenario in &mut parsed.scenarios {
        remap_scenario(scenario, package_id, merged, keys)?;
    }
    parsed.source_map = merged.clone();
    Ok(())
}

fn remap_condition(
    condition: &mut SourceCondition,
    package_id: &PackageId,
    merged: &SourceMap,
    keys: &SourceKeyMap,
) -> Result<(), AssemblyError> {
    match condition {
        SourceCondition::All { conditions, span } | SourceCondition::Any { conditions, span } => {
            *span = remap_span(*span, package_id, merged, keys)?;
            for child in conditions {
                remap_condition(child, package_id, merged, keys)?;
            }
        }
        SourceCondition::Not { condition, span } => {
            *span = remap_span(*span, package_id, merged, keys)?;
            remap_condition(condition, package_id, merged, keys)?;
        }
        SourceCondition::Predicate(predicate) => {
            predicate.span = remap_span(predicate.span, package_id, merged, keys)?;
        }
    }
    Ok(())
}

fn remap_effect(
    effect: &mut SourceEffect,
    package_id: &PackageId,
    merged: &SourceMap,
    keys: &SourceKeyMap,
) -> Result<(), AssemblyError> {
    let span = match effect {
        SourceEffect::Approve { span, .. }
        | SourceEffect::Deny { span, .. }
        | SourceEffect::Escalate { span, .. }
        | SourceEffect::RequestInformation { span, .. } => span,
    };
    *span = remap_span(*span, package_id, merged, keys)?;
    Ok(())
}

fn remap_scenario(
    scenario: &mut SourceScenario,
    package_id: &PackageId,
    merged: &SourceMap,
    keys: &SourceKeyMap,
) -> Result<(), AssemblyError> {
    scenario.span = remap_span(scenario.span, package_id, merged, keys)?;
    for expectation in &mut scenario.expectations {
        expectation.span = remap_span(expectation.span, package_id, merged, keys)?;
    }
    Ok(())
}

fn remap_span(
    span: Span,
    package_id: &PackageId,
    merged: &SourceMap,
    keys: &SourceKeyMap,
) -> Result<Span, AssemblyError> {
    let new_key = keys
        .get(&(package_id.clone(), span.source()))
        .copied()
        .ok_or(AssemblyError::UnmappedSourceKey {
            package: package_id.clone(),
            source_key: span.source(),
        })?;
    merged
        .span(new_key, span.start(), span.end())
        .map_err(AssemblyError::from)
}

fn resolve_declared_location(root: &Path, import: &SourceImport) -> PathBuf {
    let path = root.join(import.path.as_str());
    if path.extension().is_some() {
        path.parent().map_or(path.clone(), Path::to_path_buf)
    } else {
        path
    }
}

/// Typed package assembly error.
#[derive(Debug, Error)]
pub enum AssemblyError {
    /// Store adapter failed.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// Source parser failed.
    #[error(transparent)]
    Parse(#[from] SourceParseError),
    /// Boundary contract construction failed.
    #[error(transparent)]
    Boundary(#[from] rulery_contracts::BoundaryError),
    /// Import graph contains a cycle.
    #[error("import cycle: {cycle:?}")]
    ImportCycle {
        /// Ordered cycle path.
        cycle: Vec<PackageId>,
    },
    /// Parsed package ID differs from its import declaration.
    #[error("import declared `{declared}` but parsed `{actual}`")]
    PackageIdMismatch {
        /// Declared identity.
        declared: PackageId,
        /// Parsed identity.
        actual: PackageId,
    },
    /// Parsed package version does not satisfy its import requirement.
    #[error("import `{package}` resolved incompatible version `{actual}`")]
    VersionMismatch {
        /// Imported package.
        package: PackageId,
        /// Resolved version.
        actual: rulery_contracts::Version,
    },
    /// One package identity resolved to conflicting location or content.
    #[error("package `{package}` resolved with conflicting location or content")]
    PackageConflict {
        /// Conflicting package identity.
        package: PackageId,
    },
    /// Assembly graph exceeded a configured bound.
    #[error("{kind} exceeded configured maximum {max}")]
    ResourceLimit {
        /// Bounded resource.
        kind: &'static str,
        /// Configured maximum.
        max: usize,
    },
    /// Global source-key allocation overflowed.
    #[error("global source key allocation overflowed")]
    SourceKeyOverflow,
    /// Root node disappeared during assembly.
    #[error("assembled graph is missing root package `{0}`")]
    MissingRoot(PackageId),
    /// AST span referred to a missing package-local source key.
    #[error("package `{package}` span references unmapped source key {source_key:?}")]
    UnmappedSourceKey {
        /// Package containing the span.
        package: PackageId,
        /// Missing local source key.
        source_key: SourceKey,
    },
    /// Frozen mode requires a lock file.
    #[error("frozen assembly requires rulery.lock")]
    LockfileMissing,
    /// Existing lock does not exactly match the resolved package closure.
    #[error("rulery.lock is out of date")]
    LockfileOutOfDate,
    /// An imported package's locked content hash changed.
    #[error("an imported package content hash changed")]
    ImportHashChanged,
    /// Lock construction violated an internal invariant.
    #[error("failed to construct complete package lock")]
    LockConstruction,
    /// Resolved package location escaped the root package.
    #[error("package `{package}` location is outside the root package")]
    PackageLocationOutsideRoot {
        /// Imported package identity.
        package: PackageId,
    },
    /// Resolved package location was not UTF-8.
    #[error("package `{package}` location is not valid UTF-8")]
    NonUtf8PackageLocation {
        /// Imported package identity.
        package: PackageId,
    },
}

impl AssemblyError {
    /// Returns the stable diagnostic code for graph-level failures.
    #[must_use]
    pub const fn diagnostic_code(&self) -> Option<&'static str> {
        match self {
            Self::ImportCycle { .. } => Some(DiagnosticCode::IMPORT_CYCLE),
            Self::ResourceLimit { .. } => Some(DiagnosticCode::IMPORT_RESOURCE_LIMIT),
            Self::LockfileMissing => Some(DiagnosticCode::LOCKFILE_MISSING),
            Self::LockfileOutOfDate => Some(DiagnosticCode::LOCKFILE_OUT_OF_DATE),
            Self::ImportHashChanged => Some(DiagnosticCode::IMPORT_HASH_CHANGED),
            Self::SourceKeyOverflow | Self::MissingRoot(_) | Self::UnmappedSourceKey { .. } => {
                Some(DiagnosticCode::INTERNAL_INVARIANT)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use std::sync::{Arc, Mutex};

    use rulery_contracts::{
        ContentHash, DecisionId, FactPath, LoadedSourceBundle, PackageId, QualifiedRuleId, RuleId,
        RulebookLockEnvelope, SourceBundle, SourceDocument, SourceId, SourcePath, StableId,
        Version, VersionRequirement,
    };
    use rulery_syntax::{
        SourceAction, SourceDecision, SourceExpectedDecision, SourceMetadata, SourceOperand,
        SourceOperator, SourcePackage, SourcePredicate, SourceRule, SourceSemantics,
        SourceVocabulary,
    };

    use super::*;

    #[test]
    fn assembly_traverses_and_remaps_deterministically() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let assembler = PackageAssembler::new(
            FakeStore {
                log: log.clone(),
                conflict_content: false,
                lock: None,
                writes: Arc::new(Mutex::new(0)),
            },
            FakeParser {
                mode: ParserMode::Normal,
            },
        );
        let root = PackagePath::new("/packages/root").expect("root path");
        let assembly = assembler
            .assemble(&root, LockMode::Update)
            .expect("assembly");

        assert_eq!(
            assembly
                .traversal_order
                .iter()
                .map(PackageId::as_str)
                .collect::<Vec<_>>(),
            vec!["pkg.a", "pkg.c", "pkg.b"]
        );
        assert_eq!(
            log.lock()
                .expect("log")
                .iter()
                .map(PackageId::as_str)
                .collect::<Vec<_>>(),
            vec!["pkg.a", "pkg.c", "pkg.b", "pkg.c"]
        );
        assert_eq!(assembly.input.imports.len(), 3);
        assert_eq!(assembly.source_scenarios.len(), 1);
        assert!(
            assembly
                .input
                .imports
                .values()
                .all(|parsed| parsed.scenarios.is_empty() && parsed.package.scenario.is_none())
        );

        let keys = assembly
            .input
            .source_map
            .iter()
            .map(|(key, _)| key.get())
            .collect::<Vec<_>>();
        assert_eq!(keys, (0..8).collect::<Vec<_>>());

        let root_span_key = assembly.input.root.package.decisions[0].rules[0]
            .span
            .source();
        assert!(root_span_key.get() >= 6);
        assert_all_spans_remapped(&assembly.input.root, root_span_key);
        for parsed in assembly.input.imports.values() {
            let key = parsed.package.decisions[0].rules[0].span.source();
            assert_all_spans_remapped(parsed, key);
        }

        let cycle = PackageAssembler::new(
            FakeStore {
                log: Arc::new(Mutex::new(Vec::new())),
                conflict_content: false,
                lock: None,
                writes: Arc::new(Mutex::new(0)),
            },
            FakeParser {
                mode: ParserMode::Cycle,
            },
        )
        .assemble(&root, LockMode::Update)
        .expect_err("cycle");
        assert_eq!(cycle.diagnostic_code(), Some(DiagnosticCode::IMPORT_CYCLE));

        assert!(matches!(
            assembler_for(ParserMode::IdMismatch, false)
                .assemble(&root, LockMode::Update)
                .expect_err("id mismatch"),
            AssemblyError::PackageIdMismatch { .. }
        ));
        assert!(matches!(
            assembler_for(ParserMode::VersionMismatch, false)
                .assemble(&root, LockMode::Update)
                .expect_err("version mismatch"),
            AssemblyError::VersionMismatch { .. }
        ));
        assert!(matches!(
            assembler_for(ParserMode::LocationConflict, false)
                .assemble(&root, LockMode::Update)
                .expect_err("location conflict"),
            AssemblyError::PackageConflict { .. }
        ));
        assert!(matches!(
            assembler_for(ParserMode::Normal, true)
                .assemble(&root, LockMode::Update)
                .expect_err("content conflict"),
            AssemblyError::PackageConflict { .. }
        ));
    }

    #[test]
    fn assembly_enforces_lock_modes_exactly() {
        let root = PackagePath::new("/packages/root").expect("root path");
        let writes = Arc::new(Mutex::new(0));
        let update_store = FakeStore {
            log: Arc::new(Mutex::new(Vec::new())),
            conflict_content: false,
            lock: None,
            writes: writes.clone(),
        };
        let update = PackageAssembler::new(
            update_store,
            FakeParser {
                mode: ParserMode::Normal,
            },
        )
        .assemble(&root, LockMode::Update)
        .expect("update assembly");
        let expected = update.proposed_lock.expect("replacement lock");
        assert_eq!(
            expected
                .payload()
                .imports()
                .iter()
                .map(|entry| entry.package().as_str())
                .collect::<Vec<_>>(),
            vec!["pkg.a", "pkg.b", "pkg.c"]
        );
        assert_eq!(*writes.lock().expect("writes"), 0);

        let exact = assembler_with_lock(ParserMode::Normal, Some(expected.clone()), writes.clone())
            .assemble(&root, LockMode::Frozen)
            .expect("exact frozen lock");
        assert!(exact.proposed_lock.is_none());
        assert_eq!(*writes.lock().expect("writes"), 0);

        let missing = assembler_with_lock(ParserMode::NoImports, None, writes.clone())
            .assemble(&root, LockMode::Frozen)
            .expect_err("missing lock");
        assert_eq!(
            missing.diagnostic_code(),
            Some(DiagnosticCode::LOCKFILE_MISSING)
        );

        let stale_root = mutate_lock(&expected, |value| {
            value["payload"]["root"]["version"] = serde_json::Value::String("9.0.0".to_owned());
        });
        let stale = assembler_with_lock(ParserMode::Normal, Some(stale_root), writes.clone())
            .assemble(&root, LockMode::Frozen)
            .expect_err("stale root");
        assert_eq!(
            stale.diagnostic_code(),
            Some(DiagnosticCode::LOCKFILE_OUT_OF_DATE)
        );

        let changed_hash = mutate_lock(&expected, |value| {
            value["payload"]["imports"][0]["content_hash"] = serde_json::Value::String(
                "blake3:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
                    .to_owned(),
            );
        });
        let changed = assembler_with_lock(ParserMode::Normal, Some(changed_hash), writes.clone())
            .assemble(&root, LockMode::Frozen)
            .expect_err("changed import hash");
        assert_eq!(
            changed.diagnostic_code(),
            Some(DiagnosticCode::IMPORT_HASH_CHANGED)
        );

        let additional = mutate_lock(&expected, |value| {
            let extra = value["payload"]["imports"][0].clone();
            let mut extra = extra;
            extra["package"] = serde_json::Value::String("pkg.extra".to_owned());
            value["payload"]["imports"]
                .as_array_mut()
                .expect("imports")
                .push(extra);
        });
        let additional_error =
            assembler_with_lock(ParserMode::Normal, Some(additional), writes.clone())
                .assemble(&root, LockMode::Frozen)
                .expect_err("additional import");
        assert_eq!(
            additional_error.diagnostic_code(),
            Some(DiagnosticCode::LOCKFILE_OUT_OF_DATE)
        );

        let stale_update = assembler_with_lock(
            ParserMode::Normal,
            Some(mutate_lock(&expected, |value| {
                value["payload"]["root"]["source"] =
                    serde_json::Value::String("stale-root".to_owned());
            })),
            writes.clone(),
        )
        .assemble(&root, LockMode::Update)
        .expect("update accepts stale lock");
        assert!(stale_update.proposed_lock.is_some());
        assert_eq!(*writes.lock().expect("writes"), 0);
    }

    #[derive(Clone)]
    struct FakeStore {
        log: Arc<Mutex<Vec<PackageId>>>,
        conflict_content: bool,
        lock: Option<RulebookLock>,
        writes: Arc<Mutex<u32>>,
    }

    impl PackageStore for FakeStore {
        fn load_source(&self, _root: &PackagePath) -> Result<LoadedSourceBundle, StoreError> {
            Ok(bundle("pkg.root"))
        }

        fn resolve_import(
            &self,
            _importer: &PackagePath,
            import: &ImportRef,
            _lock: Option<&RulebookLock>,
        ) -> Result<LoadedSourceBundle, StoreError> {
            let mut log = self.log.lock().expect("log");
            log.push(import.package.clone());
            let c_count = log
                .iter()
                .filter(|package| package.as_str() == "pkg.c")
                .count();
            drop(log);
            let integrity_name =
                if self.conflict_content && import.package.as_str() == "pkg.c" && c_count > 1 {
                    "pkg.c.changed"
                } else {
                    import.package.as_str()
                };
            Ok(bundle_with_integrity(
                import.package.as_str(),
                integrity_name,
            ))
        }

        fn load_lock(&self, _root: &PackagePath) -> Result<Option<RulebookLock>, StoreError> {
            Ok(self.lock.clone())
        }

        fn write_lock(&self, _root: &PackagePath, _lock: &RulebookLock) -> Result<(), StoreError> {
            *self.writes.lock().expect("writes") += 1;
            Ok(())
        }
    }

    #[derive(Clone, Copy)]
    struct FakeParser {
        mode: ParserMode,
    }

    #[derive(Clone, Copy, Eq, PartialEq)]
    enum ParserMode {
        Normal,
        NoImports,
        Cycle,
        IdMismatch,
        VersionMismatch,
        LocationConflict,
    }

    impl SourceParser for FakeParser {
        fn parse(&self, input: &[u8]) -> Result<ParsedPackage, SourceParseError> {
            let package = std::str::from_utf8(input).expect("fixture utf8").trim();
            let parsed_package = if self.mode == ParserMode::IdMismatch && package == "pkg.a" {
                "pkg.wrong"
            } else {
                package
            };
            Ok(parsed(parsed_package, self.mode))
        }

        fn parse_bundle(&self, bundle: &SourceBundle) -> Result<ParsedPackage, SourceParseError> {
            self.parse(bundle.documents()[0].content().as_bytes())
        }
    }

    fn bundle(package: &str) -> LoadedSourceBundle {
        bundle_with_integrity(package, package)
    }

    fn bundle_with_integrity(package: &str, integrity_name: &str) -> LoadedSourceBundle {
        let bundle = SourceBundle::new(vec![SourceDocument::new(
            SourcePath::new("rulery.yaml").expect("source path"),
            Arc::<str>::from(format!("{package}\n")),
        )])
        .expect("bundle");
        LoadedSourceBundle::new(
            bundle,
            SourceIntegrity::new(*ContentHash::digest(integrity_name.as_bytes()).as_bytes()),
        )
    }

    fn parsed(package: &str, mode: ParserMode) -> ParsedPackage {
        let mut source_map = SourceMap::new();
        source_map
            .insert(
                SourceKey::new(1),
                SourceFile::new(
                    SourceId::new(format!("source-{package}").replace('.', "-"))
                        .expect("source id"),
                    SourcePath::new("rules/main.yaml").expect("path"),
                    Arc::<str>::from("x"),
                ),
            )
            .expect("source");
        source_map
            .insert(
                SourceKey::new(2),
                SourceFile::new(
                    SourceId::new(format!("extra-{package}").replace('.', "-")).expect("source id"),
                    SourcePath::new("actions.yaml").expect("path"),
                    Arc::<str>::from("x"),
                ),
            )
            .expect("source");
        let span = source_map.span(SourceKey::new(1), 0, 1).expect("span");
        let id = PackageId::new(package).expect("package id");
        let scenario = scenario(&id, span);
        ParsedPackage {
            package: SourcePackage {
                metadata: SourceMetadata {
                    package_id: id.clone(),
                    version: Version::new(
                        if mode == ParserMode::VersionMismatch && package == "pkg.a" {
                            "2.0.0"
                        } else {
                            "1.0.0"
                        },
                    )
                    .expect("version"),
                    title: None,
                },
                semantics: SourceSemantics {
                    timezone: "UTC".to_owned(),
                    missing: "unknown".to_owned(),
                    invalid: "reject".to_owned(),
                    precedence: "specificity".to_owned(),
                },
                imports: fixture_imports(package, mode),
                decisions: vec![decision(span)],
                vocabulary: SourceVocabulary::default(),
                actions: vec![SourceAction {
                    id: StableId::new("action.notify").expect("action"),
                    parameters: BTreeMap::new(),
                    span,
                }],
                scenario: Some(scenario.clone()),
            },
            scenarios: vec![scenario],
            source_map,
        }
    }

    fn fixture_imports(package: &str, mode: ParserMode) -> Vec<SourceImport> {
        let ids: &[&str] = match package {
            "pkg.root" if mode == ParserMode::NoImports => &[],
            "pkg.root" => &["pkg.b", "pkg.a"],
            "pkg.a" => &["pkg.c"],
            "pkg.b" => &["pkg.c"],
            "pkg.c" if mode == ParserMode::Cycle => &["pkg.a"],
            _ => &[],
        };
        ids.iter()
            .map(|id| SourceImport {
                package: PackageId::new(*id).expect("package"),
                version: VersionRequirement::new("^1.0").expect("requirement"),
                alias: None,
                path: SourcePath::new(
                    if mode == ParserMode::LocationConflict && package == "pkg.b" && *id == "pkg.c"
                    {
                        "alternate/pkg.c".to_owned()
                    } else {
                        format!("imports/{id}")
                    },
                )
                .expect("path"),
            })
            .collect()
    }

    fn decision(span: Span) -> SourceDecision {
        let predicate = SourceCondition::Predicate(SourcePredicate {
            operator: SourceOperator::Exists,
            left: SourceOperand::Fact(FactPath::from_str("member.id").expect("fact")),
            right: None,
            span,
        });
        let condition = SourceCondition::All {
            conditions: vec![SourceCondition::Any {
                conditions: vec![SourceCondition::Not {
                    condition: Box::new(predicate),
                    span,
                }],
                span,
            }],
            span,
        };
        SourceDecision {
            id: DecisionId::new("decision.main").expect("decision"),
            rules: vec![
                SourceRule {
                    id: StableId::new("rule.approve").expect("rule"),
                    when: condition.clone(),
                    effect: SourceEffect::Approve {
                        reasons: vec!["ok".to_owned()],
                        span,
                    },
                    span,
                },
                SourceRule {
                    id: StableId::new("rule.deny").expect("rule"),
                    when: condition.clone(),
                    effect: SourceEffect::Deny {
                        reasons: vec!["no".to_owned()],
                        span,
                    },
                    span,
                },
                SourceRule {
                    id: StableId::new("rule.escalate").expect("rule"),
                    when: condition.clone(),
                    effect: SourceEffect::Escalate {
                        to: "review".to_owned(),
                        span,
                    },
                    span,
                },
                SourceRule {
                    id: StableId::new("rule.request").expect("rule"),
                    when: condition,
                    effect: SourceEffect::RequestInformation {
                        facts: vec![FactPath::from_str("member.id").expect("fact")],
                        span,
                    },
                    span,
                },
            ],
        }
    }

    fn scenario(package: &PackageId, span: Span) -> SourceScenario {
        SourceScenario {
            id: StableId::new("scenario.main").expect("scenario"),
            title: "main".to_owned(),
            expectations: vec![SourceExpectedDecision {
                decision: DecisionId::new("decision.main").expect("decision"),
                determining_rules: vec![QualifiedRuleId::new(
                    package.clone(),
                    RuleId::new("rule.approve").expect("rule"),
                )],
                span,
            }],
            span,
        }
    }

    fn assert_all_spans_remapped(parsed: &ParsedPackage, key: SourceKey) {
        for decision in &parsed.package.decisions {
            for rule in &decision.rules {
                assert_eq!(rule.span.source(), key);
                assert_condition_key(&rule.when, key);
                assert_eq!(rule.effect.span().source(), key);
            }
        }
        assert!(
            parsed
                .package
                .actions
                .iter()
                .all(|action| action.span.source() == key)
        );
        for scenario in &parsed.scenarios {
            assert_eq!(scenario.span.source(), key);
            assert!(
                scenario
                    .expectations
                    .iter()
                    .all(|expectation| expectation.span.source() == key)
            );
        }
    }

    fn assert_condition_key(condition: &SourceCondition, key: SourceKey) {
        assert_eq!(condition.span().source(), key);
        match condition {
            SourceCondition::All { conditions, .. } | SourceCondition::Any { conditions, .. } => {
                for child in conditions {
                    assert_condition_key(child, key);
                }
            }
            SourceCondition::Not { condition, .. } => assert_condition_key(condition, key),
            SourceCondition::Predicate(_) => {}
        }
    }

    fn assembler_for(
        mode: ParserMode,
        conflict_content: bool,
    ) -> PackageAssembler<FakeStore, FakeParser> {
        PackageAssembler::new(
            FakeStore {
                log: Arc::new(Mutex::new(Vec::new())),
                conflict_content,
                lock: None,
                writes: Arc::new(Mutex::new(0)),
            },
            FakeParser { mode },
        )
    }

    fn assembler_with_lock(
        mode: ParserMode,
        lock: Option<RulebookLock>,
        writes: Arc<Mutex<u32>>,
    ) -> PackageAssembler<FakeStore, FakeParser> {
        PackageAssembler::new(
            FakeStore {
                log: Arc::new(Mutex::new(Vec::new())),
                conflict_content: false,
                lock,
                writes,
            },
            FakeParser { mode },
        )
    }

    fn mutate_lock(
        lock: &RulebookLock,
        mutate: impl FnOnce(&mut serde_json::Value),
    ) -> RulebookLock {
        let mut value =
            serde_json::to_value(RulebookLockEnvelope::from(lock.clone())).expect("serialize lock");
        mutate(&mut value);
        let envelope = serde_json::from_value::<RulebookLockEnvelope>(value).expect("mutated lock");
        RulebookLock::from(envelope)
    }
}
