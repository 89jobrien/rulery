//! Canonical authored-source rendering.
//!
//! The canonical form is defined over the parsed document rather than over the lowered source AST,
//! so a document that omits a defaulted field keeps omitting it. Rendering sorts mapping keys, fixes
//! indentation, chooses one scalar style, and drops comments, which the parsed document does not
//! carry. The result is a total function of the parsed document and is idempotent.

use std::fmt::Write as _;

use serde_yaml::Value;

/// One space per indentation level is not canonical; the canonical form uses two.
const INDENT: usize = 2;

/// Renders one YAML document in canonical form.
///
/// # Errors
///
/// Returns a message when the supplied text is not a YAML document.
pub fn canonicalize(source: &str) -> Result<String, String> {
    let value: Value = serde_yaml::from_str(source).map_err(|error| error.to_string())?;
    Ok(render(&value))
}

/// Renders one already-parsed document in canonical form.
#[must_use]
pub fn render(value: &Value) -> String {
    let mut out = String::new();
    match value {
        Value::Mapping(mapping) if !mapping.is_empty() => {
            write_mapping(&mut out, mapping, 0);
        }
        other => {
            let _ = writeln!(out, "{}", scalar_or_flow(other));
        }
    }
    out
}

/// Writes one block mapping whose keys are sorted into ascending canonical-text order.
fn write_mapping(out: &mut String, mapping: &serde_yaml::Mapping, depth: usize) {
    let mut entries: Vec<(String, &Value)> = mapping
        .iter()
        .map(|(key, value)| (scalar_text(key), value))
        .collect();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    for (key, value) in entries {
        push_indent(out, depth);
        let _ = write!(out, "{key}:");
        write_child(out, value, depth);
    }
}

/// Writes the value that follows a mapping key, choosing inline, nested, or sequence form.
fn write_child(out: &mut String, value: &Value, depth: usize) {
    match value {
        Value::Mapping(mapping) if !mapping.is_empty() => {
            out.push('\n');
            write_mapping(out, mapping, depth + 1);
        }
        Value::Sequence(sequence) if !sequence.is_empty() => {
            out.push('\n');
            write_sequence(out, sequence, depth);
        }
        other => {
            let _ = writeln!(out, " {}", scalar_or_flow(other));
        }
    }
}

/// Writes one block sequence whose dash markers align with the owning key's column.
fn write_sequence(out: &mut String, sequence: &[Value], depth: usize) {
    for item in sequence {
        push_indent(out, depth);
        out.push('-');
        match item {
            Value::Mapping(mapping) if !mapping.is_empty() => {
                let mut nested = String::new();
                write_mapping(&mut nested, mapping, depth + 1);
                // The first line follows the dash on the same line; the rest keep their own depth.
                let (first, rest) = nested.split_once('\n').unwrap_or((nested.as_str(), ""));
                out.push(' ');
                out.push_str(first.trim_start());
                out.push('\n');
                out.push_str(rest);
            }
            Value::Sequence(inner) if !inner.is_empty() => {
                out.push('\n');
                write_sequence(out, inner, depth + 1);
            }
            other => {
                let _ = writeln!(out, " {}", scalar_or_flow(other));
            }
        }
    }
}

/// Appends the indentation for one nesting depth.
fn push_indent(out: &mut String, depth: usize) {
    for _ in 0..depth * INDENT {
        out.push(' ');
    }
}

/// Renders one value that occupies a whole line, which is only ever a scalar or an empty collection.
fn scalar_or_flow(value: &Value) -> String {
    match value {
        Value::Mapping(mapping) if mapping.is_empty() => "{}".to_owned(),
        Value::Sequence(sequence) if sequence.is_empty() => "[]".to_owned(),
        other => scalar_text(other),
    }
}

/// Renders one scalar, choosing plain or double-quoted style.
///
/// A value that already carries a YAML type renders in the form that preserves that type, so a
/// boolean stays a boolean and a number stays a number. Only a string can be at risk of being
/// implicitly retyped on the next parse, so only a string is ever quoted.
#[must_use]
pub fn scalar_text(value: &Value) -> String {
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => {
            if needs_quotes(text) {
                quote(text)
            } else {
                text.clone()
            }
        }
        other => serde_yaml::to_string(other)
            .unwrap_or_default()
            .trim_end()
            .to_owned(),
    }
}

/// Returns whether a string must be quoted to survive the next parse with the same value and type.
fn needs_quotes(text: &str) -> bool {
    if text.is_empty() || text != text.trim() {
        return true;
    }
    if RESERVED_PLAIN.contains(&text) {
        return true;
    }
    if text.parse::<i64>().is_ok() || text.parse::<f64>().is_ok() {
        return true;
    }
    // Date and date-time literals must stay quoted, which the authored grammar also requires.
    if opens_with_date(text) {
        return true;
    }
    if text.contains(": ") || text.contains(" #") || text.contains('\n') || text.contains('\t') {
        return true;
    }
    let mut characters = text.chars();
    let Some(first) = characters.next() else {
        return true;
    };
    if LEADING.contains(&first) || text.ends_with(':') {
        return true;
    }
    text.chars().any(is_disallowed)
}

/// Returns whether a character can never appear in a plain scalar after the first position.
fn is_disallowed(character: char) -> bool {
    character.is_control()
        || matches!(
            character,
            '\\' | '"'
                | '{'
                | '}'
                | '['
                | ']'
                | ','
                | '&'
                | '*'
                | '!'
                | '|'
                | '>'
                | '%'
                | '@'
                | '`'
                | '\''
        )
}

/// Double-quotes one scalar, escaping the characters YAML requires escaped.
fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if control.is_control() => {
                let _ = write!(out, "\\u{:04x}", control as u32);
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// Text that the YAML core schema would implicitly retype if it were left plain.
///
/// `y` and `n` are deliberately absent: the core schema reads them as strings, so quoting them
/// would add noise without preserving anything.
const RESERVED_PLAIN: [&str; 10] = [
    "null", "Null", "NULL", "~", "true", "True", "TRUE", "false", "False", "FALSE",
];

/// Leading characters that make a scalar ambiguous or structural.
const LEADING: [char; 18] = [
    '-', '?', ':', ',', '[', ']', '{', '}', '#', '&', '*', '!', '|', '>', '\'', '"', '%', '@',
];

/// Returns whether a string opens with a `YYYY-MM-DD` date or date-time.
fn opens_with_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() >= 10
        && bytes[0..4].iter().all(u8::is_ascii_digit)
        && bytes[4] == b'-'
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[7] == b'-'
        && bytes[8..10].iter().all(u8::is_ascii_digit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_form_sorts_keys_and_uses_two_space_indent() {
        let canonical = canonicalize("b: 2\na:\n    z: 26\n    y: 25\n").expect("valid document");
        assert_eq!("a:\n  y: 25\n  z: 26\nb: 2\n", canonical);
    }
    #[test]
    fn canonical_form_is_idempotent_and_drops_comments() {
        let once = canonicalize("# leading\nrules: # trailing\n- id: r\n  priority: 0 # note\n")
            .expect("valid document");
        assert!(!once.contains('#'), "comment survived: {once}");
        let twice = canonicalize(&once).expect("canonical output parses");
        assert_eq!(once, twice, "canonical form is not idempotent");
    }

    #[test]
    fn block_sequences_align_with_their_owning_key() {
        let canonical = canonicalize("decisions:\n  - id: a\n  - id: b\n").expect("valid document");
        assert_eq!("decisions:\n- id: a\n- id: b\n", canonical);
    }

    #[test]
    fn empty_collections_use_flow_form() {
        let canonical =
            canonicalize("actions: {}\ntypes: {}\nitems: []\n").expect("valid document");
        assert_eq!("actions: {}\nitems: []\ntypes: {}\n", canonical);
    }

    #[test]
    fn typed_values_keep_their_type_and_risky_strings_are_quoted() {
        for (authored, expected) in [
            // A real boolean stays a boolean rather than becoming a string.
            ("a: true\n", "a: true\n"),
            ("a: 26\n", "a: 26\n"),
            // A string that YAML would retype is quoted so it stays a string.
            ("a: 'true'\n", "a: \"true\"\n"),
            ("a: '1'\n", "a: \"1\"\n"),
            ("a: 2026-09-16\n", "a: \"2026-09-16\"\n"),
            // An omitted value is null, not the empty string.
            ("a:\n", "a: null\n"),
            // An authored empty string must be distinguished from null.
            ("a: ''\n", "a: \"\"\n"),
            ("a: plain\n", "a: plain\n"),
            ("a: kebab-case_id\n", "a: kebab-case_id\n"),
        ] {
            let canonical = canonicalize(authored).expect("valid document");
            assert_eq!(expected, canonical, "for {authored:?}");
        }
    }

    #[test]
    fn quote_escapes_control_characters_and_quotes() {
        assert_eq!(r#""a\"b""#, quote("a\"b"));
        assert_eq!(r#""a\\b""#, quote("a\\b"));
        assert_eq!(r#""a\nb""#, quote("a\nb"));
    }
}
