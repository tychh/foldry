use std::{
    fs, io,
    path::{Path, PathBuf},
};

use directories::ProjectDirs;
use thiserror::Error;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DirectoryOverrides {
    pub config: Option<PathBuf>,
    pub data: Option<PathBuf>,
    pub cache: Option<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppDirectories {
    pub config: PathBuf,
    pub data: PathBuf,
    pub cache: PathBuf,
}

#[derive(Debug, Error)]
pub enum DirectoryError {
    #[error("platform directories are unavailable")]
    Unavailable,
    #[error("cannot create application directory {path}: {source}")]
    Create { path: PathBuf, source: io::Error },
}

impl AppDirectories {
    pub fn resolve(overrides: &DirectoryOverrides) -> Result<Self, DirectoryError> {
        let platform = if overrides.config.is_none()
            || overrides.data.is_none()
            || overrides.cache.is_none()
        {
            Some(project_directories().ok_or(DirectoryError::Unavailable)?)
        } else {
            None
        };
        Ok(Self {
            config: overrides.config.clone().unwrap_or_else(|| {
                platform
                    .as_ref()
                    .expect("platform config")
                    .config_dir()
                    .to_path_buf()
            }),
            data: overrides.data.clone().unwrap_or_else(|| {
                platform
                    .as_ref()
                    .expect("platform data")
                    .data_local_dir()
                    .to_path_buf()
            }),
            cache: overrides.cache.clone().unwrap_or_else(|| {
                platform
                    .as_ref()
                    .expect("platform cache")
                    .cache_dir()
                    .to_path_buf()
            }),
        })
    }

    pub fn ensure_layout(&self) -> Result<(), DirectoryError> {
        for path in [
            &self.config,
            &self.data,
            &self.cache,
            &self.profiles(),
            &self.presets(),
            &self.crash_reports(),
            &self.manifests(),
        ] {
            fs::create_dir_all(path).map_err(|source| DirectoryError::Create {
                path: path.to_path_buf(),
                source,
            })?;
        }
        Ok(())
    }

    #[must_use]
    pub fn settings(&self) -> PathBuf {
        self.config.join("settings.yaml")
    }

    #[must_use]
    pub fn active_plan(&self) -> PathBuf {
        self.config.join("active.packplan.yaml")
    }

    #[must_use]
    pub fn profiles(&self) -> PathBuf {
        self.config.join("profiles")
    }

    #[must_use]
    pub fn presets(&self) -> PathBuf {
        self.config.join("presets")
    }

    #[must_use]
    pub fn database(&self) -> PathBuf {
        self.data.join("app.db")
    }

    #[must_use]
    pub fn crash_reports(&self) -> PathBuf {
        self.data.join("crash-reports")
    }

    #[must_use]
    pub fn manifests(&self) -> PathBuf {
        self.cache.join("manifests")
    }

    #[must_use]
    pub fn contains_owned_cache_path(&self, path: &Path) -> bool {
        path.starts_with(self.manifests())
    }
}

#[cfg(target_os = "macos")]
fn project_directories() -> Option<ProjectDirs> {
    ProjectDirs::from("app", "foldry", "desktop")
}

#[cfg(not(target_os = "macos"))]
fn project_directories() -> Option<ProjectDirs> {
    ProjectDirs::from("", "", "Foldry")
}
