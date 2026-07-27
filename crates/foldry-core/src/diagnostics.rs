use serde::{Deserialize, Serialize};

use crate::Extensions;

/// Stable machine-readable parser diagnostic code.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticCode {
    InvalidMetadata,
    DuplicateMetadata,
    InvalidRule,
    InvalidEscape,
    UnterminatedCharacterClass,
    DuplicatePresetBlock,
    UnterminatedPresetBlock,
}

/// User-facing importance of a parser diagnostic.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
}

/// One-based source location in a UTF-8 text document.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceLocation {
    pub line: u32,
    pub column: u32,
}

/// Half-open source range used by profile editor diagnostics.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceSpan {
    pub start: SourceLocation,
    pub end: SourceLocation,
}

/// Parser feedback suitable for CLI, GUI, and editor annotations.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ParserDiagnostic {
    pub code: DiagnosticCode,
    pub severity: DiagnosticSeverity,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<SourceSpan>,
    #[serde(default, skip_serializing_if = "Extensions::is_empty", flatten)]
    pub extensions: Extensions,
}
