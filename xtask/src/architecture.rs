//! Workspace architecture validation.

use std::{collections::BTreeSet, path::Path};

use cargo_metadata::{MetadataCommand, TargetKind as CargoTargetKind};

use crate::{XtaskError, model};

/// Cargo package projection used by pure architecture validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageSnapshot {
    /// Package name.
    pub name: String,
    /// Relative manifest path.
    pub manifest: String,
    /// Target kind.
    pub target: model::TargetKind,
    /// Internal dependencies.
    pub dependencies: Vec<String>,
}

/// Validates a package graph against the declarative model.
///
/// # Errors
///
/// Returns a typed architecture error for the first deterministic violation.
pub fn validate_architecture(
    workspace_model: model::WorkspaceModel,
    packages: &[PackageSnapshot],
) -> Result<(), XtaskError> {
    let expected: BTreeSet<_> = workspace_model
        .crates
        .iter()
        .map(|spec| spec.name)
        .collect();
    let actual: BTreeSet<_> = packages
        .iter()
        .map(|package| package.name.as_str())
        .collect();
    if let Some(missing) = expected.iter().find(|name| !actual.contains(**name)) {
        return Err(XtaskError::MissingMember {
            package: (*missing).to_owned(),
        });
    }
    if let Some(unexpected) = actual.iter().find(|name| !expected.contains(**name)) {
        return Err(XtaskError::UnexpectedMember {
            package: (*unexpected).to_owned(),
        });
    }
    for spec in workspace_model.crates {
        let package = packages
            .iter()
            .find(|package| package.name == spec.name)
            .ok_or_else(|| XtaskError::MissingMember {
                package: spec.name.to_owned(),
            })?;
        if package.manifest != spec.manifest {
            return Err(XtaskError::WrongLocation {
                package: package.name.clone(),
                expected: spec.manifest.to_owned(),
            });
        }
        if package.target != spec.target {
            return Err(XtaskError::WrongTarget {
                package: package.name.clone(),
                expected: target_name(spec.target).to_owned(),
            });
        }
        let allowed: BTreeSet<_> = spec.dependencies.allowed.iter().copied().collect();
        if let Some(dependency) = package.dependencies.iter().find(|dependency| {
            expected.contains(dependency.as_str()) && !allowed.contains(dependency.as_str())
        }) {
            return Err(XtaskError::ForbiddenDependency {
                package: package.name.clone(),
                dependency: dependency.clone(),
            });
        }
    }
    detect_cycle(packages)?;
    Ok(())
}

fn detect_cycle(packages: &[PackageSnapshot]) -> Result<(), XtaskError> {
    fn visit<'a>(
        name: &'a str,
        graph: &std::collections::BTreeMap<&'a str, Vec<&'a str>>,
        visiting: &mut Vec<&'a str>,
        visited: &mut BTreeSet<&'a str>,
    ) -> Option<Vec<String>> {
        if let Some(index) = visiting.iter().position(|candidate| *candidate == name) {
            return Some(
                visiting[index..]
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect(),
            );
        }
        if !visited.insert(name) {
            return None;
        }
        visiting.push(name);
        for dependency in graph.get(name).into_iter().flatten() {
            if let Some(cycle) = visit(dependency, graph, visiting, visited) {
                return Some(cycle);
            }
        }
        visiting.pop();
        None
    }
    let names: BTreeSet<_> = packages
        .iter()
        .map(|package| package.name.as_str())
        .collect();
    let graph = packages
        .iter()
        .map(|package| {
            (
                package.name.as_str(),
                package
                    .dependencies
                    .iter()
                    .map(String::as_str)
                    .filter(|dependency| names.contains(dependency))
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut visited = BTreeSet::new();
    for name in &names {
        if let Some(packages) = visit(name, &graph, &mut Vec::new(), &mut visited) {
            return Err(XtaskError::DependencyCycle { packages });
        }
    }
    Ok(())
}

const fn target_name(target: model::TargetKind) -> &'static str {
    match target {
        model::TargetKind::Library => "lib",
        model::TargetKind::Binary => "bin",
        model::TargetKind::ProcMacro => "proc-macro",
    }
}

pub(crate) fn check(root: &Path) -> Result<(), XtaskError> {
    let metadata = MetadataCommand::new()
        .manifest_path(root.join("Cargo.toml"))
        .exec()
        .map_err(|error| XtaskError::Metadata(error.to_string()))?;
    let workspace_ids: BTreeSet<_> = metadata.workspace_members.iter().collect();
    let packages: Vec<_> = metadata
        .packages
        .iter()
        .filter(|package| workspace_ids.contains(&package.id))
        .collect();
    let snapshots = packages
        .into_iter()
        .map(|package| {
            let target = if package
                .targets
                .iter()
                .any(|target| target.kind.contains(&CargoTargetKind::ProcMacro))
            {
                model::TargetKind::ProcMacro
            } else if package
                .targets
                .iter()
                .any(|target| target.kind.contains(&CargoTargetKind::Bin))
            {
                model::TargetKind::Binary
            } else {
                model::TargetKind::Library
            };
            let manifest = package
                .manifest_path
                .as_std_path()
                .strip_prefix(root)
                .unwrap_or(package.manifest_path.as_std_path())
                .to_string_lossy()
                .into_owned();
            PackageSnapshot {
                name: package.name.to_string(),
                manifest,
                target,
                dependencies: package
                    .dependencies
                    .iter()
                    .map(|dependency| dependency.name.clone())
                    .collect(),
            }
        })
        .collect::<Vec<_>>();
    validate_architecture(model::workspace_model(), &snapshots)
}
