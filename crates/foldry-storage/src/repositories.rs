use std::{
    fs, io,
    path::{Path, PathBuf},
};

use atomicwrites::{AllowOverwrite, AtomicFile};
use foldry_application::{
    ActivePlanRepository, DEFAULT_PROFILE_FILENAME, Plan, PresetId, PresetRepository, ProfileId,
    ProfileRepository, RepositoryError, Settings, SettingsRepository, StoredPreset, StoredProfile,
    parse_profile,
};

use crate::{decode_plan, decode_settings, encode_plan, encode_settings};

pub struct FileSettingsRepository {
    path: PathBuf,
}

impl FileSettingsRepository {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl SettingsRepository for FileSettingsRepository {
    fn load(&self) -> Result<Option<Settings>, RepositoryError> {
        read_optional(&self.path)?
            .map(|source| decode_settings(&source).map_err(repository_error))
            .transpose()
    }

    fn save(&self, settings: &Settings) -> Result<(), RepositoryError> {
        let encoded = encode_settings(settings).map_err(repository_error)?;
        decode_settings(&encoded).map_err(repository_error)?;
        atomic_write(&self.path, encoded.as_bytes())
    }
}

pub struct FileActivePlanRepository {
    path: PathBuf,
}

impl FileActivePlanRepository {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl ActivePlanRepository for FileActivePlanRepository {
    fn load(&self) -> Result<Option<Plan>, RepositoryError> {
        read_optional(&self.path)?
            .map(|source| decode_plan(&source).map_err(repository_error))
            .transpose()
    }

    fn save(&self, plan: &Plan) -> Result<(), RepositoryError> {
        let encoded = encode_plan(plan).map_err(repository_error)?;
        decode_plan(&encoded).map_err(repository_error)?;
        atomic_write(&self.path, encoded.as_bytes())
    }
}

pub struct FileProfileRepository {
    directory: PathBuf,
    default_resource: PathBuf,
}

impl FileProfileRepository {
    #[must_use]
    pub fn new(directory: PathBuf, default_resource: PathBuf) -> Self {
        Self {
            directory,
            default_resource,
        }
    }

    fn read_profile(&self, path: PathBuf) -> Result<StoredProfile, RepositoryError> {
        let text = fs::read_to_string(&path).map_err(repository_error)?;
        Ok(stored_profile(path, text))
    }
}

impl ProfileRepository for FileProfileRepository {
    fn list(&self) -> Result<Vec<StoredProfile>, RepositoryError> {
        fs::create_dir_all(&self.directory).map_err(repository_error)?;
        let mut profiles = fs::read_dir(&self.directory)
            .map_err(repository_error)?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "packignore")
            })
            .map(|path| self.read_profile(path))
            .collect::<Result<Vec<_>, _>>()?;
        profiles.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(profiles)
    }

    fn get(&self, id: ProfileId) -> Result<Option<StoredProfile>, RepositoryError> {
        Ok(self
            .list()?
            .into_iter()
            .find(|profile| profile.id == Some(id)))
    }

    fn save_text(&self, filename: &str, text: &str) -> Result<StoredProfile, RepositoryError> {
        validate_filename(filename, "packignore")?;
        fs::create_dir_all(&self.directory).map_err(repository_error)?;
        let path = self.directory.join(filename);
        if let Some(current) = read_optional(&path)?
            && parse_profile(&current).profile.is_some()
        {
            atomic_write(&previous_good_path(&path), current.as_bytes())?;
        }
        atomic_write(&path, text.as_bytes())?;
        Ok(stored_profile(path, text.to_owned()))
    }

    fn delete(&self, id: ProfileId) -> Result<bool, RepositoryError> {
        let Some(profile) = self.get(id)? else {
            return Ok(false);
        };
        if profile.path.file_name().and_then(|name| name.to_str()) == Some(DEFAULT_PROFILE_FILENAME)
        {
            return Err(RepositoryError::new(
                "the default profile cannot be deleted",
            ));
        }
        fs::remove_file(profile.path).map_err(repository_error)?;
        Ok(true)
    }

    fn restore_default(&self) -> Result<StoredProfile, RepositoryError> {
        let text = fs::read_to_string(&self.default_resource).map_err(repository_error)?;
        self.save_text(DEFAULT_PROFILE_FILENAME, &text)
    }
}

pub struct FilePresetRepository {
    directory: PathBuf,
    resources: PathBuf,
}

impl FilePresetRepository {
    #[must_use]
    pub fn new(directory: PathBuf, resources: PathBuf) -> Self {
        Self {
            directory,
            resources,
        }
    }

    fn read_preset(path: PathBuf) -> Result<StoredPreset, RepositoryError> {
        let text = fs::read_to_string(&path).map_err(repository_error)?;
        let (id, version) = preset_metadata(&text)?;
        Ok(StoredPreset {
            id,
            path,
            text,
            resource_version: Some(version),
        })
    }
}

impl PresetRepository for FilePresetRepository {
    fn list(&self) -> Result<Vec<StoredPreset>, RepositoryError> {
        fs::create_dir_all(&self.directory).map_err(repository_error)?;
        let mut presets = fs::read_dir(&self.directory)
            .map_err(repository_error)?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "packignore")
            })
            .map(Self::read_preset)
            .collect::<Result<Vec<_>, _>>()?;
        presets.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(presets)
    }

    fn save_text(&self, filename: &str, text: &str) -> Result<StoredPreset, RepositoryError> {
        validate_filename(filename, "packignore")?;
        preset_metadata(text)?;
        fs::create_dir_all(&self.directory).map_err(repository_error)?;
        let path = self.directory.join(filename);
        atomic_write(&path, text.as_bytes())?;
        Self::read_preset(path)
    }

    fn delete(&self, id: &PresetId) -> Result<bool, RepositoryError> {
        let Some(preset) = self.list()?.into_iter().find(|preset| &preset.id == id) else {
            return Ok(false);
        };
        fs::remove_file(preset.path).map_err(repository_error)?;
        Ok(true)
    }

    fn reset_from_resources(&self, id: &PresetId) -> Result<StoredPreset, RepositoryError> {
        let resource = fs::read_dir(&self.resources)
            .map_err(repository_error)?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| {
                fs::read_to_string(path)
                    .ok()
                    .and_then(|text| preset_metadata(&text).ok())
                    .is_some_and(|(resource_id, _)| &resource_id == id)
            })
            .ok_or_else(|| RepositoryError::new(format!("resource preset `{id}` not found")))?;
        let text = fs::read_to_string(&resource).map_err(repository_error)?;
        let filename = resource
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| RepositoryError::new("resource preset filename is not UTF-8"))?;
        self.save_text(filename, &text)
    }
}

pub fn install_missing_resources(resources: &Path, working: &Path) -> Result<u64, RepositoryError> {
    fs::create_dir_all(working).map_err(repository_error)?;
    let mut installed = 0;
    for entry in fs::read_dir(resources).map_err(repository_error)? {
        let entry = entry.map_err(repository_error)?;
        let source = entry.path();
        if !source.is_file() {
            continue;
        }
        let target = working.join(entry.file_name());
        if target.exists() {
            continue;
        }
        let contents = fs::read(&source).map_err(repository_error)?;
        atomic_write(&target, &contents)?;
        installed += 1;
    }
    Ok(installed)
}

pub fn initialize_resource_copies(
    resource_root: &Path,
    config_directory: &Path,
) -> Result<bool, RepositoryError> {
    let marker = config_directory.join(".resource-copies-v1-installed");
    if marker.exists() {
        return Ok(false);
    }
    let profiles = config_directory.join("profiles");
    let presets = config_directory.join("presets");
    fs::create_dir_all(&profiles).map_err(repository_error)?;
    fs::create_dir_all(&presets).map_err(repository_error)?;
    let default_target = profiles.join(DEFAULT_PROFILE_FILENAME);
    if !default_target.exists() {
        let contents = fs::read(resource_root.join("profiles/default.packignore"))
            .map_err(repository_error)?;
        atomic_write(&default_target, &contents)?;
    }
    install_missing_resources(&resource_root.join("presets"), &presets)?;
    atomic_write(&marker, b"Foldry resource working copies v1\n")?;
    Ok(true)
}

fn stored_profile(path: PathBuf, text: String) -> StoredProfile {
    let parsed = parse_profile(&text);
    let metadata = profile_metadata(&text);
    let id = parsed
        .profile
        .as_ref()
        .map(|profile| profile.id)
        .or_else(|| metadata.as_ref().and_then(|(id, _)| *id));
    let name = parsed
        .profile
        .as_ref()
        .map(|profile| profile.name.clone())
        .or_else(|| metadata.and_then(|(_, name)| name))
        .unwrap_or_else(|| filename_stem(&path));
    StoredProfile {
        id,
        path,
        name,
        text,
        valid: parsed.profile.is_some(),
        diagnostics: parsed.diagnostics,
    }
}

fn profile_metadata(text: &str) -> Option<(Option<ProfileId>, Option<String>)> {
    let mut id = None;
    let mut name = None;
    for line in text.lines().take(20) {
        let trimmed = line.trim().trim_start_matches('#').trim();
        if let Some(value) = trimmed.strip_prefix("@profile-id ") {
            id = value.parse().ok();
        } else if let Some(value) = trimmed.strip_prefix("@profile-name ") {
            name = Some(value.trim().to_owned()).filter(|value| !value.is_empty());
        }
    }
    (id.is_some() || name.is_some()).then_some((id, name))
}

fn preset_metadata(text: &str) -> Result<(PresetId, u16), RepositoryError> {
    let mut id = None;
    let mut version = None;
    for line in text.lines().take(20) {
        let trimmed = line.trim().trim_start_matches('#').trim();
        if let Some(value) = trimmed.strip_prefix("@preset-id ") {
            id = Some(value.parse().map_err(repository_error)?);
        } else if let Some(value) = trimmed.strip_prefix("@preset-version ") {
            version = Some(value.parse::<u16>().map_err(repository_error)?);
        }
    }
    match (id, version) {
        (Some(id), Some(version)) if version > 0 => Ok((id, version)),
        _ => Err(RepositoryError::new(
            "preset requires @preset-id and positive @preset-version",
        )),
    }
}

fn validate_filename(filename: &str, extension: &str) -> Result<(), RepositoryError> {
    if filename.is_empty()
        || filename.contains(['/', '\\'])
        || Path::new(filename).components().count() != 1
        || Path::new(filename)
            .extension()
            .is_none_or(|value| value != extension)
    {
        return Err(RepositoryError::new(format!(
            "unsafe `{filename}` repository filename"
        )));
    }
    Ok(())
}

fn previous_good_path(path: &Path) -> PathBuf {
    path.with_extension("packignore.previous-good")
}

fn filename_stem(path: &Path) -> String {
    path.file_stem().map_or_else(
        || "Invalid profile".into(),
        |name| name.to_string_lossy().into_owned(),
    )
}

fn read_optional(path: &Path) -> Result<Option<String>, RepositoryError> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(Some(contents)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(repository_error(error)),
    }
}

fn atomic_write(path: &Path, contents: &[u8]) -> Result<(), RepositoryError> {
    let parent = path
        .parent()
        .ok_or_else(|| RepositoryError::new("repository path has no parent"))?;
    fs::create_dir_all(parent).map_err(repository_error)?;
    AtomicFile::new(path, AllowOverwrite)
        .write(|file| -> io::Result<()> {
            io::Write::write_all(file, contents)?;
            file.sync_all()
        })
        .map_err(repository_error)
}

fn repository_error(error: impl std::fmt::Display) -> RepositoryError {
    RepositoryError::new(error.to_string())
}
