use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::PresetId;

/// Version of one reusable preset's content.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct PresetVersion(pub u16);

/// One current built-in or user-defined preset.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PresetDefinition {
    pub id: PresetId,
    pub version: PresetVersion,
    pub name: String,
    pub description: String,
    pub sensitive: bool,
    pub content: String,
    #[serde(default)]
    pub historical_hashes: BTreeMap<PresetVersion, String>,
}

impl PresetDefinition {
    /// Hash of normalized current content, excluding profile marker lines.
    #[must_use]
    pub fn current_hash(&self) -> String {
        preset_content_hash(&self.content)
    }
}

/// Installation state of one preset block in a profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresetState {
    Absent,
    Installed {
        version: PresetVersion,
    },
    Outdated {
        installed_version: PresetVersion,
        current_version: PresetVersion,
    },
    Modified {
        declared_version: PresetVersion,
    },
}

/// Explicit approval required before inserting sensitive exclusions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SensitivePresetApproval {
    NotGranted,
    Granted,
}

/// Explicit approval required before replacing or deleting modified text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModifiedBlockConfirmation {
    NotGranted,
    Granted,
}

/// Indexed definitions with duplicate protection.
pub struct PresetCatalog {
    definitions: BTreeMap<PresetId, PresetDefinition>,
}

impl PresetCatalog {
    pub fn new(
        definitions: impl IntoIterator<Item = PresetDefinition>,
    ) -> Result<Self, PresetCatalogError> {
        let mut indexed = BTreeMap::new();
        for mut definition in definitions {
            if definition.version.0 == 0 {
                return Err(PresetCatalogError::ZeroVersion(definition.id));
            }
            definition.content = normalize_preset_content(&definition.content);
            let id = definition.id.clone();
            if indexed.insert(id.clone(), definition).is_some() {
                return Err(PresetCatalogError::DuplicateId(id));
            }
        }
        Ok(Self {
            definitions: indexed,
        })
    }

    #[must_use]
    pub fn get(&self, id: &PresetId) -> Option<&PresetDefinition> {
        self.definitions.get(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &PresetDefinition> {
        self.definitions.values()
    }

    pub fn state(&self, profile_text: &str, id: &PresetId) -> Result<PresetState, PresetEditError> {
        let definition = self
            .get(id)
            .ok_or_else(|| PresetEditError::UnknownPreset(id.clone()))?;
        let blocks = extract_blocks(profile_text)?;
        let Some(block) = blocks.get(id) else {
            return Ok(PresetState::Absent);
        };
        let actual_hash =
            preset_content_hash(&profile_text[block.content_start..block.content_end]);
        if block.version == definition.version && actual_hash == definition.current_hash() {
            return Ok(PresetState::Installed {
                version: block.version,
            });
        }
        if block.version < definition.version
            && definition
                .historical_hashes
                .get(&block.version)
                .is_some_and(|hash| hash == &actual_hash)
        {
            return Ok(PresetState::Outdated {
                installed_version: block.version,
                current_version: definition.version,
            });
        }
        Ok(PresetState::Modified {
            declared_version: block.version,
        })
    }

    pub fn insert(
        &self,
        profile_text: &str,
        id: &PresetId,
        sensitive_approval: SensitivePresetApproval,
    ) -> Result<String, PresetEditError> {
        let definition = self
            .get(id)
            .ok_or_else(|| PresetEditError::UnknownPreset(id.clone()))?;
        if definition.sensitive && sensitive_approval != SensitivePresetApproval::Granted {
            return Err(PresetEditError::SensitiveApprovalRequired(id.clone()));
        }
        let state = self.state(profile_text, id)?;
        if state != PresetState::Absent {
            return Err(PresetEditError::AlreadyPresent {
                id: id.clone(),
                state,
            });
        }

        let mut edited = profile_text.replace("\r\n", "\n");
        if !edited.ends_with('\n') {
            edited.push('\n');
        }
        if !edited.ends_with("\n\n") {
            edited.push('\n');
        }
        edited.push_str(&render_block(definition));
        Ok(edited)
    }

    pub fn remove(
        &self,
        profile_text: &str,
        id: &PresetId,
        confirmation: ModifiedBlockConfirmation,
    ) -> Result<String, PresetEditError> {
        let state = self.state(profile_text, id)?;
        if state == PresetState::Absent {
            return Err(PresetEditError::BlockAbsent(id.clone()));
        }
        if matches!(state, PresetState::Modified { .. })
            && confirmation != ModifiedBlockConfirmation::Granted
        {
            return Err(PresetEditError::ModifiedConfirmationRequired(id.clone()));
        }
        let blocks = extract_blocks(profile_text)?;
        let block = blocks
            .get(id)
            .expect("state detected an existing preset block");
        let mut edited = String::with_capacity(profile_text.len() - (block.end - block.start));
        edited.push_str(&profile_text[..block.start]);
        edited.push_str(&profile_text[block.end..]);
        Ok(collapse_boundary_blank_lines(edited))
    }

    pub fn update(
        &self,
        profile_text: &str,
        id: &PresetId,
        confirmation: ModifiedBlockConfirmation,
        sensitive_approval: SensitivePresetApproval,
    ) -> Result<String, PresetEditError> {
        let definition = self
            .get(id)
            .ok_or_else(|| PresetEditError::UnknownPreset(id.clone()))?;
        if definition.sensitive && sensitive_approval != SensitivePresetApproval::Granted {
            return Err(PresetEditError::SensitiveApprovalRequired(id.clone()));
        }
        let state = self.state(profile_text, id)?;
        match state {
            PresetState::Absent => return Err(PresetEditError::BlockAbsent(id.clone())),
            PresetState::Installed { .. } => return Ok(profile_text.to_owned()),
            PresetState::Modified { .. } if confirmation != ModifiedBlockConfirmation::Granted => {
                return Err(PresetEditError::ModifiedConfirmationRequired(id.clone()));
            }
            PresetState::Modified { .. } | PresetState::Outdated { .. } => {}
        }

        let blocks = extract_blocks(profile_text)?;
        let block = blocks
            .get(id)
            .expect("state detected an existing preset block");
        let replacement = render_block(definition);
        let mut edited = String::with_capacity(
            profile_text.len() - (block.end - block.start) + replacement.len(),
        );
        edited.push_str(&profile_text[..block.start]);
        edited.push_str(&replacement);
        edited.push_str(&profile_text[block.end..]);
        Ok(edited)
    }
}

/// Catalog construction error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PresetCatalogError {
    DuplicateId(PresetId),
    ZeroVersion(PresetId),
}

/// Pure profile editing failure; the original string is never modified.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PresetEditError {
    UnknownPreset(PresetId),
    BlockAbsent(PresetId),
    AlreadyPresent { id: PresetId, state: PresetState },
    SensitiveApprovalRequired(PresetId),
    ModifiedConfirmationRequired(PresetId),
    InvalidMarkers(String),
}

#[derive(Clone, Debug)]
struct PresetBlock {
    version: PresetVersion,
    start: usize,
    content_start: usize,
    content_end: usize,
    end: usize,
}

/// Normalizes line endings and guarantees one final newline.
#[must_use]
pub fn normalize_preset_content(content: &str) -> String {
    let mut normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    while normalized.ends_with('\n') {
        normalized.pop();
    }
    normalized.push('\n');
    normalized
}

/// Lowercase SHA-256 of normalized marker-free content.
#[must_use]
pub fn preset_content_hash(content: &str) -> String {
    let normalized = normalize_preset_content(content);
    let digest = Sha256::digest(normalized.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn render_block(definition: &PresetDefinition) -> String {
    format!(
        "# @preset-begin id={} version={}\n{}# @preset-end id={}\n",
        definition.id,
        definition.version.0,
        normalize_preset_content(&definition.content),
        definition.id
    )
}

fn extract_blocks(profile_text: &str) -> Result<HashMap<PresetId, PresetBlock>, PresetEditError> {
    let mut blocks = HashMap::new();
    let mut active: Option<(PresetId, PresetVersion, usize, usize)> = None;
    let mut offset = 0;

    for line in profile_text.split_inclusive('\n') {
        let logical = line
            .strip_suffix('\n')
            .unwrap_or(line)
            .strip_suffix('\r')
            .unwrap_or_else(|| line.strip_suffix('\n').unwrap_or(line));
        if let Some(attributes) = logical.strip_prefix("# @preset-begin ") {
            if active.is_some() {
                return Err(PresetEditError::InvalidMarkers(
                    "preset blocks cannot be nested".into(),
                ));
            }
            let (id, version) = parse_begin(attributes)?;
            if blocks.contains_key(&id) {
                return Err(PresetEditError::InvalidMarkers(format!(
                    "preset `{id}` occurs more than once"
                )));
            }
            active = Some((id, version, offset, offset + line.len()));
        } else if let Some(attributes) = logical.strip_prefix("# @preset-end ") {
            let end_id = parse_end(attributes)?;
            let Some((begin_id, version, start, content_start)) = active.take() else {
                return Err(PresetEditError::InvalidMarkers(format!(
                    "preset end marker `{end_id}` has no begin marker"
                )));
            };
            if begin_id != end_id {
                return Err(PresetEditError::InvalidMarkers(format!(
                    "preset `{begin_id}` is closed as `{end_id}`"
                )));
            }
            blocks.insert(
                begin_id,
                PresetBlock {
                    version,
                    start,
                    content_start,
                    content_end: offset,
                    end: offset + line.len(),
                },
            );
        }
        offset += line.len();
    }

    if let Some((id, _, _, _)) = active {
        return Err(PresetEditError::InvalidMarkers(format!(
            "preset `{id}` has no end marker"
        )));
    }
    Ok(blocks)
}

fn parse_begin(attributes: &str) -> Result<(PresetId, PresetVersion), PresetEditError> {
    let mut id = None;
    let mut version = None;
    for attribute in attributes.split_ascii_whitespace() {
        if let Some(value) = attribute.strip_prefix("id=") {
            id = Some(value.parse().map_err(|error| {
                PresetEditError::InvalidMarkers(format!("invalid preset id: {error}"))
            })?);
        } else if let Some(value) = attribute.strip_prefix("version=") {
            version = Some(PresetVersion(value.parse::<u16>().map_err(|error| {
                PresetEditError::InvalidMarkers(format!("invalid preset version: {error}"))
            })?));
        }
    }
    match (id, version) {
        (Some(id), Some(version)) if version.0 > 0 => Ok((id, version)),
        _ => Err(PresetEditError::InvalidMarkers(
            "preset begin marker requires non-zero `id` and `version`".into(),
        )),
    }
}

fn parse_end(attributes: &str) -> Result<PresetId, PresetEditError> {
    let value = attributes
        .trim()
        .strip_prefix("id=")
        .ok_or_else(|| PresetEditError::InvalidMarkers("preset end marker requires `id`".into()))?;
    value
        .parse()
        .map_err(|error| PresetEditError::InvalidMarkers(format!("invalid preset id: {error}")))
}

fn collapse_boundary_blank_lines(mut text: String) -> String {
    while text.contains("\n\n\n") {
        text = text.replace("\n\n\n", "\n\n");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn definition(content: &str, sensitive: bool) -> PresetDefinition {
        PresetDefinition {
            id: "python".parse().unwrap(),
            version: PresetVersion(2),
            name: "Python".into(),
            description: "Python artifacts".into(),
            sensitive,
            content: content.into(),
            historical_hashes: BTreeMap::from([(PresetVersion(1), preset_content_hash("*.pyc\n"))]),
        }
    }

    #[test]
    fn detects_absent_installed_outdated_and_modified_states() {
        let catalog = PresetCatalog::new([definition("*.py[cod]\n", false)]).unwrap();
        let id = "python".parse().unwrap();

        assert_eq!(catalog.state("header\n", &id).unwrap(), PresetState::Absent);
        let installed = catalog
            .insert("header\n", &id, SensitivePresetApproval::NotGranted)
            .unwrap();
        assert_eq!(
            catalog.state(&installed, &id).unwrap(),
            PresetState::Installed {
                version: PresetVersion(2)
            }
        );
        let outdated = "# @preset-begin id=python version=1\n*.pyc\n# @preset-end id=python\n";
        assert_eq!(
            catalog.state(outdated, &id).unwrap(),
            PresetState::Outdated {
                installed_version: PresetVersion(1),
                current_version: PresetVersion(2)
            }
        );
        let modified = installed.replace("*.py[cod]", "*.pyc");
        assert_eq!(
            catalog.state(&modified, &id).unwrap(),
            PresetState::Modified {
                declared_version: PresetVersion(2)
            }
        );
    }

    #[test]
    fn sensitive_insert_and_modified_delete_require_confirmation() {
        let catalog = PresetCatalog::new([definition(".env\n", true)]).unwrap();
        let id = "python".parse().unwrap();
        assert_eq!(
            catalog
                .insert("header\n", &id, SensitivePresetApproval::NotGranted)
                .unwrap_err(),
            PresetEditError::SensitiveApprovalRequired(id.clone())
        );

        let installed = catalog
            .insert("header\n", &id, SensitivePresetApproval::Granted)
            .unwrap();
        let modified = installed.replace(".env", ".env.local");
        assert_eq!(
            catalog
                .remove(&modified, &id, ModifiedBlockConfirmation::NotGranted)
                .unwrap_err(),
            PresetEditError::ModifiedConfirmationRequired(id)
        );
    }

    #[test]
    fn update_is_all_or_nothing_and_preserves_surrounding_text() {
        let catalog = PresetCatalog::new([definition("*.py[cod]\n", false)]).unwrap();
        let id = "python".parse().unwrap();
        let source =
            "before\n# @preset-begin id=python version=1\n*.pyc\n# @preset-end id=python\nafter\n";

        let updated = catalog
            .update(
                source,
                &id,
                ModifiedBlockConfirmation::NotGranted,
                SensitivePresetApproval::NotGranted,
            )
            .unwrap();

        assert!(updated.starts_with("before\n"));
        assert!(updated.ends_with("after\n"));
        assert!(updated.contains("version=2"));
        assert!(updated.contains("*.py[cod]"));
    }
}
