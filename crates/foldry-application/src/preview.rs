use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use foldry_core::{ActionId, FolderId, ScanSummary};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::ActionSpec;

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct PreviewCacheKey {
    pub folder_id: FolderId,
    pub action_id: ActionId,
    pub profile_hash: String,
    pub source_metadata_hash: String,
    pub action_hash: String,
}

impl PreviewCacheKey {
    pub fn build(
        folder_id: FolderId,
        action_id: ActionId,
        profile_text: &str,
        source: &Path,
        action: &ActionSpec,
    ) -> Result<Self, PreviewKeyError> {
        let action_json = serde_json::to_vec(action)
            .map_err(|error| PreviewKeyError::SerializeAction(error.to_string()))?;
        Ok(Self {
            folder_id,
            action_id,
            profile_hash: hash_bytes(profile_text.as_bytes()),
            source_metadata_hash: source_metadata_hash(source)?,
            action_hash: hash_bytes(&action_json),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreviewKeyError {
    SourceMetadata { path: PathBuf, message: String },
    SerializeAction(String),
}

impl std::fmt::Display for PreviewKeyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SourceMetadata { path, message } => {
                write!(
                    formatter,
                    "cannot fingerprint source {}: {message}",
                    path.display()
                )
            }
            Self::SerializeAction(message) => {
                write!(formatter, "cannot fingerprint action: {message}")
            }
        }
    }
}

impl std::error::Error for PreviewKeyError {}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewFilter {
    #[default]
    All,
    Included,
    Excluded,
    Skipped,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PreviewSnapshot {
    pub cache_key: PreviewCacheKey,
    pub manifest_id: String,
    pub created_at: String,
    pub summary: ScanSummary,
}

impl PreviewSnapshot {
    #[must_use]
    pub fn new(cache_key: PreviewCacheKey, manifest_id: String, summary: ScanSummary) -> Self {
        Self {
            cache_key,
            manifest_id,
            created_at: jiff::Timestamp::now().to_string(),
            summary,
        }
    }
}

/// One bounded descriptor per action; manifest entries remain on disk.
#[derive(Default)]
pub struct PreviewCache {
    snapshots: HashMap<(FolderId, ActionId), PreviewSnapshot>,
}

impl PreviewCache {
    #[must_use]
    pub fn get(&self, key: &PreviewCacheKey) -> Option<&PreviewSnapshot> {
        self.snapshots
            .get(&(key.folder_id, key.action_id))
            .filter(|snapshot| snapshot.cache_key == *key)
    }

    pub fn insert(&mut self, snapshot: PreviewSnapshot) -> Option<PreviewSnapshot> {
        self.snapshots.insert(
            (snapshot.cache_key.folder_id, snapshot.cache_key.action_id),
            snapshot,
        )
    }

    pub fn invalidate_action(
        &mut self,
        folder_id: FolderId,
        action_id: ActionId,
    ) -> Option<PreviewSnapshot> {
        self.snapshots.remove(&(folder_id, action_id))
    }

    pub fn invalidate_folder(&mut self, folder_id: FolderId) -> Vec<PreviewSnapshot> {
        let keys = self
            .snapshots
            .keys()
            .filter(|(candidate, _)| *candidate == folder_id)
            .copied()
            .collect::<Vec<_>>();
        keys.into_iter()
            .filter_map(|key| self.snapshots.remove(&key))
            .collect()
    }

    pub fn invalidate_all(&mut self) -> Vec<PreviewSnapshot> {
        self.snapshots
            .drain()
            .map(|(_, snapshot)| snapshot)
            .collect()
    }
}

fn source_metadata_hash(source: &Path) -> Result<String, PreviewKeyError> {
    let metadata = fs::metadata(source).map_err(|error| PreviewKeyError::SourceMetadata {
        path: source.to_path_buf(),
        message: error.to_string(),
    })?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_nanos());
    let mut hasher = Sha256::new();
    hasher.update(source.to_string_lossy().as_bytes());
    hasher.update(metadata.len().to_le_bytes());
    hasher.update(modified.to_le_bytes());
    hasher.update([u8::from(metadata.is_dir()), u8::from(metadata.is_file())]);
    Ok(format!("{:x}", hasher.finalize()))
}

fn hash_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, fs};

    use foldry_core::{
        ActionId, ActionVersion, ArchiveActionSpec, ArchiveFormat, ArchiveOutputDirectory,
        ArchiveOutputSpec, ChecksumAlgorithm, CompressionLevel, ConflictPolicy, FolderId,
        UnreadablePolicy, VerificationMode, VerificationSpec,
    };

    use super::{PreviewCache, PreviewCacheKey, PreviewSnapshot};
    use crate::ActionSpec;

    fn action(include_root: bool) -> ActionSpec {
        ActionSpec::Archive(ArchiveActionSpec {
            version: ActionVersion::V1,
            output: ArchiveOutputSpec {
                directory: ArchiveOutputDirectory::Parent,
                filename: "archive".into(),
                format: ArchiveFormat::Zip,
                compression: CompressionLevel::Balanced,
                conflict_policy: ConflictPolicy::Increment,
                extensions: BTreeMap::new(),
            },
            include_root,
            unreadable_policy: UnreadablePolicy::Fail,
            verification: VerificationSpec {
                mode: VerificationMode::Structural,
                checksum: ChecksumAlgorithm::None,
                extensions: BTreeMap::new(),
            },
            extensions: BTreeMap::new(),
        })
    }

    #[test]
    fn key_changes_with_profile_source_metadata_and_action() {
        let directory = tempfile::tempdir().expect("temp directory");
        let source = directory.path().join("source");
        fs::write(&source, "initial").expect("write initial source");
        let folder_id = FolderId::new();
        let action_id = ActionId::new();
        let first =
            PreviewCacheKey::build(folder_id, action_id, "profile-a", &source, &action(true))
                .expect("first key");
        let changed_profile =
            PreviewCacheKey::build(folder_id, action_id, "profile-b", &source, &action(true))
                .expect("profile");
        let changed_action =
            PreviewCacheKey::build(folder_id, action_id, "profile-a", &source, &action(false))
                .expect("action");
        fs::write(&source, "changed source metadata").expect("change source");
        let changed_source =
            PreviewCacheKey::build(folder_id, action_id, "profile-a", &source, &action(true))
                .expect("source");

        assert_ne!(first, changed_profile);
        assert_ne!(first, changed_action);
        assert_ne!(first, changed_source);
    }

    #[test]
    fn cache_requires_an_exact_key_and_supports_explicit_invalidation() {
        let directory = tempfile::tempdir().expect("temp directory");
        let folder_id = FolderId::new();
        let action_id = ActionId::new();
        let key = PreviewCacheKey::build(
            folder_id,
            action_id,
            "profile-a",
            directory.path(),
            &action(true),
        )
        .expect("key");
        let snapshot = PreviewSnapshot {
            cache_key: key.clone(),
            manifest_id: "manifest".to_owned(),
            created_at: "2026-07-27T00:00:00Z".to_owned(),
            summary: Default::default(),
        };
        let mut cache = PreviewCache::default();
        cache.insert(snapshot);

        assert!(cache.get(&key).is_some());
        assert!(
            cache
                .get(&PreviewCacheKey {
                    profile_hash: "changed".to_owned(),
                    ..key.clone()
                })
                .is_none()
        );
        assert!(cache.invalidate_action(folder_id, action_id).is_some());
        assert!(cache.get(&key).is_none());
    }
}
