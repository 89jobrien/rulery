//! Workspace architecture validation.

use std::{collections::BTreeSet, path::Path};

use cargo_metadata::{MetadataCommand, TargetKind};

use crate::{XtaskError, model};

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
    let names: BTreeSet<_> = packages
        .iter()
        .map(|package| package.name.as_str())
        .collect();

    for required in model::REQUIRED_PACKAGES {
        if !names.contains(required) {
            return Err(XtaskError::MissingMember {
                package: (*required).to_owned(),
            });
        }
    }
    if names.len() != model::REQUIRED_PACKAGES.len() {
        let unexpected = names
            .iter()
            .find(|name| !model::REQUIRED_PACKAGES.contains(name))
            .copied()
            .unwrap_or("unknown");
        return Err(XtaskError::UnexpectedMember {
            package: unexpected.to_owned(),
        });
    }

    for package in packages {
        let expected_manifest = model::manifest_path(package.name.as_str()).ok_or_else(|| {
            XtaskError::UnexpectedMember {
                package: package.name.to_string(),
            }
        })?;
        if !package
            .manifest_path
            .as_std_path()
            .ends_with(expected_manifest)
        {
            return Err(XtaskError::WrongLocation {
                package: package.name.to_string(),
                expected: expected_manifest.to_owned(),
            });
        }
        let expected_kind = model::target_kind(package.name.as_str());
        let expected_target = TargetKind::from(expected_kind);
        if !package
            .targets
            .iter()
            .any(|target| target.kind.contains(&expected_target))
        {
            return Err(XtaskError::WrongTarget {
                package: package.name.to_string(),
                expected: expected_kind.to_owned(),
            });
        }
        let allowed: BTreeSet<_> = model::allowed_dependencies(package.name.as_str())
            .iter()
            .copied()
            .collect();
        for dependency in &package.dependencies {
            let name = dependency.name.as_str();
            if names.contains(name) && !allowed.contains(name) {
                return Err(XtaskError::ForbiddenDependency {
                    package: package.name.to_string(),
                    dependency: name.to_owned(),
                });
            }
        }
    }

    Ok(())
}
