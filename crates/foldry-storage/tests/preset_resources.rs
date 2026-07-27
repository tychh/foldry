use std::{fs, path::PathBuf};

use foldry_storage::load_preset_catalog;

#[test]
fn shipped_catalog_is_complete_valid_and_explicitly_classified() {
    let resources = resource_directory();
    let catalog = load_preset_catalog(&resources).unwrap();
    let definitions = catalog.iter().collect::<Vec<_>>();

    assert_eq!(definitions.len(), 30);
    assert_eq!(
        definitions
            .iter()
            .filter(|definition| definition.sensitive)
            .count(),
        6
    );
    for required in [
        "python",
        "nodejs",
        "rust",
        "go",
        "java",
        "dotnet",
        "php",
        "ruby",
        "cpp",
        "cmake",
        "django",
        "react-vite",
        "nextjs",
        "vue",
        "jetbrains",
        "vscode",
        "macos",
        "windows",
        "linux",
        "test-artifacts",
        "coverage",
        "build-output",
        "environment-secrets",
        "certificates-keys",
        "database-dumps",
        "private-media",
        "deployment-credentials",
    ] {
        assert!(
            catalog.get(&required.parse().unwrap()).is_some(),
            "{required}"
        );
    }
}

#[test]
fn default_profile_never_auto_installs_sensitive_presets() {
    let catalog = load_preset_catalog(&resource_directory()).unwrap();
    let default_profile = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../resources/profiles/default.packignore"),
    )
    .unwrap();

    for definition in catalog.iter().filter(|definition| definition.sensitive) {
        assert!(
            !default_profile.contains(&format!("@preset-begin id={}", definition.id)),
            "{}",
            definition.id
        );
    }
}

fn resource_directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../resources/presets")
}
