//! Fail-fast verification and architecture tests.

use std::cell::RefCell;

use xtask::{PackageSnapshot, TargetKind, VerifyGate, VerifyRunner, XtaskError};

#[test]
fn verify_runs_exact_fail_fast_gate_order() {
    let gates = [
        VerifyGate::BootstrapCheck,
        VerifyGate::Conformance,
        VerifyGate::Architecture,
        VerifyGate::Format,
        VerifyGate::Clippy,
        VerifyGate::Nextest,
        VerifyGate::Rustdoc,
    ];
    for failure in 0..gates.len() {
        let runner = RecordingRunner {
            seen: RefCell::new(Vec::new()),
            fail_at: Some(failure),
        };
        assert!(xtask::verify_with(&runner).is_err());
        assert_eq!(runner.seen.borrow().as_slice(), &gates[..=failure]);
    }
    let runner = RecordingRunner {
        seen: RefCell::new(Vec::new()),
        fail_at: None,
    };
    xtask::verify_with(&runner).expect("verify");
    assert_eq!(runner.seen.borrow().as_slice(), gates);

    let model = xtask::workspace_model();
    let valid = model
        .crates
        .iter()
        .map(|spec| PackageSnapshot {
            name: spec.name.to_owned(),
            manifest: spec.manifest.to_owned(),
            target: spec.target,
            dependencies: spec
                .dependencies
                .allowed
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
        })
        .collect::<Vec<_>>();
    xtask::validate_architecture(model, &valid).expect("valid model");

    let mut missing = valid.clone();
    missing.pop();
    assert!(matches!(
        xtask::validate_architecture(model, &missing),
        Err(XtaskError::MissingMember { .. })
    ));
    let mut unexpected = valid.clone();
    unexpected.push(PackageSnapshot {
        name: "unexpected".to_owned(),
        manifest: "x/Cargo.toml".to_owned(),
        target: TargetKind::Library,
        dependencies: Vec::new(),
    });
    assert!(matches!(
        xtask::validate_architecture(model, &unexpected),
        Err(XtaskError::UnexpectedMember { .. })
    ));
    let mut wrong_path = valid.clone();
    wrong_path[0].manifest = "wrong/Cargo.toml".to_owned();
    assert!(matches!(
        xtask::validate_architecture(model, &wrong_path),
        Err(XtaskError::WrongLocation { .. })
    ));
    let mut wrong_target = valid.clone();
    wrong_target[0].target = TargetKind::Binary;
    assert!(matches!(
        xtask::validate_architecture(model, &wrong_target),
        Err(XtaskError::WrongTarget { .. })
    ));
    let mut forbidden = valid.clone();
    forbidden
        .iter_mut()
        .find(|package| package.name == "rulery-contracts")
        .expect("contracts")
        .dependencies
        .push("rulery-engine".to_owned());
    assert!(matches!(
        xtask::validate_architecture(model, &forbidden),
        Err(XtaskError::ForbiddenDependency { .. })
    ));
    let mut cycle = valid.clone();
    cycle
        .iter_mut()
        .find(|package| package.name == "rulery-contracts")
        .expect("contracts")
        .dependencies
        .push("rulery".to_owned());
    assert!(matches!(
        xtask::validate_architecture(model, &cycle),
        Err(XtaskError::ForbiddenDependency { .. }) | Err(XtaskError::DependencyCycle { .. })
    ));
    assert!(valid.iter().all(|package| {
        !package
            .dependencies
            .iter()
            .any(|dependency| dependency == "xtask")
    }));
}

struct RecordingRunner {
    seen: RefCell<Vec<VerifyGate>>,
    fail_at: Option<usize>,
}
impl VerifyRunner for RecordingRunner {
    fn run(&self, gate: VerifyGate) -> Result<(), XtaskError> {
        let index = self.seen.borrow().len();
        self.seen.borrow_mut().push(gate);
        if self.fail_at == Some(index) {
            Err(XtaskError::Command(format!("failed {gate:?}")))
        } else {
            Ok(())
        }
    }
}
