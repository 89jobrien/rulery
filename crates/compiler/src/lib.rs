//! Validation and deterministic lowering for Rulery rulebooks.
//!
//! Compiler passes resolve symbols, validate references, type-check predicates and actions,
//! normalize conditions, compute static specificity, and lower checked declarations into
//! `rulery.compiled-package/v1`. [`CompilationOutput`] deliberately omits the package whenever an
//! error diagnostic exists; callers must not evaluate partially checked input.

#![forbid(unsafe_code)]

mod lower;
mod normalize;
mod resolve;
mod typecheck;
mod validate;

use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

use rulery_contracts::{
    ActionId, ContentHash, DecimalValue, DecisionId, DurationValue, EscalationId, FactPath,
    LanguageVersion, OutcomeTemplate, PackageId, PolicyDate, PolicyTimeZone, QualifiedRuleId,
    Reason, ReasonCode, Reasons, RequiredFacts, RuleId, SourceMap, StableId, TypeId, UtcInstant,
    Value, Version,
};
use rulery_diagnostics::DiagnosticCode;
use rulery_ir::{
    CompilationInput as PackageCompilationInput, CompiledAction, CompiledActionParameter,
    CompiledDecision, CompiledEffect, CompiledPackage, CompiledPackageDraft, CompiledRule,
    DecisionPrecedence, DecisionSemantics, ExpiryPolicy, Expr, ExprOperand, InvalidFactStrategy,
    MissingFactStrategy, Operator, PackageIntegritySet, PrecedenceDimension, Predicate,
    ReservedOperand,
};
use rulery_syntax::{
    ParsedPackage, SourceCondition, SourceOperand, SourceOperator, SourceOutcome, SourcePackage,
    SourceValue,
};
use rulery_vocabulary::{
    EnumVariant, FieldDeclaration, FieldPresence, OperationalTerm, ResolvedRoot,
    ResolvedVocabulary, TypeDeclaration, VocabularyInput, resolve_vocabulary,
};

pub use lower::{LoweringInput, lower_package};
pub use normalize::{
    NormalizationError, NormalizationOutput, normalize_condition, specificity_from_counts,
};
pub use resolve::{ResolvedSymbols, SymbolError, resolve_symbols};
pub use typecheck::{
    ActionCallSpec, ActionParamSpec, CheckedType, OperandSpec, TypecheckError, typecheck_action,
    typecheck_predicate,
};
pub use validate::{ValidationIssue, validate_symbols};

/// One authored rule binding used for symbol validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuleBinding {
    /// Label describing the source provenance for this rule.
    pub source_label: String,
    /// Declared decision identity.
    pub decision: DecisionId,
    /// Rule identity.
    pub rule: RuleId,
    /// Referenced fact paths.
    pub fact_paths: Vec<FactPath>,
    /// Invoked actions.
    pub action_calls: Vec<ActionId>,
    /// Referenced operational terms.
    pub terms: Vec<StableId>,
    /// Optional override target (`package::rule`).
    pub override_target: Option<String>,
    /// Optional rationale when `override_target` is present.
    pub override_rationale: Option<String>,
}

/// Symbol-validation compiler input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompilationInput {
    /// Package identity.
    pub package_id: PackageId,
    /// Package semantic version.
    pub package_version: Version,
    /// Language version.
    pub language_version: LanguageVersion,
    /// Compiler identity.
    pub compiler_identity: String,
    /// Source map retained for compiled package construction.
    pub source_map: SourceMap,
    /// Resolved vocabulary retained for compiled package construction.
    pub vocabulary: ResolvedVocabulary,
    /// Declared decisions.
    pub decisions: Vec<DecisionId>,
    /// Declared rules.
    pub rules: Vec<RuleBinding>,
    /// Declared actions.
    pub actions: Vec<ActionId>,
    /// Declared types.
    pub types: Vec<TypeId>,
    /// Available fact paths.
    pub available_fact_paths: Vec<FactPath>,
    /// Available terms.
    pub available_terms: Vec<StableId>,
    /// Imported package ids available for qualification.
    pub imported_packages: Vec<PackageId>,
}

/// Minimal compiler provenance evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProvenanceEvidence {
    /// Package identity under compilation.
    pub package_id: PackageId,
    /// Compiler identity.
    pub compiler_identity: String,
    /// Language version.
    pub language_version: LanguageVersion,
}

/// Compiler-produced diagnostic entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompilerDiagnostic {
    /// Stable diagnostic code.
    pub code: DiagnosticCode,
    /// Human-readable message.
    pub message: String,
    /// Source provenance label.
    pub source_label: String,
    /// Required provenance evidence.
    pub provenance: ProvenanceEvidence,
}

/// Compiler output with optional compiled package.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompilationOutput {
    /// Compiled package when no error diagnostics exist.
    pub package: Option<CompiledPackage>,
    /// Deterministically ordered diagnostics.
    pub diagnostics: Vec<CompilerDiagnostic>,
}

/// Fully assembled authored sources accepted by the production compiler.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceCompilationInput {
    /// Root package with globally remapped spans.
    pub root: ParsedPackage,
    /// Imported package closure, keyed deterministically by package identity.
    pub imports: BTreeMap<PackageId, ParsedPackage>,
    /// Complete remapped source catalog.
    pub source_map: SourceMap,
    /// Hash of the assembled root source bundle.
    pub source_bundle_hash: ContentHash,
    /// Optional lockfile hash for frozen assembly.
    pub lock_hash: Option<ContentHash>,
}

/// Symbol validation compiler.
#[derive(Clone, Debug, Default)]
pub struct PolicyCompiler;

impl PolicyCompiler {
    /// Compiles and validates symbol declarations.
    #[must_use]
    pub fn compile(&self, input: &CompilationInput) -> CompilationOutput {
        let resolved = resolve_symbols(input);
        let mut diagnostics = validate_symbols(input, &resolved)
            .into_iter()
            .map(|issue| issue.into_diagnostic(input))
            .collect::<Vec<_>>();
        diagnostics.sort_by(|left, right| {
            left.code
                .as_str()
                .cmp(right.code.as_str())
                .then_with(|| left.source_label.cmp(&right.source_label))
                .then_with(|| left.message.cmp(&right.message))
        });

        let has_error = diagnostics
            .iter()
            .any(|entry| entry.code.as_str() != DiagnosticCode::UNDEFINED_OPERATIONAL_TERM);

        let package = if has_error {
            None
        } else {
            build_empty_package(input)
        };

        CompilationOutput {
            package,
            diagnostics,
        }
    }
}

fn source_vocabulary(package: &SourcePackage) -> Result<ResolvedVocabulary, String> {
    let vocabulary = &package.vocabulary;
    let roots = vocabulary
        .roots
        .iter()
        .map(|(path, root)| {
            Ok(ResolvedRoot {
                path: FactPath::from_str(path.as_str()).map_err(|error| error.to_string())?,
                type_id: TypeId::new(root.type_id.clone()).map_err(|error| error.to_string())?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let types = vocabulary
        .types
        .iter()
        .map(|(name, ty)| source_type(name, ty))
        .collect::<Result<Vec<_>, String>>()?;
    let terms = vocabulary
        .terms
        .iter()
        .flat_map(|(id, term)| {
            term.applies_to.iter().map(move |path| OperationalTerm {
                id: id.clone(),
                applies_to: path.clone(),
            })
        })
        .collect();
    resolve_vocabulary(VocabularyInput {
        roots,
        types,
        terms,
    })
    .map_err(|error| error.to_string())
}

fn source_type(
    name: &StableId,
    source: &rulery_syntax::SourceType,
) -> Result<TypeDeclaration, String> {
    let id = TypeId::new(name.as_str()).map_err(|error| error.to_string())?;
    match source.kind.as_str() {
        "enum" => Ok(TypeDeclaration::Enum {
            id,
            variants: source
                .variants
                .keys()
                .cloned()
                .map(|symbol| EnumVariant { symbol })
                .collect(),
        }),
        "record" => Ok(TypeDeclaration::Record {
            id,
            fields: source
                .fields
                .iter()
                .map(|(name, field)| {
                    let presence = match field.presence.as_str() {
                        "required" => FieldPresence::Required,
                        "optional" | "derived" => FieldPresence::Optional,
                        _ => return Err(format!("invalid presence `{}`", field.presence)),
                    };
                    Ok((
                        name.clone(),
                        FieldDeclaration {
                            type_id: TypeId::new(canonical_type_name(&field.type_id))
                                .map_err(|error| error.to_string())?,
                            presence,
                            derived: field.presence == "derived",
                        },
                    ))
                })
                .collect::<Result<_, String>>()?,
            closed: source.closed.unwrap_or(false),
        }),
        "list" => Ok(TypeDeclaration::List {
            id,
            element: TypeId::new(source.items.clone().ok_or("list has no items type")?)
                .map(|id| TypeId::new(canonical_type_name(id.as_str())).expect("canonical type"))
                .map_err(|error| error.to_string())?,
            min_items: source.min_items,
            max_items: source.max_items,
        }),
        _ => Err(format!("unsupported type kind `{}`", source.kind)),
    }
}

fn canonical_type_name(value: &str) -> String {
    match value {
        "boolean" => "bool".to_owned(),
        "integer" => "int".to_owned(),
        "text" => "string".to_owned(),
        "date-time" => "datetime".to_owned(),
        _ => value.to_owned(),
    }
}

fn source_actions(
    package: &SourcePackage,
    diagnostics: &mut Vec<CompilerDiagnostic>,
    provenance: &ProvenanceEvidence,
) -> Vec<CompiledAction> {
    let mut seen = BTreeSet::new();
    package
        .actions
        .iter()
        .filter_map(|action| {
            let Ok(id) = ActionId::new(action.id.as_str()) else {
                diagnostics.push(source_diagnostic(
                    DiagnosticCode::UNKNOWN_SYMBOL,
                    format!("action id `{}` is invalid", action.id),
                    label(action.span),
                    provenance,
                ));
                return None;
            };
            if !seen.insert(id.clone()) {
                diagnostics.push(source_diagnostic(
                    DiagnosticCode::DUPLICATE_DECLARATION,
                    format!("duplicate action declaration `{id}`"),
                    label(action.span),
                    provenance,
                ));
                return None;
            }
            Some(CompiledAction::new(
                id,
                action
                    .parameters
                    .iter()
                    .map(|(name, parameter)| {
                        (
                            name.clone(),
                            CompiledActionParameter::new(parameter.required),
                        )
                    })
                    .collect(),
            ))
        })
        .collect()
}

fn action_specs(
    package: &SourcePackage,
    vocabulary: &ResolvedVocabulary,
) -> BTreeMap<ActionId, BTreeMap<StableId, ActionParamSpec>> {
    package
        .actions
        .iter()
        .filter_map(|action| {
            let id = ActionId::new(action.id.as_str()).ok()?;
            Some((
                id,
                action
                    .parameters
                    .iter()
                    .filter_map(|(name, parameter)| {
                        checked_type(&parameter.type_id, vocabulary).map(|ty| {
                            (
                                name.clone(),
                                ActionParamSpec {
                                    ty,
                                    required: parameter.required,
                                },
                            )
                        })
                    })
                    .collect(),
            ))
        })
        .collect()
}

fn source_semantics(package: &SourcePackage) -> Result<DecisionSemantics, String> {
    let semantics = &package.semantics;
    let missing = match semantics.missing_facts.kind.as_str() {
        "preserve_unknown" => MissingFactStrategy::PreserveUnknown,
        "closed_world_false" => MissingFactStrategy::ClosedWorldFalse,
        "request_information" => MissingFactStrategy::RequestInformation,
        "escalate" => MissingFactStrategy::Escalate {
            destination: EscalationId::new(
                semantics
                    .missing_facts
                    .destination
                    .clone()
                    .ok_or("missing destination")?,
            )
            .map_err(|error| error.to_string())?,
        },
        value => return Err(format!("unknown missing-facts strategy `{value}`")),
    };
    let invalid = match semantics.invalid_facts.kind.as_str() {
        "reject_evaluation" => InvalidFactStrategy::RejectEvaluation,
        "preserve_invalid" => InvalidFactStrategy::PreserveInvalid,
        "escalate" => InvalidFactStrategy::Escalate {
            destination: EscalationId::new(
                semantics
                    .invalid_facts
                    .destination
                    .clone()
                    .ok_or("missing destination")?,
            )
            .map_err(|error| error.to_string())?,
        },
        value => return Err(format!("unknown invalid-facts strategy `{value}`")),
    };
    let precedence = match semantics.precedence.kind.as_str() {
        "safety_first" => DecisionPrecedence::SafetyFirst,
        "priority_first" => DecisionPrecedence::PriorityFirst,
        "explicit" => DecisionPrecedence::Explicit {
            primary: match semantics.precedence.primary.as_deref() {
                Some("outcome") => PrecedenceDimension::Outcome,
                Some("priority") => PrecedenceDimension::Priority,
                _ => {
                    return Err(
                        "explicit precedence primary must be outcome or priority".to_owned()
                    );
                }
            },
            outcome_ranks: semantics
                .precedence
                .outcome_ranks
                .iter()
                .map(|(kind, rank)| Ok((outcome_kind(kind)?, *rank)))
                .collect::<Result<_, String>>()?,
        },
        value => return Err(format!("unknown precedence `{value}`")),
    };
    DecisionSemantics::new(
        missing,
        invalid,
        precedence,
        PolicyTimeZone::new(semantics.timezone.clone()).map_err(|error| error.to_string())?,
        match semantics.expiry.as_str() {
            "inclusive" => ExpiryPolicy::Inclusive,
            "exclusive" => ExpiryPolicy::Exclusive,
            value => return Err(format!("unknown expiry policy `{value}`")),
        },
    )
    .map_err(|error| error.to_string())
}

fn source_condition(
    source: &SourceCondition,
    vocabulary: &ResolvedVocabulary,
    diagnostics: &mut Vec<CompilerDiagnostic>,
    provenance: &ProvenanceEvidence,
) -> Option<Expr> {
    match source {
        SourceCondition::All { conditions, span } => Some(Expr::All {
            expressions: conditions
                .iter()
                .filter_map(|condition| {
                    source_condition(condition, vocabulary, diagnostics, provenance)
                })
                .collect(),
            span: *span,
        }),
        SourceCondition::Any { conditions, span } => Some(Expr::Any {
            expressions: conditions
                .iter()
                .filter_map(|condition| {
                    source_condition(condition, vocabulary, diagnostics, provenance)
                })
                .collect(),
            span: *span,
        }),
        SourceCondition::Not { condition, span } => Some(Expr::Not {
            expression: Box::new(source_condition(
                condition,
                vocabulary,
                diagnostics,
                provenance,
            )?),
            span: *span,
        }),
        SourceCondition::Predicate(predicate) => {
            let Some(left_type) = fact_type(&predicate.fact, vocabulary) else {
                diagnostics.push(source_diagnostic(
                    DiagnosticCode::INVALID_FACT_PATH,
                    format!("fact path `{}` is not declared", predicate.fact),
                    label(predicate.span),
                    provenance,
                ));
                return None;
            };
            let right = predicate
                .value
                .as_ref()
                .and_then(|value| {
                    operand(value, &left_type, vocabulary)
                        .map_err(|message| {
                            diagnostics.push(source_diagnostic(
                                DiagnosticCode::TYPE_MISMATCH,
                                message,
                                label(predicate.span),
                                provenance,
                            ));
                        })
                        .ok()
                })
                .or(match predicate.operator {
                    SourceOperator::IsExpired | SourceOperator::IsUnexpired => {
                        Some(ExprOperand::Reserved(ReservedOperand::Today))
                    }
                    _ => None,
                });
            if predicate.value.is_some() && right.is_none() {
                return None;
            }
            let operator = source_operator(&predicate.operator);
            let expression = Predicate {
                operator,
                left: ExprOperand::Fact(predicate.fact.clone()),
                right,
                span: predicate.span,
            };
            let left = OperandSpec::Fact {
                path: predicate.fact.clone(),
                ty: left_type,
            };
            let right_spec = expression
                .right
                .as_ref()
                .map(|right| operand_spec(right, vocabulary));
            if let Err(error) = typecheck_predicate(operator, &left, right_spec.as_ref()) {
                diagnostics.push(source_diagnostic(
                    DiagnosticCode::TYPE_MISMATCH,
                    error.message,
                    label(predicate.span),
                    provenance,
                ));
                return None;
            }
            Some(Expr::Predicate(expression))
        }
    }
}

impl PolicyCompiler {
    /// Compiles an assembled authored package into an executable compiled package.
    ///
    /// This is the production entrypoint: it resolves vocabulary and action declarations,
    /// validates references and types, normalizes conditions, and only lowers a package when
    /// every phase succeeds.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn compile_source(&self, input: &SourceCompilationInput) -> CompilationOutput {
        let package = &input.root.package;
        let provenance = ProvenanceEvidence {
            package_id: package.metadata.package_id.clone(),
            compiler_identity: "rulery-compiler/0.1.0".to_owned(),
            language_version: LanguageVersion::V1,
        };
        let mut diagnostics = Vec::new();
        let vocabulary = match source_vocabulary(package) {
            Ok(vocabulary) => vocabulary,
            Err(message) => {
                diagnostics.push(source_diagnostic(
                    DiagnosticCode::UNKNOWN_SYMBOL,
                    message,
                    "vocabulary.yaml",
                    &provenance,
                ));
                ResolvedVocabulary::default()
            }
        };
        let actions = source_actions(package, &mut diagnostics, &provenance);
        let action_specs = action_specs(package, &vocabulary);
        let mut decisions = Vec::new();
        let mut decision_ids = BTreeSet::new();
        for decision in &package.decisions {
            if !decision_ids.insert(decision.id.clone()) {
                diagnostics.push(source_diagnostic(
                    DiagnosticCode::DUPLICATE_DECLARATION,
                    format!("duplicate decision declaration `{}`", decision.id),
                    label(decision.span),
                    &provenance,
                ));
                continue;
            }
            let semantics = match source_semantics(package) {
                Ok(semantics) => semantics,
                Err(message) => {
                    diagnostics.push(source_diagnostic(
                        DiagnosticCode::UNKNOWN_SYMBOL,
                        message,
                        label(decision.span),
                        &provenance,
                    ));
                    continue;
                }
            };
            let Some(default) = source_outcome(
                &decision.default,
                &actions,
                &action_specs,
                &vocabulary,
                &mut diagnostics,
                &provenance,
            ) else {
                continue;
            };
            let mut rules = Vec::new();
            let mut rule_ids = BTreeSet::new();
            for rule in &decision.rules {
                let Ok(rule_id) = RuleId::new(rule.id.as_str()) else {
                    diagnostics.push(source_diagnostic(
                        DiagnosticCode::UNKNOWN_SYMBOL,
                        format!("rule id `{}` is invalid", rule.id),
                        label(rule.span),
                        &provenance,
                    ));
                    continue;
                };
                if !rule_ids.insert(rule_id.clone()) {
                    diagnostics.push(source_diagnostic(
                        DiagnosticCode::DUPLICATE_DECLARATION,
                        format!("duplicate rule declaration `{rule_id}`"),
                        label(rule.span),
                        &provenance,
                    ));
                    continue;
                }
                let Some(condition) =
                    source_condition(&rule.when, &vocabulary, &mut diagnostics, &provenance)
                else {
                    continue;
                };
                let condition = match normalize_condition(condition) {
                    Ok(normalized) => normalized,
                    Err(error) => {
                        diagnostics.push(source_diagnostic(
                            DiagnosticCode::UNKNOWN_SYMBOL,
                            error.to_string(),
                            label(rule.span),
                            &provenance,
                        ));
                        continue;
                    }
                };
                let Some(effect) = source_outcome(
                    &rule.effect,
                    &actions,
                    &action_specs,
                    &vocabulary,
                    &mut diagnostics,
                    &provenance,
                ) else {
                    continue;
                };
                rules.push(CompiledRule::new_with_override(
                    rule_id.clone(),
                    QualifiedRuleId::new(package.metadata.package_id.clone(), rule_id),
                    rule.title.clone(),
                    rule.priority,
                    condition.condition,
                    effect,
                    rule.rationale.clone(),
                    rule.span,
                    condition.specificity,
                    rule.explicit_override,
                ));
            }
            match CompiledDecision::new(
                decision.id.clone(),
                semantics,
                default,
                rules,
                decision.span,
            ) {
                Ok(value) => decisions.push(value),
                Err(error) => diagnostics.push(source_diagnostic(
                    DiagnosticCode::DUPLICATE_DECLARATION,
                    error.to_string(),
                    label(decision.span),
                    &provenance,
                )),
            }
        }
        let vocabulary_hash = serde_json::to_vec(&vocabulary).map_or_else(
            |_| ContentHash::digest(&[]),
            |bytes| ContentHash::digest(&bytes),
        );
        lower_package(LoweringInput {
            draft: CompiledPackageDraft {
                package_id: package.metadata.package_id.clone(),
                package_version: package.metadata.version.clone(),
                language_version: LanguageVersion::V1,
                compiler_identity: provenance.compiler_identity,
                decisions,
                actions,
                integrity: PackageIntegritySet::new(PackageCompilationInput::new(
                    input.source_bundle_hash,
                    vocabulary_hash,
                    input.lock_hash,
                )),
            },
            source_map: input.source_map.clone(),
            vocabulary,
            diagnostics,
        })
    }
}

#[allow(clippy::too_many_lines)]
fn source_outcome(
    source: &SourceOutcome,
    actions: &[CompiledAction],
    specs: &BTreeMap<ActionId, BTreeMap<StableId, ActionParamSpec>>,
    vocabulary: &ResolvedVocabulary,
    diagnostics: &mut Vec<CompilerDiagnostic>,
    provenance: &ProvenanceEvidence,
) -> Option<CompiledEffect> {
    let reasons = source
        .reasons
        .iter()
        .map(|reason| {
            Reason::new(
                ReasonCode::new(reason.code.as_str()).map_err(|error| error.to_string())?,
                reason.message.clone(),
            )
            .map_err(|error| error.to_string())
        })
        .collect::<Result<Vec<_>, _>>()
        .and_then(|values| Reasons::new(values).map_err(|error| error.to_string()));
    let Ok(reasons) = reasons else {
        diagnostics.push(source_diagnostic(
            DiagnosticCode::UNKNOWN_SYMBOL,
            "outcome reasons are invalid",
            label(source.span),
            provenance,
        ));
        return None;
    };
    let mut invocations = Vec::new();
    for invocation in &source.actions {
        let Ok(action_id) = ActionId::new(invocation.action.as_str()) else {
            diagnostics.push(source_diagnostic(
                DiagnosticCode::UNKNOWN_SYMBOL,
                format!("action `{}` is invalid", invocation.action),
                label(invocation.span),
                provenance,
            ));
            continue;
        };
        if !actions.iter().any(|action| action.id() == &action_id) {
            diagnostics.push(source_diagnostic(
                DiagnosticCode::UNKNOWN_SYMBOL,
                format!("action `{action_id}` is not declared"),
                label(invocation.span),
                provenance,
            ));
            continue;
        }
        let params = specs
            .get(&action_id)
            .expect("compiled action has a type specification");
        let mut call = ActionCallSpec {
            args: BTreeMap::new(),
        };
        let mut values = BTreeMap::new();
        for (name, value) in &invocation.arguments {
            let Some(parameter) = params.get(name) else {
                diagnostics.push(source_diagnostic(
                    DiagnosticCode::TYPE_MISMATCH,
                    format!("unknown action argument `{name}`"),
                    label(invocation.span),
                    provenance,
                ));
                continue;
            };
            let Ok(value) = operand(value, &parameter.ty, vocabulary) else {
                diagnostics.push(source_diagnostic(
                    DiagnosticCode::TYPE_MISMATCH,
                    format!("action argument `{name}` has wrong type"),
                    label(invocation.span),
                    provenance,
                ));
                continue;
            };
            call.args
                .insert(name.clone(), operand_spec(&value, vocabulary));
            if let ExprOperand::Literal(value) = value {
                values.insert(name.clone(), value);
            } else {
                diagnostics.push(source_diagnostic(
                    DiagnosticCode::TYPE_MISMATCH,
                    "action arguments must lower to literal values",
                    label(invocation.span),
                    provenance,
                ));
            }
        }
        if let Err(error) = typecheck_action(params, &call) {
            diagnostics.push(source_diagnostic(
                DiagnosticCode::TYPE_MISMATCH,
                error.message,
                label(invocation.span),
                provenance,
            ));
            continue;
        }
        let invocation = serde_json::from_value(serde_json::json!({
            "action": action_id,
            "arguments": values,
        }))
        .expect("validated action invocation serializes");
        invocations.push(invocation);
    }
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.source_label == label(source.span))
    {
        return None;
    }
    let outcome = match source.kind.as_str() {
        "approve" => OutcomeTemplate::approve(reasons, invocations),
        "deny" => OutcomeTemplate::deny(reasons, invocations),
        "escalate" => OutcomeTemplate::escalate(
            EscalationId::new(source.destination.clone()?).ok()?,
            reasons,
            invocations,
        ),
        "request_information" => OutcomeTemplate::request_information(
            RequiredFacts::new(source.required_facts.clone()).ok()?,
            reasons,
            invocations,
        ),
        _ => return None,
    };
    Some(CompiledEffect::new(outcome))
}

fn source_operator(operator: &SourceOperator) -> Operator {
    match operator {
        SourceOperator::Equal => Operator::Equals,
        SourceOperator::NotEqual | SourceOperator::NotContains => Operator::NotEquals,
        SourceOperator::LessThan => Operator::LessThan,
        SourceOperator::LessThanOrEqual => Operator::LessOrEqual,
        SourceOperator::GreaterThan => Operator::GreaterThan,
        SourceOperator::GreaterThanOrEqual => Operator::GreaterOrEqual,
        SourceOperator::Contains => Operator::Contains,
        SourceOperator::StartsWith => Operator::StartsWith,
        SourceOperator::EndsWith => Operator::EndsWith,
        SourceOperator::IsOneOf => Operator::IsOneOf,
        SourceOperator::IsAbsent => Operator::Missing,
        SourceOperator::IsPresent | SourceOperator::IsValid => Operator::Exists,
        SourceOperator::IsInvalid => Operator::IsFalse,
        SourceOperator::Before | SourceOperator::IsExpired => Operator::Before,
        SourceOperator::OnOrAfter | SourceOperator::IsUnexpired => Operator::OnOrAfter,
    }
}

fn operand(
    source: &SourceOperand,
    expected: &CheckedType,
    vocabulary: &ResolvedVocabulary,
) -> Result<ExprOperand, String> {
    match source {
        SourceOperand::Fact(path) => {
            let actual =
                fact_type(path, vocabulary).ok_or_else(|| format!("unknown fact `{path}`"))?;
            if &actual != expected {
                return Err(format!("fact `{path}` has incompatible type"));
            }
            Ok(ExprOperand::Fact(path.clone()))
        }
        SourceOperand::Reserved(value) => match value.as_str() {
            "today" => Ok(ExprOperand::Reserved(ReservedOperand::Today)),
            "now" => Ok(ExprOperand::Reserved(ReservedOperand::Now)),
            _ => Err(format!("unknown reserved operand `{value}`")),
        },
        SourceOperand::Literal(value) => Ok(ExprOperand::Literal(literal(value, expected)?)),
    }
}

/// Decodes one authored literal against the type the type checker assigned to it.
///
/// Only the shapes [`CheckedType`] can express are accepted. A record literal has no
/// representation here because `CheckedType` has no record variant; record-valued authored facts
/// are resolved by the scenario bridge against the vocabulary instead.
fn literal(value: &SourceValue, ty: &CheckedType) -> Result<Value, String> {
    let SourceValue::Scalar(value) = value else {
        return match (value, ty) {
            (SourceValue::Null, _) => Ok(Value::Null),
            (SourceValue::Sequence(items), CheckedType::List(inner)) => items
                .iter()
                .map(|item| literal(item, inner))
                .collect::<Result<Vec<_>, _>>()
                .map(Value::List),
            _ => Err(format!("literal shape is not valid for `{ty:?}`")),
        };
    };
    match ty {
        CheckedType::Boolean => value
            .parse()
            .map(Value::Boolean)
            .map_err(|_| format!("`{value}` is not boolean")),
        CheckedType::Integer => value
            .parse()
            .map(Value::Integer)
            .map_err(|_| format!("`{value}` is not integer")),
        CheckedType::Decimal => DecimalValue::parse(value)
            .map(Value::Decimal)
            .map_err(|error| error.to_string()),
        CheckedType::Text => Ok(Value::Text(value.to_owned())),
        CheckedType::Date => PolicyDate::parse(value)
            .map(Value::Date)
            .map_err(|error| error.to_string()),
        CheckedType::DateTime => UtcInstant::parse(value)
            .map(Value::DateTime)
            .map_err(|error| error.to_string()),
        CheckedType::Duration => DurationValue::parse(value)
            .map(Value::Duration)
            .map_err(|error| error.to_string()),
        CheckedType::Enum(type_id) => Ok(Value::Enum(rulery_contracts::EnumValue::new(
            TypeId::new(type_id.as_str()).map_err(|error| error.to_string())?,
            StableId::new(value).map_err(|error| error.to_string())?,
        ))),
        CheckedType::List(_) => Err(format!("`{value}` is not a list")),
    }
}

fn operand_spec(operand: &ExprOperand, vocabulary: &ResolvedVocabulary) -> OperandSpec {
    match operand {
        ExprOperand::Fact(path) => OperandSpec::Fact {
            path: path.clone(),
            ty: fact_type(path, vocabulary).expect("checked fact exists"),
        },
        ExprOperand::Literal(value) => OperandSpec::Literal {
            value: value.clone(),
            ty: value_type(value),
        },
        ExprOperand::Reserved(ReservedOperand::Today) => OperandSpec::ReservedToday,
        ExprOperand::Reserved(ReservedOperand::Now) => OperandSpec::ReservedNow,
    }
}

fn value_type(value: &Value) -> CheckedType {
    match value {
        Value::Boolean(_) => CheckedType::Boolean,
        Value::Integer(_) => CheckedType::Integer,
        Value::Text(_) | Value::Null | Value::Record(_) => CheckedType::Text,
        Value::Enum(value) => CheckedType::Enum(
            StableId::new(value.type_id().as_str()).expect("type identifiers are stable ids"),
        ),
        Value::Date(_) => CheckedType::Date,
        Value::DateTime(_) => CheckedType::DateTime,
        Value::Decimal(_) => CheckedType::Decimal,
        Value::Duration(_) => CheckedType::Duration,
        Value::List(values) => CheckedType::List(Box::new(
            values.first().map_or(CheckedType::Text, value_type),
        )),
    }
}

fn fact_type(path: &FactPath, vocabulary: &ResolvedVocabulary) -> Option<CheckedType> {
    let (root_path, root) = vocabulary
        .roots
        .iter()
        .find(|(root_path, _)| path.segments().starts_with(root_path.segments()))?;
    let mut type_id = root.type_id.clone();
    for segment in &path.segments()[root_path.len()..] {
        let declaration = &vocabulary.types.get(&type_id)?.declaration;
        let TypeDeclaration::Record { fields, .. } = declaration else {
            return None;
        };
        type_id = fields
            .iter()
            .find_map(|(name, field)| (name.as_str() == segment.as_str()).then_some(field))?
            .type_id
            .clone();
    }
    checked_type(type_id.as_str(), vocabulary)
}

fn checked_type(name: &str, vocabulary: &ResolvedVocabulary) -> Option<CheckedType> {
    Some(match name {
        "bool" | "boolean" => CheckedType::Boolean,
        "int" | "integer" => CheckedType::Integer,
        "decimal" => CheckedType::Decimal,
        "string" | "text" => CheckedType::Text,
        "date" => CheckedType::Date,
        "datetime" | "date-time" => CheckedType::DateTime,
        "duration" => CheckedType::Duration,
        _ => match &vocabulary.types.get(&TypeId::new(name).ok()?)?.declaration {
            TypeDeclaration::Enum { id, .. } => CheckedType::Enum(
                StableId::new(id.as_str()).expect("type IDs satisfy stable ID syntax"),
            ),
            TypeDeclaration::List { element, .. } => {
                CheckedType::List(Box::new(checked_type(element.as_str(), vocabulary)?))
            }
            TypeDeclaration::Alias { target, .. } => checked_type(target.as_str(), vocabulary)?,
            TypeDeclaration::Record { .. } => CheckedType::Text,
            TypeDeclaration::Primitive => return None,
        },
    })
}

fn outcome_kind(value: &str) -> Result<rulery_contracts::OutcomeKind, String> {
    match value {
        "approve" => Ok(rulery_contracts::OutcomeKind::Approve),
        "deny" => Ok(rulery_contracts::OutcomeKind::Deny),
        "escalate" => Ok(rulery_contracts::OutcomeKind::Escalate),
        "request_information" => Ok(rulery_contracts::OutcomeKind::RequestInformation),
        _ => Err(format!("unknown outcome `{value}`")),
    }
}

fn source_diagnostic(
    code: &'static str,
    message: impl Into<String>,
    source_label: impl Into<String>,
    provenance: &ProvenanceEvidence,
) -> CompilerDiagnostic {
    CompilerDiagnostic {
        code: DiagnosticCode::new(code).expect("compiler uses known diagnostic codes"),
        message: message.into(),
        source_label: source_label.into(),
        provenance: provenance.clone(),
    }
}

fn label(span: rulery_contracts::Span) -> String {
    format!(
        "source.{}:{}-{}",
        span.source().get(),
        span.start(),
        span.end()
    )
}

fn build_empty_package(input: &CompilationInput) -> Option<CompiledPackage> {
    let draft = CompiledPackageDraft {
        package_id: input.package_id.clone(),
        package_version: input.package_version.clone(),
        language_version: input.language_version,
        compiler_identity: input.compiler_identity.clone(),
        decisions: Vec::new(),
        actions: Vec::new(),
        integrity: PackageIntegritySet::new(PackageCompilationInput::new(
            ContentHash::from_bytes([0; 32]),
            ContentHash::from_bytes([0; 32]),
            None,
        )),
    };
    CompiledPackage::new(draft, input.source_map.clone(), input.vocabulary.clone()).ok()
}

impl ValidationIssue {
    fn into_diagnostic(self, input: &CompilationInput) -> CompilerDiagnostic {
        CompilerDiagnostic {
            code: self.code,
            message: self.message,
            source_label: self.source_label,
            provenance: ProvenanceEvidence {
                package_id: input.package_id.clone(),
                compiler_identity: input.compiler_identity.clone(),
                language_version: input.language_version,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, fs, str::FromStr, sync::Arc};

    use rulery_contracts::{
        ActionId, ContentHash, DecisionId, FactPath, PackageId, RuleId, SourceBundle,
        SourceDocument, SourceFile, SourceId, SourceKey, SourceMap, SourcePath, StableId, TypeId,
    };
    use rulery_syntax::{SourceParser, YamlSourceParser};

    use super::*;

    #[test]
    fn compiler_rejects_duplicate_and_unknown_symbols() {
        let input = CompilationInput {
            package_id: PackageId::new("pkg.main").expect("package"),
            package_version: Version::new("1.0.0").expect("version"),
            language_version: LanguageVersion::V1,
            compiler_identity: "rulery.compiler".to_owned(),
            source_map: sample_source_map(),
            vocabulary: ResolvedVocabulary::default(),
            decisions: vec![
                DecisionId::new("decision.authz").expect("decision"),
                DecisionId::new("decision.authz").expect("decision duplicate"),
            ],
            rules: vec![
                RuleBinding {
                    source_label: "rulery.yaml:decision.authz.rule.allow".to_owned(),
                    decision: DecisionId::new("decision.authz").expect("decision"),
                    rule: RuleId::new("rule.allow").expect("rule"),
                    fact_paths: vec![FactPath::from_str("account.status").expect("fact path")],
                    action_calls: vec![ActionId::new("action.notify").expect("action")],
                    terms: vec![StableId::new("term.unknown").expect("term")],
                    override_target: Some("pkg.unknown::rule.base".to_owned()),
                    override_rationale: None,
                },
                RuleBinding {
                    source_label: "rules/secondary.yaml:decision.ghost.rule.shadow".to_owned(),
                    decision: DecisionId::new("decision.ghost").expect("ghost decision"),
                    rule: RuleId::new("rule.shadow").expect("rule"),
                    fact_paths: vec![FactPath::from_str("ghost.fact").expect("fact path")],
                    action_calls: vec![ActionId::new("action.missing").expect("action")],
                    terms: vec![StableId::new("term.temporal.deadline").expect("term")],
                    override_target: None,
                    override_rationale: None,
                },
            ],
            actions: vec![
                ActionId::new("action.notify").expect("action"),
                ActionId::new("action.notify").expect("action duplicate"),
            ],
            types: vec![
                TypeId::new("type.account").expect("type"),
                TypeId::new("type.account").expect("type duplicate"),
            ],
            available_fact_paths: vec![FactPath::from_str("account.status").expect("fact path")],
            available_terms: Vec::new(),
            imported_packages: vec![PackageId::new("pkg.shared").expect("imported package")],
        };

        let output = PolicyCompiler.compile(&input);
        let codes = output
            .diagnostics
            .iter()
            .map(|entry| entry.code.as_str().to_owned())
            .collect::<Vec<_>>();

        assert!(
            codes
                .iter()
                .any(|code| code == DiagnosticCode::DUPLICATE_DECLARATION)
        );
        assert!(
            codes
                .iter()
                .any(|code| code == DiagnosticCode::UNKNOWN_SYMBOL)
        );
        assert!(
            codes
                .iter()
                .any(|code| code == DiagnosticCode::INVALID_FACT_PATH)
        );
        assert!(
            codes
                .iter()
                .any(|code| code == DiagnosticCode::UNDEFINED_OPERATIONAL_TERM)
        );
        assert!(
            codes
                .iter()
                .any(|code| code == DiagnosticCode::TEMPORAL_TERM_UNDEFINED)
        );

        for diagnostic in &output.diagnostics {
            assert!(!diagnostic.source_label.is_empty());
            assert_eq!(diagnostic.provenance.package_id, input.package_id);
            assert_eq!(diagnostic.provenance.language_version, LanguageVersion::V1);
            assert_eq!(diagnostic.provenance.compiler_identity, "rulery.compiler");
        }

        let mut sorted = output.diagnostics.clone();
        sorted.sort_by(|left, right| {
            left.code
                .as_str()
                .cmp(right.code.as_str())
                .then_with(|| left.source_label.cmp(&right.source_label))
                .then_with(|| left.message.cmp(&right.message))
        });
        assert_eq!(output.diagnostics, sorted);
        assert!(output.package.is_none());
    }

    #[test]
    fn compiler_lowers_assembled_tool_library_to_executable_package() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/tool-library");
        let paths = [
            "rulery.yaml",
            "vocabulary.yaml",
            "actions.yaml",
            "rules/checkout.yaml",
            "scenarios/expired-training-is-denied.yaml",
        ];
        let bundle = SourceBundle::new(
            paths
                .iter()
                .map(|path| {
                    SourceDocument::new(
                        SourcePath::new(*path).expect("source path"),
                        Arc::<str>::from(
                            fs::read_to_string(format!("{root}/{path}")).expect("source"),
                        ),
                    )
                })
                .collect(),
        )
        .expect("complete source bundle");
        let root = YamlSourceParser
            .parse_bundle(&bundle)
            .expect("parsed assembled package");
        let output = PolicyCompiler.compile_source(&SourceCompilationInput {
            source_map: root.source_map.clone(),
            root,
            imports: BTreeMap::new(),
            source_bundle_hash: ContentHash::digest(
                &bundle
                    .documents()
                    .iter()
                    .flat_map(|document| document.content().bytes())
                    .collect::<Vec<_>>(),
            ),
            lock_hash: None,
        });
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let package = output.package.expect("compiled package");
        assert_eq!(package.payload().decisions().len(), 1);
        assert_eq!(
            package
                .payload()
                .decisions()
                .values()
                .next()
                .expect("decision")
                .rules()
                .len(),
            4
        );
        assert_eq!(package.payload().actions().len(), 2);
        assert!(!package.vocabulary().roots.is_empty());
        assert_eq!(package.source_map().iter().count(), paths.len());
    }

    fn sample_source_map() -> SourceMap {
        let mut map = SourceMap::new();
        map.insert(
            SourceKey::new(1),
            SourceFile::new(
                SourceId::new("src.main").expect("source id"),
                SourcePath::new("rulery.yaml").expect("path"),
                Arc::<str>::from("content"),
            ),
        )
        .expect("insert source");
        map
    }
}
