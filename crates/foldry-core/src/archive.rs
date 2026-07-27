use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::Extensions;

/// Version of an individual action specification.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ActionVersion(pub u16);

impl ActionVersion {
    pub const V1: Self = Self(1);
}

impl Default for ActionVersion {
    fn default() -> Self {
        Self::V1
    }
}

/// Archive container and compression combination.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveFormat {
    Zip,
    TarGz,
    TarZst,
}

/// Stable semantic compression choice stored in plans and settings.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompressionLevel {
    Fast,
    Balanced,
    Maximum,
}

/// Behavior when the resolved output path already exists.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictPolicy {
    Skip,
    Overwrite,
    Increment,
}

/// Behavior when a source entry cannot be read consistently.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnreadablePolicy {
    Fail,
    WarnAndSkip,
}

/// Amount of verification performed before publishing an archive.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationMode {
    Structural,
    Full,
}

/// Optional checksum calculated for the finished archive.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChecksumAlgorithm {
    None,
    Sha256,
}

/// Output naming and archive codec settings.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ArchiveOutputSpec {
    pub directory: PathBuf,
    pub filename: String,
    pub format: ArchiveFormat,
    pub compression: CompressionLevel,
    pub conflict_policy: ConflictPolicy,
    #[serde(default, skip_serializing_if = "Extensions::is_empty", flatten)]
    pub extensions: Extensions,
}

/// Verification settings applied after the archive writer closes.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VerificationSpec {
    pub mode: VerificationMode,
    pub checksum: ChecksumAlgorithm,
    #[serde(default, skip_serializing_if = "Extensions::is_empty", flatten)]
    pub extensions: Extensions,
}

/// Versioned specification of the v1 `archive` action.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ArchiveActionSpec {
    #[serde(default)]
    pub version: ActionVersion,
    pub output: ArchiveOutputSpec,
    #[serde(default = "default_include_root")]
    pub include_root: bool,
    pub unreadable_policy: UnreadablePolicy,
    pub verification: VerificationSpec,
    #[serde(default, skip_serializing_if = "Extensions::is_empty", flatten)]
    pub extensions: Extensions,
}

const fn default_include_root() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::ArchiveActionSpec;

    #[test]
    fn include_root_defaults_to_true_when_omitted() {
        let json = r#"{
          "version": 1,
          "output": {
            "directory": ".",
            "filename": "backup",
            "format": "zip",
            "compression": "balanced",
            "conflict_policy": "increment"
          },
          "unreadable_policy": "fail",
          "verification": { "mode": "structural", "checksum": "none" }
        }"#;
        let action: ArchiveActionSpec = serde_json::from_str(json).expect("action");
        assert!(action.include_root);
    }
}
