//! Scaffolded package content.
//!
//! The scaffold is the one package the specification names: the five paths the file layout requires,
//! carrying exactly the keys the authored grammar requires, with no field the grammar would reject.
//! It is emitted through the canonical authored form so that a freshly scaffolded package is already
//! in canonical form and `fmt` does not rewrite it.

use std::path::Path;

/// The three root files every package root contains.
pub const ROOT_FILES: [&str; 3] = ["rulery.yaml", "vocabulary.yaml", "actions.yaml"];

/// The two required directories, created empty.
pub const ROOT_DIRS: [&str; 2] = ["rules", "scenarios"];

/// The identity used when a destination name yields no character the grammar accepts.
const FALLBACK_IDENTITY: &str = "package";

/// Returns the scaffold bytes for each root file, keyed by its file name.
///
/// The package identity is derived from `destination`'s own name, so two differently named
/// destinations scaffold two different packages and one name always scaffolds the same bytes. The
/// templates below are written in a readable order and are rendered through the canonical authored
/// form, so a fresh scaffold is canonical by construction rather than by hand-ordering.
#[must_use]
pub fn scaffold_files(destination: &Path) -> Vec<(&'static str, String)> {
    let identity = derive_identity(destination);
    [
        ("rulery.yaml", manifest(&identity)),
        ("vocabulary.yaml", vocabulary()),
        ("actions.yaml", actions()),
    ]
    .into_iter()
    .map(|(name, template)| (name, canonical(&template)))
    .collect()
}

/// Renders one readable template into canonical form.
fn canonical(template: &str) -> String {
    crate::canonical::canonicalize(template)
        .unwrap_or_else(|error| panic!("the {template} scaffold must be a YAML document: {error}"))
}

/// Derives a package identity from a destination directory name.
///
/// The name is lowercased, every run of characters the stable-identifier grammar rejects, and every
/// run of hyphens, becomes a single `-`, and leading and trailing `-` are trimmed. A name that
/// leaves nothing behind falls back to `package`, so the result always satisfies the grammar.
#[must_use]
pub fn derive_identity(destination: &Path) -> String {
    let name = destination
        .canonicalize()
        .ok()
        .as_deref()
        .and_then(Path::file_name)
        .or_else(|| destination.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();

    let mut identity = String::with_capacity(name.len());
    for character in name.chars().flat_map(char::to_lowercase) {
        if is_identity_character(character) && character != '-' {
            identity.push(character);
        } else if !identity.ends_with('-') {
            identity.push('-');
        }
    }
    let trimmed = identity.trim_matches('-');
    if trimmed.is_empty() {
        return FALLBACK_IDENTITY.to_owned();
    }
    trimmed.to_owned()
}

/// Returns whether one character may appear in a stable identifier.
fn is_identity_character(character: char) -> bool {
    character.is_ascii_lowercase()
        || character.is_ascii_digit()
        || matches!(character, '.' | '_' | '-')
}

/// Renders the scaffolded manifest.
fn manifest(identity: &str) -> String {
    format!(
        "package:\n  id: {identity}\n  display_name: {identity}\n  version: 0.1.0\n  language_version: 1\n\
         semantics:\n  timezone: UTC\n  expiry: inclusive\n  missing_facts:\n    kind: preserve_unknown\n  \
         invalid_facts:\n    kind: reject_evaluation\n  precedence:\n    kind: priority_first\n\
         decisions:\n- id: default\n  title: Default decision\n  asks: Should this case be allowed?\n  \
         input_roots:\n  - input\n  default:\n    kind: deny\n    reasons:\n    - code: no_determining_rule\n      \
         message: No rule determined an outcome.\n"
    )
}

/// Renders the scaffolded vocabulary.
fn vocabulary() -> String {
    "roots:\n  input:\n    type: text\n    description: Case input under evaluation.\n".to_owned()
}

/// Renders the scaffolded action declarations, which are empty.
fn actions() -> String {
    "actions: {}\n".to_owned()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn init_derives_a_package_identity_from_the_destination_name() {
        for (name, expected) in [
            ("community-tool-library", "community-tool-library"),
            ("Tool Library", "tool-library"),
            // Underscore is a grammar character, so it survives and is not trimmed.
            ("__weird__name__", "__weird__name__"),
            // Runs of rejected characters collapse to one hyphen.
            ("a  b", "a-b"),
            ("a::b", "a-b"),
            // Only hyphens are trimmed at the boundaries.
            ("--lead--and--trail--", "lead-and-trail"),
            ("---", "package"),
            ("2026 Rules", "2026-rules"),
        ] {
            let destination = PathBuf::from("/tmp").join(name);
            assert_eq!(expected, derive_identity(&destination), "for {name:?}");
        }
    }

    #[test]
    fn scaffolded_package_is_canonical_and_free_of_diagnostics() {
        for (name, bytes) in scaffold_files(Path::new("/tmp/community-tool-library")) {
            let once = crate::canonical::canonicalize(&bytes)
                .unwrap_or_else(|error| panic!("{name} does not parse: {error}"));
            assert_eq!(bytes, once, "{name} is not already canonical");
            assert!(bytes.ends_with('\n'), "{name} has no trailing newline");
            assert!(
                crate::canonical::canonicalize(&once).expect("canonical output parses") == once,
                "{name} is not idempotent"
            );
        }
    }
}
