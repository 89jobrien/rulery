//! Strict YAML DTOs for the authored package manifest.

use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ManifestDto {
    pub package: PackageDto,
    #[serde(default)]
    pub semantics: SemanticsDto,
    #[serde(default)]
    pub imports: Vec<ImportDto>,
    #[serde(default)]
    pub decisions: Vec<DecisionDto>,
    #[serde(default)]
    pub vocabulary: VocabularyDto,
    #[serde(default)]
    pub actions: Vec<ActionDto>,
    #[serde(default)]
    pub scenario: Option<ScenarioDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PackageDto {
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub language: Option<u16>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SemanticsDto {
    #[serde(default = "default_timezone")]
    pub timezone: String,
    #[serde(default = "default_missing")]
    pub missing: String,
    #[serde(default = "default_invalid")]
    pub invalid: String,
    #[serde(default = "default_precedence")]
    pub precedence: String,
}

impl Default for SemanticsDto {
    fn default() -> Self {
        Self {
            timezone: default_timezone(),
            missing: default_missing(),
            invalid: default_invalid(),
            precedence: default_precedence(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ImportDto {
    #[serde(default)]
    pub package: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    pub alias: String,
    pub path: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DecisionDto {
    pub id: String,
    #[serde(default)]
    pub rules: Vec<RuleDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RuleDto {
    pub id: String,
    #[serde(default)]
    pub when: serde_yaml::Value,
    pub effect: EffectDto,
    #[serde(default)]
    pub priority: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EffectDto {
    pub kind: String,
    #[serde(default)]
    pub reasons: Vec<String>,
    #[serde(default)]
    pub escalation_to: Option<String>,
    #[serde(default)]
    pub required_facts: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct VocabularyDto {
    #[serde(default)]
    pub roots: Vec<String>,
    #[serde(default)]
    pub terms: Vec<TermDto>,
    #[serde(default)]
    pub types: Vec<TypeDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TermDto {
    pub name: String,
    #[serde(default)]
    pub applies_to: Vec<String>,
    pub definition: String,
    #[serde(default)]
    pub examples: Vec<String>,
    #[serde(default)]
    pub counterexamples: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TypeDto {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub variants: Vec<String>,
    #[serde(default)]
    pub closed: Option<bool>,
    #[serde(default)]
    pub deprecated: Option<bool>,
    #[serde(default)]
    pub min_items: Option<u64>,
    #[serde(default)]
    pub max_items: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ActionDto {
    pub id: String,
    #[serde(default)]
    pub parameters: Vec<ActionParameterDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ActionParameterDto {
    pub name: String,
    #[serde(default = "default_required")]
    pub required: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ScenarioDto {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub expectations: Vec<ScenarioExpectationDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ScenarioExpectationDto {
    pub decision: String,
    #[serde(default)]
    pub determining_rules: Vec<String>,
}

const fn default_required() -> bool {
    true
}

fn default_timezone() -> String {
    "UTC".to_owned()
}

fn default_missing() -> String {
    "unknown".to_owned()
}

fn default_invalid() -> String {
    "reject".to_owned()
}

fn default_precedence() -> String {
    "specificity".to_owned()
}
