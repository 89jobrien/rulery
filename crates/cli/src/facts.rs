//! Authored case-file reading for one `explain` invocation.

use std::collections::BTreeMap;
use std::path::Path;

use rulery::contracts::{
    CaseFacts, DecimalValue, DurationValue, EnumValue, FactRootId, PolicyDate, StableId, TypeId,
    UtcInstant, Value,
};
use rulery::vocabulary::{ResolvedVocabulary, TypeDeclaration};
use serde_yaml::Value as YamlValue;

use crate::HostError;

/// Reads one authored case file into typed facts resolved against the compiled vocabulary.
///
/// The authored shape is a mapping of fact roots to nested values, exactly as the normative
/// tool-library case file is written, so the same bytes produce the same root-keyed fact map a
/// scenario or analysis witness produces.
///
/// # Errors
///
/// Returns [`HostError::Io`] when the case file cannot be read and [`HostError::Invocation`]
/// when it is not a mapping of declared roots, or contains a value the compiled vocabulary does
/// not declare.
pub fn read_case_facts(
    path: &Path,
    vocabulary: &ResolvedVocabulary,
) -> Result<CaseFacts, HostError> {
    let bytes = std::fs::read(path)
        .map_err(|error| HostError::Io(format!("{}: {error}", path.display())))?;
    let document: YamlValue = serde_yaml::from_slice(&bytes)
        .map_err(|error| HostError::Invocation(format!("{}: {error}", path.display())))?;
    let YamlValue::Mapping(roots) = document else {
        return Err(HostError::Invocation(format!(
            "{}: case facts must be a mapping of fact roots",
            path.display()
        )));
    };
    let mut facts = BTreeMap::new();
    for (key, value) in roots {
        let name = scalar_text(&key).ok_or_else(|| {
            HostError::Invocation(format!("{}: root names must be scalars", path.display()))
        })?;
        let type_id = vocabulary
            .roots
            .values()
            .find(|root| root.path.to_string() == name)
            .map(|root| root.type_id.clone())
            .ok_or_else(|| {
                HostError::Invocation(format!("`{name}` is not a declared fact root"))
            })?;
        let root_id =
            FactRootId::new(&name).map_err(|error| HostError::Invocation(error.to_string()))?;
        facts.insert(root_id, typed_value(&value, &type_id, vocabulary, &name)?);
    }
    Ok(CaseFacts::new(facts))
}

/// Resolves one authored YAML node into a typed value under its declared type.
fn typed_value(
    node: &YamlValue,
    type_id: &TypeId,
    vocabulary: &ResolvedVocabulary,
    path: &str,
) -> Result<Value, HostError> {
    let Some(declaration) = vocabulary
        .types
        .get(type_id)
        .map(|resolved| &resolved.declaration)
    else {
        return primitive_value(node, type_id, path);
    };
    match declaration {
        TypeDeclaration::Alias { target, .. } => typed_value(node, target, vocabulary, path),
        TypeDeclaration::Enum { id, variants } => {
            let symbol = scalar_text(node)
                .ok_or_else(|| HostError::Invocation(format!("`{path}` must be an enum symbol")))?;
            if !variants
                .iter()
                .any(|variant| variant.symbol.as_str() == symbol)
            {
                return Err(HostError::Invocation(format!(
                    "`{symbol}` is not a variant of `{id}`"
                )));
            }
            Ok(Value::Enum(EnumValue::new(
                id.clone(),
                StableId::new(&symbol).map_err(|error| HostError::Invocation(error.to_string()))?,
            )))
        }
        TypeDeclaration::Record { fields, .. } => {
            let YamlValue::Mapping(entries) = node else {
                return Err(HostError::Invocation(format!("`{path}` must be a mapping")));
            };
            let mut record = BTreeMap::new();
            for (key, value) in entries {
                let name = scalar_text(key).ok_or_else(|| {
                    HostError::Invocation(format!("`{path}` field names must be scalars"))
                })?;
                let field = StableId::new(&name)
                    .map_err(|error| HostError::Invocation(error.to_string()))?;
                let Some(declaration) = fields.get(&field) else {
                    return Err(HostError::Invocation(format!(
                        "`{path}.{name}` is not a declared field"
                    )));
                };
                let child = format!("{path}.{name}");
                record.insert(
                    field,
                    typed_value(value, &declaration.type_id, vocabulary, &child)?,
                );
            }
            Ok(Value::Record(record))
        }
        TypeDeclaration::List { element, .. } => {
            let YamlValue::Sequence(items) = node else {
                return Err(HostError::Invocation(format!(
                    "`{path}` must be a sequence"
                )));
            };
            items
                .iter()
                .map(|item| typed_value(item, element, vocabulary, path))
                .collect::<Result<Vec<_>, _>>()
                .map(Value::List)
        }
        TypeDeclaration::Primitive => primitive_value(node, type_id, path),
    }
}

/// Resolves one authored scalar into the primitive value its built-in type identity names.
fn primitive_value(node: &YamlValue, type_id: &TypeId, path: &str) -> Result<Value, HostError> {
    let text = || {
        scalar_text(node).ok_or_else(|| HostError::Invocation(format!("`{path}` must be a scalar")))
    };
    let invalid = |expected: &str| HostError::Invocation(format!("`{path}` is not {expected}"));
    Ok(match type_id.as_str() {
        "bool" | "boolean" => Value::Boolean(text()?.parse().map_err(|_| invalid("a boolean"))?),
        "int" | "integer" => Value::Integer(text()?.parse().map_err(|_| invalid("an integer"))?),
        "decimal" => Value::Decimal(
            DecimalValue::parse(text()?)
                .map_err(|error| HostError::Invocation(error.to_string()))?,
        ),
        "string" | "text" => Value::Text(text()?),
        "date" => Value::Date(
            PolicyDate::parse(text()?).map_err(|error| HostError::Invocation(error.to_string()))?,
        ),
        "datetime" | "date-time" => Value::DateTime(
            UtcInstant::parse(&text()?)
                .map_err(|error| HostError::Invocation(error.to_string()))?,
        ),
        "duration" => Value::Duration(
            DurationValue::parse(&text()?)
                .map_err(|error| HostError::Invocation(error.to_string()))?,
        ),
        other => {
            return Err(HostError::Invocation(format!(
                "`{other}` is not a primitive built-in"
            )));
        }
    })
}

/// Returns the canonical spelling of one authored YAML scalar node.
fn scalar_text(node: &YamlValue) -> Option<String> {
    match node {
        YamlValue::String(text) => Some(text.clone()),
        YamlValue::Number(number) => Some(number.to_string()),
        YamlValue::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}
