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

    let loaded = paths
        .iter()
        .map(|path| load_definition(path))
        .collect::<Result<Vec<_>, _>>()?;
    let mut by_id = BTreeMap::new();
    for definition in loaded {
        let id = definition.definition.id.clone();
        if by_id.insert(id.clone(), definition).is_some() {
            return Err(invalid(directory, format!("duplicate preset id `{id}`")));
        }
    }
    let ids = by_id.keys().cloned().collect::<Vec<_>>();
    let definitions = ids
        .iter()
        .map(|id| resolve_definition(id, &by_id, &mut Vec::new()))
        .collect::<Result<Vec<_>, _>>()?;
    PresetCatalog::new(definitions).map_err(ResourcePresetError::Catalog)
}

struct LoadedDefinition {
    definition: PresetDefinition,
    includes: Vec<PresetId>,
    path: PathBuf,
}

fn load_definition(path: &Path) -> Result<LoadedDefinition, ResourcePresetError> {
    let text = fs::read_to_string(path).map_err(|error| io_error(path, error))?;
    let mut id = None;
    let mut version = None;
    let mut name = None;
    let mut description = None;
    let mut sensitive = None;
    let mut historical_hashes = BTreeMap::new();
    let mut includes = Vec::new();
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
        } else if let Some(value) = logical.strip_prefix("# @preset-include id=") {
            let include = value
                .trim()
                .parse::<PresetId>()
                .map_err(|error| invalid(path, format!("invalid included preset id: {error}")))?;
            if includes.contains(&include) {
                return Err(invalid(
                    path,
                    format!("duplicate preset include `{include}`"),
                ));
            }
            includes.push(include);
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

    let profile_version = if content.contains("# @rule-source ") {
        2
    } else {
        1
    };
    let validation_text = format!(
        "# @profile-id 0190f5f0-7f8b-7d80-a120-4f4f9fe95c20\n\
         # @profile-version {profile_version}\n# @profile-name Preset validation\n{content}"
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

    Ok(LoadedDefinition {
        definition: PresetDefinition {
            id,
            version,
            name,
            description,
            sensitive,
            content: content.to_owned(),
            historical_hashes,
        },
        includes,
        path: path.to_owned(),
    })
}

fn resolve_definition(
    id: &PresetId,
    definitions: &BTreeMap<PresetId, LoadedDefinition>,
    stack: &mut Vec<PresetId>,
) -> Result<PresetDefinition, ResourcePresetError> {
    let loaded = definitions.get(id).ok_or_else(|| {
        invalid(
            Path::new("presets"),
            format!("unknown included preset `{id}`"),
        )
    })?;
    if stack.contains(id) {
        let mut cycle = stack.iter().map(ToString::to_string).collect::<Vec<_>>();
        cycle.push(id.to_string());
        return Err(invalid(
            &loaded.path,
            format!("preset include cycle: {}", cycle.join(" -> ")),
        ));
    }
    stack.push(id.clone());
    let mut definition = loaded.definition.clone();
    for include_id in &loaded.includes {
        let included = resolve_definition(include_id, definitions, stack)?;
        definition.content.push_str(&included.content);
        definition.sensitive |= included.sensitive;
    }
    stack.pop();
    Ok(definition)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn development_preset_flattens_components_without_nested_markers() {
        let resources = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../resources/presets");
        let catalog = load_preset_catalog(&resources).unwrap();
        let preset = catalog
            .get(&"development-project".parse().unwrap())
            .unwrap();

        assert_eq!(preset.version, PresetVersion(2));
        assert!(!preset.sensitive);
        assert!(preset.content.contains("# @rule-source gitignore"));
        for expected_rule in [
            ".venv/",
            ".mypy_cache/",
            ".ruff_cache/",
            "__pycache__/",
            "node_modules/",
            "target/",
            ".gradle/",
            "bin/",
            "vendor/",
            "*.o",
            "CMakeFiles/",
            ".bundle/",
            ".phpunit.cache/",
            ".idea/",
            ".vscode-test/",
            "test-results/",
            "coverage/",
            ".git/",
            ".DS_Store",
        ] {
            assert!(preset.content.contains(expected_rule), "{expected_rule}");
        }
        assert!(!preset.content.contains("# @preset-begin"));
    }

    #[test]
    fn development_preset_excludes_cross_language_artifacts_but_keeps_other_gitignored_paths() {
        use foldry_application::{
            CompiledProfile, FileSystemCaseSensitivity, MatchDecision, resolve_effective_profile,
        };

        let resources = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../resources/presets");
        let catalog = load_preset_catalog(&resources).unwrap();
        let preset = catalog
            .get(&"development-project".parse().unwrap())
            .unwrap();
        let source = tempfile::tempdir().unwrap();
        std::fs::write(source.path().join(".gitignore"), "*\n").unwrap();
        let profile_text = format!(
            "# @profile-id 0190f5f0-7f8b-7d80-a120-4f4f9fe95c20\n\
             # @profile-version 2\n# @profile-name Development\n{}",
            preset.content
        );
        let snapshot = resolve_effective_profile(&profile_text, source.path()).unwrap();
        let matcher = CompiledProfile::from_snapshot(
            &snapshot,
            source.path(),
            FileSystemCaseSensitivity::Sensitive,
        )
        .unwrap();

        for artifact in [
            ".venv/lib/python/site.py",
            ".mypy_cache/state.json",
            ".ruff_cache/cache.db",
            "pkg/__pycache__/module.pyc",
            "node_modules/pkg/index.js",
            "target/debug/foldry",
            ".gradle/cache.bin",
            "bin/tool",
            "vendor/pkg/source.go",
            "obj/module.o",
            "CMakeFiles/project.dir/cache",
            ".bundle/config",
            ".phpunit.cache/result",
            ".idea/workspace.xml",
            ".vscode-test/extensions.json",
            "test-results/results.json",
            "coverage/index.html",
            ".git/objects/pack/data",
            ".DS_Store",
        ] {
            assert_eq!(
                matcher.matched(artifact, false).unwrap().decision,
                MatchDecision::Exclude,
                "{artifact}"
            );
        }
        assert_eq!(
            matcher.matched("local-notes.txt", false).unwrap().decision,
            MatchDecision::Include
        );
    }

    #[test]
    fn include_cycles_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("a.packignore"),
            "# @preset-id a\n# @preset-version 1\n# @preset-name A\n# @preset-description A.\n# @preset-safety safe\n# @preset-include id=b\n\n*.a\n",
        ).unwrap();
        std::fs::write(
            directory.path().join("b.packignore"),
            "# @preset-id b\n# @preset-version 1\n# @preset-name B\n# @preset-description B.\n# @preset-safety safe\n# @preset-include id=a\n\n*.b\n",
        ).unwrap();

        assert!(matches!(
            load_preset_catalog(directory.path()),
            Err(ResourcePresetError::Invalid { .. })
        ));
    }
}
