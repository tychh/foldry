use std::{
    collections::{BTreeMap, HashSet},
    path::PathBuf,
};

use foldry_core::{ActionVersion, ArchiveActionSpec, Extensions, ProfileId, TaskId};
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::Error as DeError,
    ser::{Error as SerError, SerializeMap},
};
use serde_json::Value;

use crate::{
    ContractValidation, ExecutionBlocker, ExecutionBlockerCode, ValidationCode, ValidationIssue,
};

/// Version of the active plan document.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct PlanVersion(pub u16);

impl PlanVersion {
    pub const V1: Self = Self(1);
    pub const CURRENT: Self = Self::V1;
}

impl Default for PlanVersion {
    fn default() -> Self {
        Self::CURRENT
    }
}

/// One active `.packplan.yaml` document.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Plan {
    pub version: PlanVersion,
    pub name: String,
    pub tasks: Vec<Task>,
    #[serde(default, skip_serializing_if = "Extensions::is_empty", flatten)]
    pub extensions: Extensions,
}

/// One configured source and its ordered action scenario.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Task {
    pub id: TaskId,
    pub source: PathBuf,
    pub enabled: bool,
    pub profile_id: ProfileId,
    pub steps: Vec<ActionSpec>,
    #[serde(default, skip_serializing_if = "Extensions::is_empty", flatten)]
    pub extensions: Extensions,
}

/// Known or safely preserved future action specification.
#[derive(Clone, Debug, PartialEq)]
pub enum ActionSpec {
    Archive(ArchiveActionSpec),
    Unsupported(UnsupportedActionSpec),
}

impl ActionSpec {
    /// Returns the stable wire discriminator.
    #[must_use]
    pub fn action_type(&self) -> &str {
        match self {
            Self::Archive(_) => "archive",
            Self::Unsupported(spec) => &spec.action_type,
        }
    }
}

/// Unknown action payload retained for round-trip safety.
#[derive(Clone, Debug, PartialEq)]
pub struct UnsupportedActionSpec {
    pub action_type: String,
    pub version: Option<u16>,
    pub fields: BTreeMap<String, Value>,
}

impl Serialize for ActionSpec {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Archive(spec) => {
                let Value::Object(fields) = serde_json::to_value(spec).map_err(S::Error::custom)?
                else {
                    return Err(S::Error::custom("archive action must serialize as a map"));
                };
                let mut map = serializer.serialize_map(Some(fields.len() + 1))?;
                map.serialize_entry("type", "archive")?;
                for (key, value) in fields {
                    map.serialize_entry(&key, &value)?;
                }
                map.end()
            }
            Self::Unsupported(spec) => {
                let mut map = serializer.serialize_map(Some(
                    spec.fields.len() + 1 + usize::from(spec.version.is_some()),
                ))?;
                map.serialize_entry("type", &spec.action_type)?;
                if let Some(version) = spec.version {
                    map.serialize_entry("version", &version)?;
                }
                for (key, value) in &spec.fields {
                    map.serialize_entry(key, value)?;
                }
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for ActionSpec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let Value::Object(mut fields) = Value::deserialize(deserializer)? else {
            return Err(D::Error::custom("action step must be a map"));
        };
        let action_type = fields
            .remove("type")
            .and_then(|value| value.as_str().map(str::to_owned))
            .ok_or_else(|| D::Error::custom("action step requires a string field `type`"))?;

        if action_type == "archive" {
            return serde_json::from_value(Value::Object(fields))
                .map(Self::Archive)
                .map_err(D::Error::custom);
        }

        let version = fields
            .remove("version")
            .map(|value| {
                value
                    .as_u64()
                    .and_then(|value| u16::try_from(value).ok())
                    .ok_or_else(|| {
                        D::Error::custom("action `version` must be an unsigned 16-bit integer")
                    })
            })
            .transpose()?;

        Ok(Self::Unsupported(UnsupportedActionSpec {
            action_type,
            version,
            fields: fields.into_iter().collect(),
        }))
    }
}

impl Plan {
    /// Compatibility blockers that prevent execution but not safe editing or saving.
    #[must_use]
    pub fn execution_blockers(&self) -> Vec<ExecutionBlocker> {
        let mut blockers = Vec::new();
        for (task_index, task) in self.tasks.iter().enumerate() {
            for (step_index, step) in task.steps.iter().enumerate() {
                let path = format!("$.tasks[{task_index}].steps[{step_index}]");
                match step {
                    ActionSpec::Archive(spec) if spec.version != ActionVersion::V1 => {
                        blockers.push(ExecutionBlocker {
                            code: ExecutionBlockerCode::UnsupportedActionVersion,
                            path: format!("{path}.version"),
                            message: format!(
                                "archive action version {} is not supported",
                                spec.version.0
                            ),
                        });
                    }
                    ActionSpec::Unsupported(spec) => blockers.push(ExecutionBlocker {
                        code: ExecutionBlockerCode::UnsupportedActionType,
                        path: format!("{path}.type"),
                        message: format!("action type `{}` is not supported", spec.action_type),
                    }),
                    ActionSpec::Archive(_) => {}
                }
            }
        }
        blockers
    }
}

impl ContractValidation for Plan {
    fn validate(&self) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        if self.version != PlanVersion::CURRENT {
            issues.push(issue(
                ValidationCode::UnsupportedDocumentVersion,
                "$.version",
                format!(
                    "plan version {} is not supported; current version is {}",
                    self.version.0,
                    PlanVersion::CURRENT.0
                ),
            ));
        }
        if self.name.trim().is_empty() {
            issues.push(issue(
                ValidationCode::EmptyName,
                "$.name",
                "plan name must not be empty",
            ));
        }

        validate_extensions(
            &self.extensions,
            "$",
            &["version", "name", "tasks"],
            &mut issues,
        );

        let mut task_ids = HashSet::new();
        let mut sources = HashSet::new();
        for (task_index, task) in self.tasks.iter().enumerate() {
            let path = format!("$.tasks[{task_index}]");
            if !task_ids.insert(task.id) {
                issues.push(issue(
                    ValidationCode::DuplicateTaskId,
                    format!("{path}.id"),
                    "task id must be unique within a plan",
                ));
            }

            let source = task.source.to_string_lossy().into_owned();
            if source.trim().is_empty() {
                issues.push(issue(
                    ValidationCode::EmptySource,
                    format!("{path}.source"),
                    "task source must not be empty",
                ));
            } else if !sources.insert(source) {
                issues.push(issue(
                    ValidationCode::DuplicateSource,
                    format!("{path}.source"),
                    "task source must be unique within a plan",
                ));
            }

            if task.steps.len() != 1 {
                issues.push(issue(
                    ValidationCode::InvalidStepCount,
                    format!("{path}.steps"),
                    "plan version 1 requires exactly one action step per task",
                ));
            }

            validate_extensions(
                &task.extensions,
                &path,
                &["id", "source", "enabled", "profile_id", "steps"],
                &mut issues,
            );

            for (step_index, step) in task.steps.iter().enumerate() {
                let step_path = format!("{path}.steps[{step_index}]");
                match step {
                    ActionSpec::Archive(spec) => {
                        if spec.output.directory.as_os_str().is_empty() {
                            issues.push(issue(
                                ValidationCode::EmptyOutputDirectory,
                                format!("{step_path}.output.directory"),
                                "archive output directory must not be empty",
                            ));
                        }
                        if spec.output.filename.trim().is_empty() {
                            issues.push(issue(
                                ValidationCode::EmptyOutputFilename,
                                format!("{step_path}.output.filename"),
                                "archive output filename must not be empty",
                            ));
                        }
                        validate_extensions(
                            &spec.extensions,
                            &step_path,
                            &[
                                "type",
                                "version",
                                "output",
                                "include_root",
                                "unreadable_policy",
                                "verification",
                            ],
                            &mut issues,
                        );
                        validate_extensions(
                            &spec.output.extensions,
                            &format!("{step_path}.output"),
                            &[
                                "directory",
                                "filename",
                                "format",
                                "compression",
                                "conflict_policy",
                            ],
                            &mut issues,
                        );
                        validate_extensions(
                            &spec.verification.extensions,
                            &format!("{step_path}.verification"),
                            &["mode", "checksum"],
                            &mut issues,
                        );
                    }
                    ActionSpec::Unsupported(spec) => validate_extensions(
                        &spec.fields,
                        &step_path,
                        &["type", "version"],
                        &mut issues,
                    ),
                }
            }
        }
        issues
    }
}

fn validate_extensions(
    extensions: &Extensions,
    path: &str,
    reserved: &[&str],
    issues: &mut Vec<ValidationIssue>,
) {
    for field in reserved {
        if extensions.contains_key(*field) {
            issues.push(issue(
                ValidationCode::ReservedExtensionField,
                format!("{path}.{field}"),
                "extension fields must not shadow a known field",
            ));
        }
    }
}

fn issue(
    code: ValidationCode,
    path: impl Into<String>,
    message: impl Into<String>,
) -> ValidationIssue {
    ValidationIssue {
        code,
        path: path.into(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use foldry_core::{
        ArchiveFormat, ArchiveOutputSpec, ChecksumAlgorithm, CompressionLevel, ConflictPolicy,
        UnreadablePolicy, VerificationMode, VerificationSpec,
    };

    use super::*;

    fn archive_spec() -> ArchiveActionSpec {
        ArchiveActionSpec {
            version: ActionVersion::V1,
            output: ArchiveOutputSpec {
                directory: PathBuf::from("/tmp"),
                filename: "example-{date}".into(),
                format: ArchiveFormat::Zip,
                compression: CompressionLevel::Balanced,
                conflict_policy: ConflictPolicy::Increment,
                extensions: Extensions::new(),
            },
            include_root: true,
            unreadable_policy: UnreadablePolicy::Fail,
            verification: VerificationSpec {
                mode: VerificationMode::Structural,
                checksum: ChecksumAlgorithm::None,
                extensions: Extensions::new(),
            },
            extensions: Extensions::new(),
        }
    }

    #[test]
    fn unknown_action_round_trips_without_losing_fields() {
        let source = r#"{"type":"upload","version":3,"provider":"future","retry":{"count":2}}"#;
        let action = serde_json::from_str::<ActionSpec>(source).unwrap();
        let encoded = serde_json::to_value(&action).unwrap();

        assert_eq!(encoded["type"], "upload");
        assert_eq!(encoded["version"], 3);
        assert_eq!(encoded["provider"], "future");
        assert_eq!(encoded["retry"]["count"], 2);
    }

    #[test]
    fn invalid_known_archive_is_not_downgraded_to_unsupported() {
        let source = r#"{"type":"archive","version":1,"output":{}}"#;

        assert!(serde_json::from_str::<ActionSpec>(source).is_err());
    }

    #[test]
    fn future_action_version_is_an_execution_blocker() {
        let plan = Plan {
            version: PlanVersion::V1,
            name: "Active".into(),
            tasks: vec![Task {
                id: TaskId::new(),
                source: PathBuf::from("/source"),
                enabled: true,
                profile_id: ProfileId::new(),
                steps: vec![ActionSpec::Archive(ArchiveActionSpec {
                    version: ActionVersion(2),
                    ..archive_spec()
                })],
                extensions: Extensions::new(),
            }],
            extensions: Extensions::new(),
        };

        assert_eq!(
            plan.execution_blockers()[0].code,
            ExecutionBlockerCode::UnsupportedActionVersion
        );
    }
}
