use std::fmt;

use foldry_application::{
    ContractValidation, Plan, PlanVersion, Settings, SettingsVersion, ValidationIssue,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use thiserror::Error;

use crate::{DocumentKind, MigrationRegistry};

/// Safe failure modes for reading or writing a public YAML contract.
#[derive(Debug, Error)]
pub enum DocumentError {
    #[error("{kind} YAML syntax error at line {line}, column {column}: {message}")]
    Syntax {
        kind: &'static str,
        line: usize,
        column: usize,
        message: String,
    },
    #[error("{kind} document requires an integer field `version` at $.version")]
    MissingVersion { kind: &'static str },
    #[error("{kind} version {found} is not supported by this build; current version is {current}")]
    UnsupportedVersion {
        kind: &'static str,
        found: u64,
        current: u16,
    },
    #[error("{kind} migration failed: {message}")]
    Migration { kind: &'static str, message: String },
    #[error("{kind} field {path} is invalid: {message}")]
    Decode {
        kind: &'static str,
        path: String,
        message: String,
    },
    #[error("{kind} document failed structural validation")]
    Validation {
        kind: &'static str,
        issues: Vec<ValidationIssue>,
    },
    #[error("cannot serialize {kind} document: {message}")]
    Encode { kind: &'static str, message: String },
}

impl DocumentError {
    /// Returns all validation issues when the syntax and typed shape were valid.
    #[must_use]
    pub fn validation_issues(&self) -> Option<&[ValidationIssue]> {
        match self {
            Self::Validation { issues, .. } => Some(issues),
            _ => None,
        }
    }
}

/// Decodes and validates plan schema v1.
pub fn decode_plan(source: &str) -> Result<Plan, DocumentError> {
    decode(
        source,
        DocumentKind::Plan,
        PlanVersion::CURRENT.0,
        &MigrationRegistry::new(DocumentKind::Plan, PlanVersion::CURRENT.0, Vec::new()),
    )
}

/// Produces canonical UTF-8 YAML with LF and a final newline.
pub fn encode_plan(plan: &Plan) -> Result<String, DocumentError> {
    encode(plan, DocumentKind::Plan)
}

/// Decodes and validates settings schema v1.
pub fn decode_settings(source: &str) -> Result<Settings, DocumentError> {
    decode(
        source,
        DocumentKind::Settings,
        SettingsVersion::CURRENT.0,
        &MigrationRegistry::new(
            DocumentKind::Settings,
            SettingsVersion::CURRENT.0,
            Vec::new(),
        ),
    )
}

/// Produces canonical UTF-8 YAML with LF and a final newline.
pub fn encode_settings(settings: &Settings) -> Result<String, DocumentError> {
    encode(settings, DocumentKind::Settings)
}

fn decode<T>(
    source: &str,
    kind: DocumentKind,
    current: u16,
    migrations: &MigrationRegistry,
) -> Result<T, DocumentError>
where
    T: DeserializeOwned + ContractValidation,
{
    let kind_name = kind.name();
    let generic = parse_generic(source, kind)?;
    let version = generic
        .get("version")
        .and_then(Value::as_u64)
        .ok_or(DocumentError::MissingVersion { kind: kind_name })?;

    if version > u64::from(current) {
        return Err(DocumentError::UnsupportedVersion {
            kind: kind_name,
            found: version,
            current,
        });
    }

    let prepared = if version == u64::from(current) {
        source.to_owned()
    } else {
        let version = u16::try_from(version).map_err(|_| DocumentError::UnsupportedVersion {
            kind: kind_name,
            found: version,
            current,
        })?;
        let migrated =
            migrations
                .migrate(version, generic)
                .map_err(|message| DocumentError::Migration {
                    kind: kind_name,
                    message,
                })?;
        serde_yaml_ng::to_string(&migrated).map_err(|error| DocumentError::Migration {
            kind: kind_name,
            message: error.to_string(),
        })?
    };

    let deserializer = serde_yaml_ng::Deserializer::from_str(&prepared);
    let document: T =
        serde_path_to_error::deserialize(deserializer).map_err(|error| DocumentError::Decode {
            kind: kind_name,
            path: display_path(error.path()),
            message: error.inner().to_string(),
        })?;

    let issues = document.validate();
    if issues.is_empty() {
        Ok(document)
    } else {
        Err(DocumentError::Validation {
            kind: kind_name,
            issues,
        })
    }
}

fn encode<T>(document: &T, kind: DocumentKind) -> Result<String, DocumentError>
where
    T: Serialize + ContractValidation,
{
    let kind_name = kind.name();
    let issues = document.validate();
    if !issues.is_empty() {
        return Err(DocumentError::Validation {
            kind: kind_name,
            issues,
        });
    }

    let mut encoded =
        serde_yaml_ng::to_string(document).map_err(|error| DocumentError::Encode {
            kind: kind_name,
            message: error.to_string(),
        })?;
    encoded = encoded.replace("\r\n", "\n");
    if !encoded.ends_with('\n') {
        encoded.push('\n');
    }
    Ok(encoded)
}

fn parse_generic(source: &str, kind: DocumentKind) -> Result<Value, DocumentError> {
    serde_yaml_ng::from_str(source).map_err(|error| {
        let location = error.location();
        DocumentError::Syntax {
            kind: kind.name(),
            line: location.as_ref().map_or(1, serde_yaml_ng::Location::line),
            column: location.as_ref().map_or(1, serde_yaml_ng::Location::column),
            message: error.to_string(),
        }
    })
}

fn display_path(path: &serde_path_to_error::Path) -> String {
    if path.to_string() == "." {
        "$".into()
    } else {
        format!("$.{}", PathDisplay(path))
    }
}

struct PathDisplay<'a>(&'a serde_path_to_error::Path);

impl fmt::Display for PathDisplay<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path = self.0.to_string();
        formatter.write_str(path.trim_start_matches('.'))
    }
}
