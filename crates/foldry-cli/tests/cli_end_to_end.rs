use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    thread,
    time::Duration,
};

use serde_json::Value;

#[test]
fn help_exposes_the_stable_v1_command_groups() {
    let output = Command::new(binary()).arg("--help").output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(output.status.success());
    for command in [
        "profile", "preset", "preview", "archive", "plan", "history", "config",
    ] {
        assert!(stdout.contains(command), "{stdout}");
    }
}

#[test]
fn json_mode_has_a_versioned_envelope_and_validation_exit_code() {
    let root = tempfile::tempdir().unwrap();
    let output = run(root.path(), &["--json", "config", "path"]);
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();

    assert!(output.status.success());
    assert_eq!(json["version"], 1);
    assert_eq!(json["ok"], true);
    assert!(
        json["data"]["settings"]
            .as_str()
            .unwrap()
            .ends_with("settings.yaml")
    );

    let invalid = root.path().join("invalid.packignore");
    fs::write(&invalid, "*.tmp\n").unwrap();
    let output = run(
        root.path(),
        &["--json", "profile", "validate", invalid.to_str().unwrap()],
    );
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(json["data"]["valid"], false);
}

#[test]
fn profile_preview_archive_and_history_work_end_to_end() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let output_directory = root.path().join("output");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&output_directory).unwrap();
    fs::write(source.join("keep.txt"), "hello").unwrap();
    fs::write(source.join("ignored.tmp"), "temporary").unwrap();

    let created = run(
        root.path(),
        &[
            "--json",
            "profile",
            "create",
            "--name",
            "CLI profile",
            "--filename",
            "cli.packignore",
        ],
    );
    assert!(created.status.success(), "{}", stderr(&created));
    let created_json: Value = serde_json::from_slice(&created.stdout).unwrap();
    let profile_id = created_json["data"]["id"].as_str().unwrap();
    let edited = root.path().join("edited.packignore");
    fs::write(
        &edited,
        format!(
            "# @profile-id {profile_id}\n# @profile-version 1\n\
             # @profile-name CLI profile\n*.tmp\n"
        ),
    )
    .unwrap();
    let edited_output = run(
        root.path(),
        &[
            "profile",
            "edit",
            profile_id,
            "--from",
            edited.to_str().unwrap(),
        ],
    );
    assert!(edited_output.status.success(), "{}", stderr(&edited_output));

    let preview = run(
        root.path(),
        &[
            "--json",
            "preview",
            source.to_str().unwrap(),
            "--profile",
            profile_id,
        ],
    );
    assert!(preview.status.success(), "{}", stderr(&preview));
    let preview_json: Value = serde_json::from_slice(&preview.stdout).unwrap();
    assert_eq!(preview_json["data"]["included_files"], 1);
    assert_eq!(preview_json["data"]["excluded_entries"], 1);

    let archive = run(
        root.path(),
        &[
            "--json",
            "archive",
            source.to_str().unwrap(),
            "--profile",
            profile_id,
            "--output",
            output_directory.to_str().unwrap(),
            "--name",
            "result",
            "--format",
            "tar-zst",
            "--checksum",
            "--full-verify",
        ],
    );
    assert!(archive.status.success(), "{}", stderr(&archive));
    let archive_json: Value = serde_json::from_slice(&archive.stdout).unwrap();
    let artifact = archive_json["data"][0]["artifact"]["path"]
        .as_str()
        .unwrap();
    assert!(Path::new(artifact).is_file());
    assert!(
        archive_json["data"][0]["artifact"]["checksum_sha256"]
            .as_str()
            .is_some_and(|checksum| checksum.len() == 64)
    );

    let history = run(root.path(), &["--json", "history", "list", "--limit", "5"]);
    assert!(history.status.success(), "{}", stderr(&history));
    let history_json: Value = serde_json::from_slice(&history.stdout).unwrap();
    assert_eq!(history_json["data"].as_array().unwrap().len(), 1);
    assert_eq!(history_json["data"][0]["state"], "succeeded");
}

#[cfg(unix)]
#[test]
fn sigint_uses_cooperative_stop_and_leaves_no_final_archive() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let output_directory = root.path().join("output");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&output_directory).unwrap();
    fs::File::create(source.join("large.bin"))
        .unwrap()
        .set_len(1024 * 1024 * 1024)
        .unwrap();
    let mut child = command(root.path())
        .args([
            "archive",
            source.to_str().unwrap(),
            "--output",
            output_directory.to_str().unwrap(),
            "--name",
            "cancelled",
            "--format",
            "zip",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline
        && !fs::read_dir(&output_directory).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".foldry-reserve")
        })
    {
        thread::sleep(Duration::from_millis(25));
    }
    let status = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(status.success());

    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(130));
    assert!(!output_directory.join("cancelled.zip").exists());
    assert!(fs::read_dir(&output_directory).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".part")
    }));
}

fn run(root: &Path, args: &[&str]) -> Output {
    command(root).args(args).output().unwrap()
}

fn command(root: &Path) -> Command {
    let mut command = Command::new(binary());
    command.args([
        "--config-dir",
        root.join("config").to_str().unwrap(),
        "--data-dir",
        root.join("data").to_str().unwrap(),
        "--cache-dir",
        root.join("cache").to_str().unwrap(),
    ]);
    command
}

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_foldry"))
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}
