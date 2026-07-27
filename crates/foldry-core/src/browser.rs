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
    Favorite,
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
}

#[derive(Debug)]
pub enum BrowserError {
    Cancelled,
    ReadDirectory { path: PathBuf, source: io::Error },
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
        }
    }
}

impl std::error::Error for BrowserError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ReadDirectory { source, .. } => Some(source),
            Self::Cancelled => None,
        }
    }
}

pub struct FileSystemBrowser;

impl FileSystemBrowser {
    /// Returns roots without scanning any of their descendants.
    #[must_use]
    pub fn roots(home: Option<&Path>, favorites: &[PathBuf]) -> Vec<BrowserRoot> {
        let mut roots = Vec::new();
        if let Some(home) = home {
            roots.push(root(home, BrowserRootKind::Home));
        }
        for path in filesystem_roots() {
            if !roots.iter().any(|root| root.path == path) {
                roots.push(root(&path, BrowserRootKind::FileSystem));
            }
        }
        for favorite in favorites {
            if !roots.iter().any(|root| root.path == *favorite) {
                roots.push(root(favorite, BrowserRootKind::Favorite));
            }
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
            left.name
                .to_lowercase()
                .cmp(&right.name.to_lowercase())
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(nodes)
    }
}

fn root(path: &Path, kind: BrowserRootKind) -> BrowserRoot {
    BrowserRoot {
        id: stable_node_id(path),
        path: path.to_path_buf(),
        name: display_name(path),
        kind,
    }
}

fn browser_node(
    path: &Path,
    parent_metadata: Option<&fs::Metadata>,
    mounts: &MountTable,
) -> BrowserNode {
    let metadata = fs::symlink_metadata(path);
    let (kind, available, is_mount_point) = match metadata.as_ref() {
        Ok(metadata) => {
            let kind = classify(path, metadata);
            let mount = kind == FileSystemObjectKind::Directory
                && (mounts.is_mount(path) || is_different_device(parent_metadata, metadata));
            (kind, directory_is_available(path, kind), mount)
        }
        Err(_) => (FileSystemObjectKind::Unreadable, false, false),
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
    }
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
fn filesystem_roots() -> Vec<PathBuf> {
    (b'A'..=b'Z')
        .map(|letter| PathBuf::from(format!("{}:\\", char::from(letter))))
        .filter(|path| path.exists())
        .collect()
}

#[cfg(target_os = "macos")]
fn filesystem_roots() -> Vec<PathBuf> {
    let mut roots = vec![PathBuf::from("/")];
    if let Ok(volumes) = fs::read_dir("/Volumes") {
        roots.extend(
            volumes
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.is_dir()),
        );
    }
    roots
}

#[cfg(not(any(windows, target_os = "macos")))]
fn filesystem_roots() -> Vec<PathBuf> {
    vec![PathBuf::from("/")]
}
