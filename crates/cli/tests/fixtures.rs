//! Conformance checks for the two fixtures the specification requires under `examples/`.
//!
//! The specification requires "a canonical-authored-form fixture and a scaffolded-package fixture,
//! both checked in under `examples/`". Both exist so the claims are artifacts rather than prose, and
//! these tests are what make them enforced. A fixture nothing asserts is documentation.
//!
//! Everything here runs the real binary rather than calling into the crate, for two reasons. The
//! scaffold and canonical-form modules are private, so testing them directly would mean widening the
//! CLI's public API for a test's benefit. And going through the binary tests the fixtures the way a
//! user meets them, which is the stronger claim anyway.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const CANONICAL_FIXTURE: &str = "examples/canonical-authored-form";
const SCAFFOLD_FIXTURE: &str = "examples/scaffolded-package";

/// Runs the built binary and returns its exit status.
///
/// `env_clear` keeps an ambient `CI` or `RULERY_*` from steering the result, matching the rest of the
/// process-boundary suite.
fn rulery(arguments: &[&str]) -> i32 {
    Command::new(env!("CARGO_BIN_EXE_rulery"))
        .args(arguments)
        .env_clear()
        .status()
        .expect("the rulery binary is built for integration tests")
        .code()
        .unwrap_or(-1)
}

fn fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("cli crate sits two levels below the workspace root")
        .join(relative)
}

/// The scaffolded fixture must be byte-identical to what `init` writes for its own directory name.
///
/// This is the point of checking the fixture in: the specification pins the scaffold's exact bytes,
/// and a test that only regenerates them proves nothing about what is on disk. A template change
/// cannot land without this failing, and an edited fixture cannot drift away from the renderer.
#[test]
fn scaffolded_fixture_is_exactly_what_init_writes() {
    let source = fixture(SCAFFOLD_FIXTURE);
    let scratch =
        std::env::temp_dir().join(format!("rulery-scaffold-fixture-{}", std::process::id()));
    let _ = fs::remove_dir_all(&scratch);
    fs::create_dir_all(&scratch).expect("scratch directory");
    // The directory name is the package identity, so the fixture name is what makes the bytes match.
    let destination = scratch.join("scaffolded-package");

    assert_eq!(
        rulery(&["init", destination.to_str().expect("utf-8 path")]),
        0,
        "init must succeed into an absent destination"
    );

    for name in ["rulery.yaml", "vocabulary.yaml", "actions.yaml"] {
        let scaffolded = fs::read(destination.join(name))
            .unwrap_or_else(|error| panic!("init did not write {name}: {error}"));
        let checked_in = fs::read(source.join(name))
            .unwrap_or_else(|error| panic!("fixture is missing {name}: {error}"));
        assert_eq!(
            scaffolded, checked_in,
            "{SCAFFOLD_FIXTURE}/{name} is not the canonical scaffold for this directory name"
        );
    }

    for directory in ["rules", "scenarios"] {
        assert!(
            destination.join(directory).is_dir(),
            "init must create an empty {directory}/"
        );
    }

    let _ = fs::remove_dir_all(&scratch);
}

/// Both fixtures must be valid packages that check clean.
#[test]
fn both_fixtures_check_without_diagnostics() {
    for relative in [CANONICAL_FIXTURE, SCAFFOLD_FIXTURE] {
        let path = fixture(relative);
        assert!(
            path.join("rulery.yaml").is_file(),
            "{relative} must carry a manifest"
        );
        assert_eq!(
            rulery(&["check", path.to_str().expect("utf-8 path")]),
            0,
            "{relative} must check clean"
        );
    }
}

/// `fmt` must consider both fixtures already canonical.
///
/// The canonical fixture's files were produced by running `fmt` rather than written by hand, so this
/// asserts they have not drifted since. If canonical form ever changes, this fails and names the
/// directory to regenerate.
#[test]
fn both_fixtures_are_already_in_canonical_authored_form() {
    for relative in [CANONICAL_FIXTURE, SCAFFOLD_FIXTURE] {
        let path = fixture(relative);
        assert_eq!(
            rulery(&["fmt", "--check", path.to_str().expect("utf-8 path")]),
            0,
            "{relative} must already be in canonical authored form"
        );
    }
}

/// The canonical fixture must be more than an empty package, or it proves nothing about form.
///
/// A package with no rules would be trivially canonical. This pins that the fixture carries a rule
/// and a real vocabulary, which is what makes the canonical-form claim meaningful.
#[test]
fn canonical_fixture_carries_real_content() {
    let root = fixture(CANONICAL_FIXTURE);
    let rules = fs::read_dir(root.join("rules"))
        .expect("the canonical fixture carries a rules directory")
        .count();
    assert!(
        rules > 0,
        "the canonical fixture must carry at least one rule"
    );

    let vocabulary = fs::read_to_string(root.join("vocabulary.yaml")).expect("readable vocabulary");
    assert!(
        vocabulary.contains("kind: record") && vocabulary.contains("kind: enum"),
        "the canonical fixture's vocabulary must exercise record and enum forms"
    );

    for path in ["rulery.yaml", "vocabulary.yaml", "actions.yaml"] {
        let text = fs::read_to_string(root.join(path)).expect("readable root file");
        assert!(
            text.ends_with('\n') && !text.ends_with("\n\n"),
            "{path} must end in exactly one newline"
        );
    }
}
