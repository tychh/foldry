use serde::{Deserialize, Serialize};

/// Stable code for a structural contract validation failure.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationCode {
    UnsupportedDocumentVersion,
    EmptyName,
    EmptySource,
    DuplicateFolderId,
    DuplicateActionId,
    DuplicateSource,
    EmptyOutputDirectory,
    InvalidOutputFilename,
    OutputInsideSource,
    ReservedExtensionField,
    InvalidParallelRuns,
    InvalidRetention,
}

/// Validation issue with a JSONPath-like location.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ValidationIssue {
    pub code: ValidationCode,
    pub path: String,
    pub message: String,
}

/// Stable reason why a structurally valid action cannot be executed by this build.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionBlockerCode {
    UnsupportedActionType,
    UnsupportedActionVersion,
}

/// Non-destructive compatibility problem reported after loading a document.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExecutionBlocker {
    pub code: ExecutionBlockerCode,
    pub path: String,
    pub message: String,
}

/// Shared validation behavior for versioned application contracts.
pub trait ContractValidation {
    /// Returns every structural issue instead of failing at the first field.
    fn validate(&self) -> Vec<ValidationIssue>;
}
