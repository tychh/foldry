use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use foldry_application::{
    PresetCatalog, PresetCatalogError, PresetDefinition, PresetId, PresetVersion, parse_profile,
};
use thiserror::Error;

/// Failure to read or validate the shipped preset resource directory.
#[derive(Debug, Error)]
pub enum ResourcePresetError {
    #[error("cannot read preset resource {path}: {message}")]
    Io { path: PathBuf, message: String },
    #[error("preset resource {path} is invalid: {message}")]
    Invalid { path: PathBuf, message: String },
    #[error("preset catalog is invalid: {0:?}")]
    Catalog(PresetCatalogError),
}

/// Loads all `.packignore` resources and validates their metadata and patterns.
pub fn load_preset_catalog(directory: &Path) -> Result<PresetCatalog, ResourcePresetError> {
    let mut paths = fs::read_dir(directory)
        .map_err(|error| io_error(directory, error))?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|error| io_error(directory, error))
        })
        .collect::<Result<Vec<_>, _>>()?;
    paths.retain(|path| {
        path.extension()
            .is_some_and(|extension| extension == "packignore")
    });
    paths.sort();

    let definitions = paths
        .iter()
        .map(|path| load_definition(path))
        .collect::<Result<Vec<_>, _>>()?;
    PresetCatalog::new(definitions).map_err(ResourcePresetError::Catalog)
}

fn load_definition(path: &Path) -> Result<PresetDefinition, ResourcePresetError> {
    let text = fs::read_to_string(path).map_err(|error| io_error(path, error))?;
    let mut id = None;
    let mut version = None;
    let mut name = None;
    let mut description = None;
    let mut sensitive = None;
    let mut historical_hashes = BTreeMap::new();
    let mut content_start = None;
    let mut offset = 0;

    for line in text.split_inclusive('\n') {
        let logical = line.trim_end_matches(['\r', '\n']);
        if let Some(value) = logical.strip_prefix("# @preset-id ") {
            id = Some(
                value
                    .trim()
                    .parse()
                    .map_err(|error| invalid(path, format!("invalid id: {error}")))?,
            );
        } else if let Some(value) = logical.strip_prefix("# @preset-version ") {
            version =
                Some(PresetVersion(value.trim().parse().map_err(|error| {
                    invalid(path, format!("invalid version: {error}"))
                })?));
        } else if let Some(value) = logical.strip_prefix("# @preset-name ") {
            name = Some(value.trim().to_owned());
        } else if let Some(value) = logical.strip_prefix("# @preset-description ") {
            description = Some(value.trim().to_owned());
        } else if let Some(value) = logical.strip_prefix("# @preset-safety ") {
            sensitive = Some(match value.trim() {
                "safe" => false,
                "sensitive" => true,
                other => {
                    return Err(invalid(
                        path,
                        format!("unknown safety classification `{other}`"),
                    ));
                }
            });
        } else if let Some(value) = logical.strip_prefix("# @preset-previous ") {
            let (past_version, hash) = parse_previous(path, value)?;
            if historical_hashes.insert(past_version, hash).is_some() {
                return Err(invalid(path, "duplicate historical preset version"));
            }
        } else if logical.is_empty() {
            content_start = Some(offset + line.len());
            break;
        } else if !logical.starts_with('#') {
            return Err(invalid(path, "metadata header must end with a blank line"));
        }
        offset += line.len();
    }

    let id: PresetId = id.ok_or_else(|| invalid(path, "missing @preset-id"))?;
    let version = version.ok_or_else(|| invalid(path, "missing @preset-version"))?;
    if version.0 == 0 {
        return Err(invalid(path, "preset version must be greater than zero"));
    }
    let name = name
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid(path, "missing @preset-name"))?;
    let description = description
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid(path, "missing @preset-description"))?;
    let sensitive = sensitive.ok_or_else(|| invalid(path, "missing @preset-safety"))?;
    let content_start =
        content_start.ok_or_else(|| invalid(path, "missing blank line after metadata"))?;
    let content = &text[content_start..];

    let validation_text = format!(
        "# @profile-id 0190f5f0-7f8b-7d80-a120-4f4f9fe95c20\n\
         # @profile-version 1\n# @profile-name Preset validation\n{content}"
    );
    let parsed = parse_profile(&validation_text);
    if !parsed.is_valid() {
        return Err(invalid(
            path,
            parsed
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.message.as_str())
                .collect::<Vec<_>>()
                .join("; "),
        ));
    }

    Ok(PresetDefinition {
        id,
        version,
        name,
        description,
        sensitive,
        content: content.to_owned(),
        historical_hashes,
    })
}

fn parse_previous(
    path: &Path,
    attributes: &str,
) -> Result<(PresetVersion, String), ResourcePresetError> {
    let mut version = None;
    let mut hash = None;
    for attribute in attributes.split_ascii_whitespace() {
        if let Some(value) = attribute.strip_prefix("version=") {
            version = Some(PresetVersion(value.parse().map_err(|error| {
                invalid(path, format!("invalid historical version: {error}"))
            })?));
        } else if let Some(value) = attribute.strip_prefix("hash=") {
            hash = Some(value.to_owned());
        }
    }
    let version = version.ok_or_else(|| invalid(path, "historical hash requires version"))?;
    let hash = hash.ok_or_else(|| invalid(path, "historical hash requires hash"))?;
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid(
            path,
            "historical hash must be a 64-character SHA-256",
        ));
    }
    Ok((version, hash.to_ascii_lowercase()))
}

fn io_error(path: &Path, error: std::io::Error) -> ResourcePresetError {
    ResourcePresetError::Io {
        path: path.to_owned(),
        message: error.to_string(),
    }
}

fn invalid(path: &Path, message: impl Into<String>) -> ResourcePresetError {
    ResourcePresetError::Invalid {
        path: path.to_owned(),
        message: message.into(),
    }
}
