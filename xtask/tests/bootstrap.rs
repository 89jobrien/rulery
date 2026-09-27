//! xtask workspace bootstrap workflow tests.

use std::process::Command;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use xtask::{Change, ReconcileMode, WorkspaceFileSystem};

#[test]
fn bootstrap_check_accepts_scaffolded_workspace() {
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["bootstrap", "--check"])
        .output()
        .expect("xtask must start");

    assert!(
        output.status.success(),
        "bootstrap check failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn bootstrap_is_ordered_idempotent_and_non_destructive() {
    let fs = MemoryFs::default();
    fs.files.borrow_mut().insert(
        PathBuf::from("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/*\", \"xtask\"]\n# preserve\n".to_owned(),
    );
    let model = xtask::workspace_model();
    let dry = xtask::reconcile(&fs, Path::new("."), model, ReconcileMode::DryRun).expect("dry run");
    assert!(!dry.changes.is_empty());
    assert_eq!(fs.writes.get(), 0);
    let keys = dry
        .changes
        .iter()
        .map(|change| match change {
            Change::Create { path, .. }
            | Change::ReplaceGenerated { path, .. }
            | Change::EditRootManifest { path, .. } => path.clone(),
        })
        .collect::<Vec<_>>();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted);
    assert!(
        dry.changes
            .iter()
            .all(|change| !matches!(change, Change::EditRootManifest { .. }))
    );

    assert!(xtask::reconcile(&fs, Path::new("."), model, ReconcileMode::Check).is_err());
    assert_eq!(fs.writes.get(), 0);
    let applied =
        xtask::reconcile(&fs, Path::new("."), model, ReconcileMode::Apply).expect("apply");
    assert_eq!(fs.writes.get() as usize, applied.changes.len());
    assert!(
        xtask::plan_bootstrap(&fs, Path::new("."), model)
            .expect("second plan")
            .changes
            .is_empty()
    );
    assert!(
        fs.files
            .borrow()
            .values()
            .filter(|content| content.starts_with("// @generated"))
            .all(|content| content.contains("cargo xtask bootstrap"))
    );

    fs.fail.set(true);
    let before = fs.writes.get();
    assert!(xtask::reconcile(&fs, Path::new("."), model, ReconcileMode::Check).is_ok());
    assert!(xtask::reconcile(&fs, Path::new("."), model, ReconcileMode::DryRun).is_ok());
    assert_eq!(fs.writes.get(), before);
    assert!(fs.files.borrow().contains_key(Path::new("Cargo.toml")));
}

#[derive(Default)]
struct MemoryFs {
    files: RefCell<BTreeMap<PathBuf, String>>,
    writes: Cell<u32>,
    fail: Cell<bool>,
}
impl WorkspaceFileSystem for MemoryFs {
    fn read(&self, path: &Path) -> Result<Option<String>, xtask::XtaskError> {
        Ok(self.files.borrow().get(path).cloned())
    }
    fn write_atomic(&self, path: &Path, content: &str) -> Result<(), xtask::XtaskError> {
        if self.fail.get() {
            return Err(xtask::XtaskError::Bootstrap(
                "injected atomic failure".into(),
            ));
        }
        self.files
            .borrow_mut()
            .insert(path.to_path_buf(), content.to_owned());
        self.writes.set(self.writes.get() + 1);
        Ok(())
    }
}
