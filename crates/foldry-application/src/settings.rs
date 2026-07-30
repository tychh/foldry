use std::path::PathBuf;

use foldry_core::{
    ArchiveFormat, ChecksumAlgorithm, CompressionLevel, ConflictPolicy, Extensions, ProfileId,
    UnreadablePolicy, VerificationMode,
};
use serde::{Deserialize, Serialize};

use crate::{ContractValidation, ValidationCode, ValidationIssue};

/// Version of `settings.yaml`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct SettingsVersion(pub u16);

impl SettingsVersion {
    pub const V1: Self = Self(1);
    pub const CURRENT: Self = Self::V1;
}

impl Default for SettingsVersion {
    fn default() -> Self {
        Self::CURRENT
    }
}

/// GUI language. English remains the fallback.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Locale {
    En,
    Ru,
}

/// Requested desktop color scheme.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Appearance {
    System,
    Light,
    Dark,
}

/// Preferred presentation of folder browser contents.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserView {
    #[default]
    Tree,
    List,
}

/// Defaults copied into a newly created archive action.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ArchiveDefaults {
    pub output_directory: PathBuf,
    pub format: ArchiveFormat,
    pub compression: CompressionLevel,
    pub conflict_policy: ConflictPolicy,
    pub include_root: bool,
    pub unreadable_policy: UnreadablePolicy,
    pub verification_mode: VerificationMode,
    pub checksum: ChecksumAlgorithm,
    #[serde(default, skip_serializing_if = "Extensions::is_empty", flatten)]
    pub extensions: Extensions,
}

/// Scheduler settings that affect new executions.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ExecutionSettings {
    pub max_parallel_runs: u16,
    #[serde(default, skip_serializing_if = "Extensions::is_empty", flatten)]
    pub extensions: Extensions,
}

/// One age/count retention boundary.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RetentionPolicy {
    pub unlimited: bool,
    pub max_age_days: u32,
    pub max_entries: u32,
    #[serde(default, skip_serializing_if = "Extensions::is_empty", flatten)]
    pub extensions: Extensions,
}

/// Separate metadata and detailed-log retention.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct HistorySettings {
    pub runs: RetentionPolicy,
    pub logs: RetentionPolicy,
    #[serde(default, skip_serializing_if = "Extensions::is_empty", flatten)]
    pub extensions: Extensions,
}

/// User-managed shortcuts and bounded navigation history for the folder browser.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct BrowserSettings {
    #[serde(default)]
    pub favorites: Vec<PathBuf>,
    #[serde(default)]
    pub recent: Vec<PathBuf>,
    #[serde(default)]
    pub view: BrowserView,
    #[serde(default, skip_serializing_if = "Extensions::is_empty", flatten)]
    pub extensions: Extensions,
}

/// Versioned application settings document.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Settings {
    pub version: SettingsVersion,
    pub locale: Locale,
    pub appearance: Appearance,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_profile_id: Option<ProfileId>,
    pub archive_defaults: ArchiveDefaults,
    pub execution: ExecutionSettings,
    pub history: HistorySettings,
    #[serde(default)]
    pub browser: BrowserSettings,
    #[serde(default, skip_serializing_if = "Extensions::is_empty", flatten)]
    pub extensions: Extensions,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SettingsVersion::CURRENT,
            locale: Locale::En,
            appearance: Appearance::System,
            default_profile_id: None,
            archive_defaults: ArchiveDefaults {
                output_directory: PathBuf::from("."),
                format: ArchiveFormat::Zip,
                compression: CompressionLevel::Balanced,
                conflict_policy: ConflictPolicy::Increment,
                include_root: true,
                unreadable_policy: UnreadablePolicy::Fail,
                verification_mode: VerificationMode::Structural,
                checksum: ChecksumAlgorithm::None,
                extensions: Extensions::new(),
            },
            execution: ExecutionSettings {
                max_parallel_runs: 2,
                extensions: Extensions::new(),
            },
            history: HistorySettings {
                runs: RetentionPolicy {
                    unlimited: false,
                    max_age_days: 365,
                    max_entries: 10_000,
                    extensions: Extensions::new(),
                },
                logs: RetentionPolicy {
                    unlimited: false,
                    max_age_days: 90,
                    max_entries: 1_000,
                    extensions: Extensions::new(),
                },
                extensions: Extensions::new(),
            },
            browser: BrowserSettings::default(),
            extensions: Extensions::new(),
        }
    }
}

impl ContractValidation for Settings {
    fn validate(&self) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        if self.version != SettingsVersion::CURRENT {
            issues.push(ValidationIssue {
                code: ValidationCode::UnsupportedDocumentVersion,
                path: "$.version".into(),
                message: format!(
                    "settings version {} is not supported; current version is {}",
                    self.version.0,
                    SettingsVersion::CURRENT.0
                ),
            });
        }
        if self
            .archive_defaults
            .output_directory
            .as_os_str()
            .is_empty()
        {
            issues.push(ValidationIssue {
                code: ValidationCode::EmptyOutputDirectory,
                path: "$.archive_defaults.output_directory".into(),
                message: "default archive output directory must not be empty".into(),
            });
        }
        if !(1..=64).contains(&self.execution.max_parallel_runs) {
            issues.push(ValidationIssue {
                code: ValidationCode::InvalidParallelRuns,
                path: "$.execution.max_parallel_runs".into(),
                message: "max_parallel_runs must be between 1 and 64".into(),
            });
        }
        validate_retention("$.history.runs", &self.history.runs, &mut issues);
        validate_retention("$.history.logs", &self.history.logs, &mut issues);
        issues
    }
}

fn validate_retention(path: &str, policy: &RetentionPolicy, issues: &mut Vec<ValidationIssue>) {
    if !policy.unlimited && (policy.max_age_days == 0 || policy.max_entries == 0) {
        issues.push(ValidationIssue {
            code: ValidationCode::InvalidRetention,
            path: path.into(),
            message: "finite retention requires non-zero age and entry limits".into(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_accepted_retention_contract() {
        let settings = Settings::default();

        assert_eq!(settings.history.runs.max_age_days, 365);
        assert_eq!(settings.history.runs.max_entries, 10_000);
        assert_eq!(settings.history.logs.max_age_days, 90);
        assert_eq!(settings.history.logs.max_entries, 1_000);
        assert!(settings.validate().is_empty());
    }
}
