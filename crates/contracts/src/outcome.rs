//! Declarative actions, reasons, and decision outcomes.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{ActionId, EscalationId, FactPath, ReasonCode, StableId, TypeId, Value};

/// Declared action available to outcomes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Action {
    id: ActionId,
    display_name: String,
    description: Option<String>,
    parameters: BTreeMap<StableId, ActionParameter>,
}

/// Declared action parameter.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ActionParameter {
    type_id: TypeId,
    required: bool,
    description: Option<String>,
}

/// Declarative action obligation attached to an outcome.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ActionInvocation {
    action: ActionId,
    arguments: BTreeMap<StableId, Value>,
}

/// Human-readable reason with a stable machine code.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Reason {
    code: ReasonCode,
    message: String,
    detail: Option<String>,
}

impl Reason {
    /// Creates a reason with a non-empty human message.
    ///
    /// # Errors
    ///
    /// Returns [`OutcomeError`] when the message is empty.
    pub fn new(code: ReasonCode, message: impl Into<String>) -> Result<Self, OutcomeError> {
        let message = message.into();
        if message.is_empty() {
            Err(OutcomeError::EmptyReasonMessage)
        } else {
            Ok(Self {
                code,
                message,
                detail: None,
            })
        }
    }
}

/// Non-empty ordered decision reasons.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Reasons(Vec<Reason>);

impl Reasons {
    /// Creates a non-empty reason collection.
    ///
    /// # Errors
    ///
    /// Returns [`OutcomeError`] when no reasons are supplied.
    pub fn new(values: Vec<Reason>) -> Result<Self, OutcomeError> {
        if values.is_empty() {
            Err(OutcomeError::EmptyReasons)
        } else {
            Ok(Self(values))
        }
    }
}

/// Non-empty set of facts requested from a caller.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RequiredFacts(BTreeSet<FactPath>);

impl RequiredFacts {
    /// Creates a non-empty requested-fact set.
    ///
    /// # Errors
    ///
    /// Returns [`OutcomeError`] when no paths are supplied.
    pub fn new(values: BTreeSet<FactPath>) -> Result<Self, OutcomeError> {
        if values.is_empty() {
            Err(OutcomeError::EmptyRequiredFacts)
        } else {
            Ok(Self(values))
        }
    }
}

/// Coarse decision outcome kind.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeKind {
    /// Policy permits the action.
    Approve,
    /// Policy denies the action.
    Deny,
    /// A named authority must decide.
    Escalate,
    /// Additional facts are required.
    RequestInformation,
}

/// Compiled outcome emitted when a rule or default wins.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OutcomeTemplate {
    /// Approval outcome.
    Approve {
        /// Non-empty reasons.
        reasons: Reasons,
        /// Declarative actions.
        actions: Vec<ActionInvocation>,
    },
    /// Denial outcome.
    Deny {
        /// Non-empty reasons.
        reasons: Reasons,
        /// Declarative actions.
        actions: Vec<ActionInvocation>,
    },
    /// Escalation outcome.
    Escalate {
        /// Escalation destination.
        destination: EscalationId,
        /// Non-empty reasons.
        reasons: Reasons,
        /// Declarative actions.
        actions: Vec<ActionInvocation>,
    },
    /// Information request outcome.
    RequestInformation {
        /// Non-empty requested facts.
        required_facts: RequiredFacts,
        /// Non-empty reasons.
        reasons: Reasons,
        /// Declarative actions.
        actions: Vec<ActionInvocation>,
    },
}

impl OutcomeTemplate {
    /// Creates an approval outcome.
    #[must_use]
    pub fn approve(reasons: Reasons, actions: Vec<ActionInvocation>) -> Self {
        Self::Approve { reasons, actions }
    }

    /// Creates a denial outcome.
    #[must_use]
    pub fn deny(reasons: Reasons, actions: Vec<ActionInvocation>) -> Self {
        Self::Deny { reasons, actions }
    }

    /// Creates an escalation outcome.
    #[must_use]
    pub fn escalate(
        destination: EscalationId,
        reasons: Reasons,
        actions: Vec<ActionInvocation>,
    ) -> Self {
        Self::Escalate {
            destination,
            reasons,
            actions,
        }
    }

    /// Creates an information request outcome.
    #[must_use]
    pub fn request_information(
        required_facts: RequiredFacts,
        reasons: Reasons,
        actions: Vec<ActionInvocation>,
    ) -> Self {
        Self::RequestInformation {
            required_facts,
            reasons,
            actions,
        }
    }

    /// Returns this outcome's coarse kind.
    #[must_use]
    pub const fn kind(&self) -> OutcomeKind {
        match self {
            Self::Approve { .. } => OutcomeKind::Approve,
            Self::Deny { .. } => OutcomeKind::Deny,
            Self::Escalate { .. } => OutcomeKind::Escalate,
            Self::RequestInformation { .. } => OutcomeKind::RequestInformation,
        }
    }
}

/// Evaluated decision outcome.
pub type Outcome = OutcomeTemplate;

/// Error returned when constructing outcome invariants.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum OutcomeError {
    /// No reasons were supplied.
    #[error("an outcome requires at least one reason")]
    EmptyReasons,
    /// A reason message was empty.
    #[error("a reason message must be non-empty")]
    EmptyReasonMessage,
    /// No required facts were supplied.
    #[error("an information request requires at least one fact")]
    EmptyRequiredFacts,
}
