use std::{
    fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    CancellationToken, FileSystemObjectKind,
    filesystem::{MountTable, stable_node_id},
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserRootKind {
    Home,
    FileSystem,
    Documents,
    Desktop,
    Downloads,
    Volumes,
    SystemPath,
    Drive,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserRoot {
    pub id: String,
    pub path: PathBuf,
    pub name: String,
    pub kind: BrowserRootKind,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserNode {
    pub id: String,
    pub path: PathBuf,
    pub name: String,
    pub kind: FileSystemObjectKind,
    pub is_mount_point: bool,
    pub is_network_mount: bool,
    pub is_platform_special: bool,
    pub available: bool,
    pub modified_at_unix_ms: Option<u128>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserSize {
    pub logical_bytes: u64,
    pub partial: bool,
    pub warnings: u64,
}

#[derive(Debug)]
pub enum BrowserError {
    Cancelled,
    ReadDirectory { path: PathBuf, source: io::Error },
    NotDirectory { path: PathBuf },
}

impl std::fmt::Display for BrowserError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("browser request was cancelled"),
            Self::ReadDirectory { path, source } => {
                write!(
                    formatter,
                    "cannot read directory {}: {source}",
                    path.display()
                )
            }
            Self::NotDirectory { path } => {
                write!(formatter, "{} is not a directory", path.display())
            }
        }
    }
}

impl std::error::Error for BrowserError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ReadDirectory { source, .. } => Some(source),
            Self::Cancelled | Self::NotDirectory { .. } => None,
        }
    }
}

pub struct FileSystemBrowser;

impl FileSystemBrowser {
    /// Returns roots without scanning any of their descendants.
    #[must_use]
    pub fn roots(home: Option<&Path>) -> Vec<BrowserRoot> {
        let mut roots = Vec::new();
        if let Some(home) = home {
            push_root(&mut roots, home, "Home", BrowserRootKind::Home);
            for (name, kind) in [
                ("Documents", BrowserRootKind::Documents),
                ("Desktop", BrowserRootKind::Desktop),
                ("Downloads", BrowserRootKind::Downloads),
            ] {
                let path = home.join(name);
                if path.is_dir() {
                    push_root(&mut roots, &path, name, kind);
                }
            }
        }
        for (path, name, kind) in platform_locations() {
            push_root(&mut roots, &path, &name, kind);
        }
        roots
    }

    /// Loads and sorts only direct children of `directory`.
    pub fn direct_children(
        directory: &Path,
        cancellation: &CancellationToken,
    ) -> Result<Vec<BrowserNode>, BrowserError> {
        if cancellation.is_cancelled() {
            return Err(BrowserError::Cancelled);
        }
        let entries = fs::read_dir(directory).map_err(|source| BrowserError::ReadDirectory {
            path: directory.to_path_buf(),
            source,
        })?;
        let parent_metadata = fs::metadata(directory).ok();
        let mounts = MountTable::load();
        let mut nodes = Vec::new();

        for result in entries {
            if cancellation.is_cancelled() {
                return Err(BrowserError::Cancelled);
            }
            match result {
                Ok(entry) => {
                    let path = entry.path();
                    nodes.push(browser_node(&path, parent_metadata.as_ref(), &mounts));
                }
                Err(_) => {
                    // An unreadable directory entry has no stable path to expose.
                }
            }
        }
        nodes.sort_by(|left, right| {
            object_sort_rank(left.kind)
                .cmp(&object_sort_rank(right.kind))
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(nodes)
    }

    #[must_use]
    pub fn node(path: &Path) -> BrowserNode {
        let parent_metadata = path.parent().and_then(|parent| fs::metadata(parent).ok());
        browser_node(path, parent_metadata.as_ref(), &MountTable::load())
    }

    pub fn directory_size(
        directory: &Path,
        cancellation: &CancellationToken,
    ) -> Result<BrowserSize, BrowserError> {
        if !directory.is_dir() {
            return Err(BrowserError::NotDirectory {
                path: directory.to_path_buf(),
            });
        }
        let mut result = BrowserSize {
            logical_bytes: 0,
            partial: false,
            warnings: 0,
        };
        let root_metadata =
            fs::metadata(directory).map_err(|source| BrowserError::ReadDirectory {
                path: directory.to_path_buf(),
                source,
            })?;
        let mounts = MountTable::load();
        let mut pending = vec![(directory.to_path_buf(), root_metadata)];
        while let Some((current, current_metadata)) = pending.pop() {
            if cancellation.is_cancelled() {
                return Err(BrowserError::Cancelled);
            }
            let entries = match fs::read_dir(&current) {
                Ok(entries) => entries,
                Err(_) => {
                    result.partial = true;
                    result.warnings = result.warnings.saturating_add(1);
                    continue;
                }
            };
            for entry in entries {
                if cancellation.is_cancelled() {
                    return Err(BrowserError::Cancelled);
                }
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(_) => {
                        result.partial = true;
                        result.warnings = result.warnings.saturating_add(1);
                        continue;
                    }
                };
                let metadata = match fs::symlink_metadata(entry.path()) {
                    Ok(metadata) => metadata,
                    Err(_) => {
                        result.partial = true;
                        result.warnings = result.warnings.saturating_add(1);
                        continue;
                    }
                };
                if is_link_or_reparse(&metadata) {
                    continue;
                }
                if metadata.is_dir() {
                    let path = entry.path();
                    if !mounts.is_mount(&path)
                        && !is_different_device(Some(&current_metadata), &metadata)
                    {
                        pending.push((path, metadata));
                    }
                } else if metadata.is_file() {
                    result.logical_bytes = result.logical_bytes.saturating_add(metadata.len());
                }
            }
        }
        Ok(result)
    }
}

fn push_root(roots: &mut Vec<BrowserRoot>, path: &Path, name: &str, kind: BrowserRootKind) {
    if roots.iter().any(|root| root.path == path) {
        return;
    }
    roots.push(BrowserRoot {
        id: stable_node_id(path),
        path: path.to_path_buf(),
        name: name.to_owned(),
        kind,
    });
}

fn browser_node(
    path: &Path,
    parent_metadata: Option<&fs::Metadata>,
    mounts: &MountTable,
) -> BrowserNode {
    let metadata = fs::symlink_metadata(path);
    let (kind, available, is_mount_point, modified_at_unix_ms) = match metadata.as_ref() {
        Ok(metadata) => {
            let kind = classify(path, metadata);
            let mount = kind == FileSystemObjectKind::Directory
                && (mounts.is_mount(path) || is_different_device(parent_metadata, metadata));
            (
                kind,
                directory_is_available(path, kind),
                mount,
                metadata
                    .modified()
                    .ok()
                    .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|duration| duration.as_millis()),
            )
        }
        Err(_) => (FileSystemObjectKind::Unreadable, false, false, None),
    };
    BrowserNode {
        id: stable_node_id(path),
        path: path.to_path_buf(),
        name: display_name(path),
        kind,
        is_mount_point,
        is_network_mount: is_mount_point && mounts.is_network_mount(path),
        is_platform_special: is_platform_special(path),
        available: available && !is_platform_special(path),
        modified_at_unix_ms,
    }
}

fn object_sort_rank(kind: FileSystemObjectKind) -> u8 {
    if kind == FileSystemObjectKind::Directory {
        0
    } else {
        1
    }
}

#[cfg(windows)]
fn is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .filter(|name| !name.is_empty())
        .map_or_else(
            || path.to_string_lossy().into_owned(),
            |name| name.to_string_lossy().into_owned(),
        )
}

fn directory_is_available(path: &Path, kind: FileSystemObjectKind) -> bool {
    if kind != FileSystemObjectKind::Directory {
        return true;
    }
    fs::read_dir(path).is_ok()
}

#[cfg(unix)]
fn is_different_device(parent: Option<&fs::Metadata>, child: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    parent.is_some_and(|parent| parent.dev() != child.dev())
}

#[cfg(not(unix))]
fn is_different_device(_parent: Option<&fs::Metadata>, _child: &fs::Metadata) -> bool {
    false
}

#[cfg(windows)]
fn classify(_path: &Path, metadata: &fs::Metadata) -> FileSystemObjectKind {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    if metadata.file_type().is_symlink() {
        FileSystemObjectKind::Symlink
    } else if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        FileSystemObjectKind::JunctionOrReparsePoint
    } else if metadata.is_dir() {
        FileSystemObjectKind::Directory
    } else if metadata.is_file() {
        FileSystemObjectKind::RegularFile
    } else {
        FileSystemObjectKind::SpecialFile
    }
}

#[cfg(not(windows))]
fn classify(_path: &Path, metadata: &fs::Metadata) -> FileSystemObjectKind {
    if metadata.file_type().is_symlink() {
        FileSystemObjectKind::Symlink
    } else if metadata.is_dir() {
        FileSystemObjectKind::Directory
    } else if metadata.is_file() {
        FileSystemObjectKind::RegularFile
    } else {
        FileSystemObjectKind::SpecialFile
    }
}

#[cfg(target_os = "linux")]
fn is_platform_special(path: &Path) -> bool {
    [Path::new("/proc"), Path::new("/sys"), Path::new("/dev")]
        .iter()
        .any(|special| path == *special || path.starts_with(special))
}

#[cfg(not(target_os = "linux"))]
fn is_platform_special(_path: &Path) -> bool {
    false
}

#[cfg(windows)]
fn platform_locations() -> Vec<(PathBuf, String, BrowserRootKind)> {
    (b'A'..=b'Z')
        .map(|letter| PathBuf::from(format!("{}:\\", char::from(letter))))
        .filter(|path| path.exists())
        .map(|path| {
            let name = path.to_string_lossy().into_owned();
            (path, name, BrowserRootKind::Drive)
        })
        .collect()
}

#[cfg(target_os = "macos")]
fn platform_locations() -> Vec<(PathBuf, String, BrowserRootKind)> {
    vec![
        (
            PathBuf::from("/"),
            "/".to_owned(),
            BrowserRootKind::FileSystem,
        ),
        (
            PathBuf::from("/Volumes"),
            "Volumes".to_owned(),
            BrowserRootKind::Volumes,
        ),
    ]
}

#[cfg(not(any(windows, target_os = "macos")))]
fn platform_locations() -> Vec<(PathBuf, String, BrowserRootKind)> {
    [
        ("/", "/"),
        ("/opt", "opt"),
        ("/srv", "srv"),
        ("/mnt", "mnt"),
        ("/media", "media"),
    ]
    .into_iter()
    .filter(|(path, _)| Path::new(path).is_dir())
    .map(|(path, name)| {
        (
            PathBuf::from(path),
            name.to_owned(),
            if path == "/" {
                BrowserRootKind::FileSystem
            } else {
                BrowserRootKind::SystemPath
            },
        )
    })
    .collect()
}
