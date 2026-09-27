//! Strict YAML DTOs for each authored package document.

use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestDto {
    pub package: PackageDto,
    pub semantics: SemanticsDto,
    #[serde(default)]
    pub imports: Vec<ImportDto>,
    pub decisions: Vec<DecisionDto>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageDto {
    pub id: String,
    pub display_name: String,
    pub version: String,
    pub language_version: u16,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub authors: Vec<AuthorDto>,
    #[serde(default)]
    pub tags: Vec<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorDto {
    pub name: String,
    #[serde(default)]
    pub contact: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticsDto {
    pub timezone: String,
    pub expiry: String,
    pub missing_facts: StrategyDto,
    pub invalid_facts: StrategyDto,
    pub precedence: PrecedenceDto,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrategyDto {
    pub kind: String,
    #[serde(default)]
    pub destination: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrecedenceDto {
    pub kind: String,
    #[serde(default)]
    pub primary: Option<String>,
    #[serde(default)]
    pub outcome_ranks: BTreeMap<String, u16>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportDto {
    pub package: String,
    pub version: String,
    pub path: String,
    #[serde(default)]
    pub alias: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionDto {
    pub id: String,
    pub title: String,
    pub asks: String,
    pub input_roots: Vec<String>,
    pub default: OutcomeDto,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VocabularyDto {
    #[serde(default)]
    pub types: BTreeMap<String, TypeDto>,
    pub roots: BTreeMap<String, RootDto>,
    #[serde(default)]
    pub terms: BTreeMap<String, TermDto>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootDto {
    #[serde(rename = "type")]
    pub type_id: String,
    #[serde(default)]
    pub description: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeDto {
    pub kind: String,
    #[serde(default)]
    pub variants: BTreeMap<String, VariantDto>,
    #[serde(default)]
    pub closed: Option<bool>,
    #[serde(default)]
    pub fields: BTreeMap<String, FieldDto>,
    #[serde(default)]
    pub items: Option<String>,
    #[serde(default)]
    pub min_items: Option<u64>,
    #[serde(default)]
    pub max_items: Option<u64>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VariantDto {
    pub display_name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub deprecated: bool,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldDto {
    #[serde(rename = "type")]
    pub type_id: String,
    pub presence: String,
    #[serde(default)]
    pub description: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TermDto {
    pub display_name: String,
    pub definition: String,
    pub applies_to: Vec<String>,
    #[serde(default)]
    pub examples: Vec<String>,
    #[serde(default)]
    pub counterexamples: Vec<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionsDto {
    #[serde(default)]
    pub actions: BTreeMap<String, ActionDto>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionDto {
    pub display_name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub parameters: BTreeMap<String, ParameterDto>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParameterDto {
    #[serde(rename = "type")]
    pub type_id: String,
    #[serde(default = "required")]
    pub required: bool,
    #[serde(default)]
    pub description: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RulesDto {
    pub decision: String,
    #[serde(default)]
    pub rules: Vec<RuleDto>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleDto {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub priority: i32,
    pub when: serde_yaml::Value,
    pub effect: OutcomeDto,
    #[serde(rename = "override", default)]
    pub explicit_override: bool,
    #[serde(default)]
    pub rationale: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutcomeDto {
    pub kind: String,
    pub reasons: Vec<ReasonDto>,
    #[serde(default)]
    pub actions: Vec<InvocationDto>,
    #[serde(default)]
    pub destination: Option<String>,
    #[serde(default)]
    pub required_facts: Vec<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReasonDto {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub detail: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvocationDto {
    pub action: String,
    #[serde(default)]
    pub arguments: BTreeMap<String, serde_yaml::Value>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioDto {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    pub decision: String,
    pub at: String,
    pub given: BTreeMap<String, serde_yaml::Value>,
    pub expect: ExpectDto,
    #[serde(default)]
    pub tags: Vec<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectDto {
    pub outcome: String,
    #[serde(default)]
    pub determining_rules: Vec<QualifiedRuleDto>,
    #[serde(default)]
    pub required_facts: Vec<String>,
    #[serde(default)]
    pub reason_codes: Vec<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualifiedRuleDto {
    pub package: String,
    pub rule: String,
}
const fn required() -> bool {
    true
}
