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
        VerifyGate::Doctest,
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
        Err(XtaskError::ForbiddenDependency { .. } | XtaskError::DependencyCycle { .. })
    ));
    assert!(valid.iter().all(|package| {
        !package
            .dependencies
            .iter()
            .any(|dependency| dependency == "xtask")
    }));
}

#[test]
fn clippy_gate_lints_every_target() {
    let clippy = xtask::gate_command(VerifyGate::Clippy).expect("clippy command");
    assert!(
        clippy.args.contains(&"--all-targets"),
        "the clippy gate must lint tests and examples, not only libs and bins: {clippy:?}"
    );

    for gate in [
        VerifyGate::Format,
        VerifyGate::Nextest,
        VerifyGate::Doctest,
        VerifyGate::Rustdoc,
    ] {
        assert!(
            xtask::gate_command(gate).is_some(),
            "external gate {gate:?} must name a command"
        );
    }
    for gate in [
        VerifyGate::BootstrapCheck,
        VerifyGate::Conformance,
        VerifyGate::Architecture,
    ] {
        assert_eq!(
            xtask::gate_command(gate),
            None,
            "in-process gate {gate:?} must not name a shell command"
        );
    }
}

#[test]
fn nextest_gate_builds_every_feature() {
    let nextest = xtask::gate_command(VerifyGate::Nextest).expect("nextest command");
    assert!(
        nextest.args.contains(&"--all-features"),
        "the test gate must build optional features, or a feature-gated test is never compiled \
         or run: {nextest:?}"
    );
}

#[test]
fn doctest_gate_covers_the_workspace_and_runs() {
    let doctest = xtask::gate_command(VerifyGate::Doctest).expect("doctest command");
    assert!(
        doctest.args.contains(&"--workspace"),
        "the doctest gate must cover every crate: {doctest:?}"
    );
    assert!(
        doctest.args.contains(&"--doc"),
        "the doctest gate must select doc tests: {doctest:?}"
    );
    assert!(
        doctest.args.contains(&"--all-features"),
        "a doctest behind a feature must be compiled, not silently skipped: {doctest:?}"
    );
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
