use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
};

use foldry_core::{
    ActionId, ActionVersion, ArchiveActionSpec, ArchiveOutputDirectory, Extensions, FolderId,
    ProfileId,
};
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
    pub const V2: Self = Self(2);
    pub const CURRENT: Self = Self::V2;
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
    pub folders: Vec<Folder>,
    #[serde(default, skip_serializing_if = "Extensions::is_empty", flatten)]
    pub extensions: Extensions,
}

/// One remembered source folder and its independently runnable actions.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Folder {
    pub id: FolderId,
    pub source: PathBuf,
    pub listed: bool,
    pub enabled: bool,
    pub default_profile_id: ProfileId,
    pub actions: Vec<FolderAction>,
    #[serde(default, skip_serializing_if = "Extensions::is_empty", flatten)]
    pub extensions: Extensions,
}

/// Common identity and behavior around one typed action specification.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct FolderAction {
    pub id: ActionId,
    pub enabled: bool,
    pub profile_id_override: Option<ProfileId>,
    pub spec: ActionSpec,
    #[serde(default, skip_serializing_if = "Extensions::is_empty", flatten)]
    pub extensions: Extensions,
}

impl FolderAction {
    /// Returns the profile selected by this action after inheritance.
    #[must_use]
    pub fn effective_profile_id(&self, folder: &Folder) -> ProfileId {
        self.profile_id_override
            .unwrap_or(folder.default_profile_id)
    }
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
            return Err(D::Error::custom("action specification must be a map"));
        };
        let action_type = fields
            .remove("type")
            .and_then(|value| value.as_str().map(str::to_owned))
            .ok_or_else(|| {
                D::Error::custom("action specification requires a string field `type`")
            })?;

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
        for (folder_index, folder) in self.folders.iter().enumerate() {
            for (action_index, action) in folder.actions.iter().enumerate() {
                let path = format!("$.folders[{folder_index}].actions[{action_index}].spec");
                match &action.spec {
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
            &["version", "name", "folders"],
            &mut issues,
        );

        let mut folder_ids = HashSet::new();
        let mut action_ids = HashSet::new();
        let mut sources = HashSet::new();
        for (folder_index, folder) in self.folders.iter().enumerate() {
            let path = format!("$.folders[{folder_index}]");
            if !folder_ids.insert(folder.id) {
                issues.push(issue(
                    ValidationCode::DuplicateFolderId,
                    format!("{path}.id"),
                    "folder id must be unique within a plan",
                ));
            }

            let source = folder.source.to_string_lossy().into_owned();
            if source.trim().is_empty() {
                issues.push(issue(
                    ValidationCode::EmptySource,
                    format!("{path}.source"),
                    "folder source must not be empty",
                ));
            } else if !sources.insert(source) {
                issues.push(issue(
                    ValidationCode::DuplicateSource,
                    format!("{path}.source"),
                    "folder source must be unique within a plan",
                ));
            }

            validate_extensions(
                &folder.extensions,
                &path,
                &[
                    "id",
                    "source",
                    "listed",
                    "enabled",
                    "default_profile_id",
                    "actions",
                ],
                &mut issues,
            );

            for (action_index, action) in folder.actions.iter().enumerate() {
                let action_path = format!("{path}.actions[{action_index}]");
                if !action_ids.insert(action.id) {
                    issues.push(issue(
                        ValidationCode::DuplicateActionId,
                        format!("{action_path}.id"),
                        "action id must be unique within a plan",
                    ));
                }
                validate_extensions(
                    &action.extensions,
                    &action_path,
                    &["id", "enabled", "profile_id_override", "spec"],
                    &mut issues,
                );
                validate_action(&folder.source, &action.spec, &action_path, &mut issues);
            }
        }
        issues
    }
}

fn validate_action(
    source: &Path,
    action: &ActionSpec,
    action_path: &str,
    issues: &mut Vec<ValidationIssue>,
) {
    let spec_path = format!("{action_path}.spec");
    match action {
        ActionSpec::Archive(spec) => {
            if let ArchiveOutputDirectory::Custom { path } = &spec.output.directory {
                if path.as_os_str().is_empty() {
                    issues.push(issue(
                        ValidationCode::EmptyOutputDirectory,
                        format!("{spec_path}.output.directory.path"),
                        "custom archive output directory must not be empty",
                    ));
                } else if path == source || path.starts_with(source) {
                    issues.push(issue(
                        ValidationCode::OutputInsideSource,
                        format!("{spec_path}.output.directory.path"),
                        "custom archive output directory cannot equal or be inside the source",
                    ));
                }
            }
            if let Err(message) = validate_filename_template(&spec.output.filename) {
                issues.push(issue(
                    ValidationCode::InvalidOutputFilename,
                    format!("{spec_path}.output.filename"),
                    message,
                ));
            }
            validate_extensions(
                &spec.extensions,
                &spec_path,
                &[
                    "type",
                    "version",
                    "output",
                    "include_root",
                    "unreadable_policy",
                    "verification",
                ],
                issues,
            );
            validate_extensions(
                &spec.output.extensions,
                &format!("{spec_path}.output"),
                &[
                    "directory",
                    "filename",
                    "format",
                    "compression",
                    "conflict_policy",
                ],
                issues,
            );
            validate_extensions(
                &spec.verification.extensions,
                &format!("{spec_path}.verification"),
                &["mode", "checksum"],
                issues,
            );
        }
        ActionSpec::Unsupported(spec) => {
            validate_extensions(&spec.fields, &spec_path, &["type", "version"], issues)
        }
    }
}

/// Validates the supported archive filename tokens without resolving them.
pub fn validate_filename_template(template: &str) -> Result<(), String> {
    if template.trim().is_empty() {
        return Err("archive output filename template must not be empty".into());
    }
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let after_open = &rest[open + 1..];
        let Some(close) = after_open.find('}') else {
            return Err("archive output filename template contains an unmatched `{`".into());
        };
        let token = &after_open[..close];
        if !matches!(token, "folder" | "date") {
            return Err(format!(
                "archive output filename token `{{{token}}}` is not supported"
            ));
        }
        rest = &after_open[close + 1..];
    }
    if rest.contains('}') {
        return Err("archive output filename template contains an unmatched `}`".into());
    }
    Ok(())
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
    use foldry_core::{
        ArchiveFormat, ArchiveOutputSpec, ChecksumAlgorithm, CompressionLevel, ConflictPolicy,
        UnreadablePolicy, VerificationMode, VerificationSpec,
    };

    use super::*;

    fn archive_spec() -> ArchiveActionSpec {
        ArchiveActionSpec {
            version: ActionVersion::V1,
            output: ArchiveOutputSpec {
                directory: ArchiveOutputDirectory::Parent,
                filename: "{folder}.{date}".into(),
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
            version: PlanVersion::V2,
            name: "Active".into(),
            folders: vec![Folder {
                id: FolderId::new(),
                source: PathBuf::from("/source"),
                listed: true,
                enabled: true,
                default_profile_id: ProfileId::new(),
                actions: vec![FolderAction {
                    id: ActionId::new(),
                    enabled: false,
                    profile_id_override: None,
                    spec: ActionSpec::Archive(ArchiveActionSpec {
                        version: ActionVersion(2),
                        ..archive_spec()
                    }),
                    extensions: Extensions::new(),
                }],
                extensions: Extensions::new(),
            }],
            extensions: Extensions::new(),
        };

        assert_eq!(
            plan.execution_blockers()[0].code,
            ExecutionBlockerCode::UnsupportedActionVersion
        );
    }

    #[test]
    fn filename_template_accepts_only_folder_and_date_tokens() {
        assert!(validate_filename_template("{folder}.{date}").is_ok());
        assert!(validate_filename_template("{profile}").is_err());
        assert!(validate_filename_template("{folder").is_err());
        assert!(validate_filename_template("folder}").is_err());
    }
}
