#![forbid(unsafe_code)]

mod archive_runner;
mod database;
mod directories;
mod manifest;
mod migration;
mod presets;
mod reconciliation;
mod repositories;
mod yaml;

pub use archive_runner::ArchiveRunExecutor;
pub use database::SqliteRepository;
pub use directories::{AppDirectories, DirectoryError, DirectoryOverrides};
pub use manifest::{
    ManifestCursor, ManifestEntryReader, ManifestError, ManifestHandle, ManifestPage,
    ManifestWriter, ScanManifestError, scan_to_manifest, temporary_manifest_directory,
};
pub use migration::{DocumentKind, MigrationRegistry, MigrationStep};
pub use presets::{ResourcePresetError, load_preset_catalog};
pub use reconciliation::{
    ArtifactCleanupReport, ProcessProbe, StartupReconciliationReport, SystemProcessProbe,
    clean_stale_manifests, clean_stale_output_artifacts, reconcile_startup,
};
pub use repositories::{
    FileActivePlanRepository, FilePresetRepository, FileProfileRepository, FileSettingsRepository,
    initialize_resource_copies, install_missing_resources,
};
pub use yaml::{DocumentError, decode_plan, decode_settings, encode_plan, encode_settings};

/// Confirms that storage adapters can access the application contract.
#[must_use]
pub const fn application_status() -> &'static str {
    foldry_application::workspace_status()
}

#[cfg(test)]
mod tests {
    use super::application_status;

    #[test]
    fn depends_on_application_ports_only() {
        assert_eq!(application_status(), "Workspace ready");
    }
}
