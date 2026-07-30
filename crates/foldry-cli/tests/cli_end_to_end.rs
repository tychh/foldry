use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    thread,
    time::Duration,
};

use serde_json::Value;

#[test]
fn help_exposes_current_folder_action_command_groups() {
    let output = Command::new(binary()).arg("--help").output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(output.status.success());
    for command in [
        "profile", "preset", "folder", "action", "preview", "archive", "run", "history", "config",
    ] {
        assert!(stdout.contains(command), "{stdout}");
    }
    assert!(stdout.contains("foldry archive ./project"), "{stdout}");

    let archive = Command::new(binary())
        .args(["archive", "--help"])
        .output()
        .unwrap();
    let archive_help = String::from_utf8(archive.stdout).unwrap();
    assert!(archive_help.contains("Output directory"), "{archive_help}");
    assert!(
        archive_help.contains("exact profile name"),
        "{archive_help}"
    );
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
fn profile_folder_action_preview_run_and_history_work_end_to_end() {
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

    let added = run(
        root.path(),
        &[
            "--json",
            "folder",
            "add",
            source.to_str().unwrap(),
            "--profile",
            profile_id,
        ],
    );
    assert!(added.status.success(), "{}", stderr(&added));
    let added_json: Value = serde_json::from_slice(&added.stdout).unwrap();
    let folder_id = added_json["data"]["id"].as_str().unwrap();
    let action_id = added_json["data"]["actions"][0]["id"].as_str().unwrap();

    let preview = run(root.path(), &["--json", "preview", folder_id, action_id]);
    assert!(preview.status.success(), "{}", stderr(&preview));
    let preview_json: Value = serde_json::from_slice(&preview.stdout).unwrap();
    assert_eq!(preview_json["data"]["folder_id"], folder_id);
    assert_eq!(preview_json["data"]["action_id"], action_id);
    assert_eq!(preview_json["data"]["summary"]["included_files"], 1);
    assert_eq!(preview_json["data"]["summary"]["excluded_entries"], 1);

    let mut action = added_json["data"]["actions"][0].clone();
    action["enabled"] = Value::Bool(true);
    action["spec"]["output"]["directory"] =
        serde_json::json!({"mode": "custom", "path": output_directory});
    action["spec"]["output"]["filename"] = Value::String("result".into());
    action["spec"]["output"]["format"] = Value::String("tar_zst".into());
    action["spec"]["verification"]["mode"] = Value::String("full".into());
    action["spec"]["verification"]["checksum"] = Value::String("sha256".into());
    let action_file = root.path().join("action.json");
    fs::write(&action_file, serde_json::to_vec_pretty(&action).unwrap()).unwrap();
    let updated = run(
        root.path(),
        &[
            "action",
            "update",
            folder_id,
            action_id,
            "--from",
            action_file.to_str().unwrap(),
        ],
    );
    assert!(updated.status.success(), "{}", stderr(&updated));

    let executed = run(
        root.path(),
        &["--json", "action", "run", folder_id, action_id],
    );
    assert!(executed.status.success(), "{}", stderr(&executed));
    let executed_json: Value = serde_json::from_slice(&executed.stdout).unwrap();
    let artifact = executed_json["data"][0]["artifact"]["path"]
        .as_str()
        .unwrap();
    assert!(Path::new(artifact).is_file());
    assert!(
        executed_json["data"][0]["artifact"]["checksum_sha256"]
            .as_str()
            .is_some_and(|checksum| checksum.len() == 64)
    );

    let history = run(
        root.path(),
        &[
            "--json", "history", "list", "--folder", folder_id, "--action", action_id, "--limit",
            "5",
        ],
    );
    assert!(history.status.success(), "{}", stderr(&history));
    let history_json: Value = serde_json::from_slice(&history.stdout).unwrap();
    assert_eq!(history_json["data"].as_array().unwrap().len(), 1);
    assert_eq!(history_json["data"][0]["state"], "succeeded");
    let run_id = history_json["data"][0]["run_id"].as_str().unwrap();
    let logs = run(
        root.path(),
        &["--json", "history", "logs", run_id, "--limit", "20"],
    );
    let logs_json: Value = serde_json::from_slice(&logs.stdout).unwrap();
    assert!(!logs_json["data"].as_array().unwrap().is_empty());

    let unlisted = run(root.path(), &["folder", "unlist", folder_id]);
    assert!(unlisted.status.success(), "{}", stderr(&unlisted));
    let remembered = run(root.path(), &["--json", "folder", "remembered"]);
    let remembered_json: Value = serde_json::from_slice(&remembered.stdout).unwrap();
    assert_eq!(remembered_json["data"][0]["id"], folder_id);
    let forgotten = run(root.path(), &["folder", "forget", folder_id]);
    assert!(forgotten.status.success(), "{}", stderr(&forgotten));

    let repeat = run(root.path(), &["--json", "run", "repeat", run_id]);
    assert!(repeat.status.success(), "{}", stderr(&repeat));
}

#[test]
fn one_shot_archive_does_not_create_a_remembered_folder() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let output = root.path().join("output");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&output).unwrap();
    fs::write(source.join("file.txt"), "content").unwrap();

    let archived = run(
        root.path(),
        &[
            "archive",
            source.to_str().unwrap(),
            "--profile",
            "Default",
            "--output",
            output.to_str().unwrap(),
            "--name",
            "one-shot",
        ],
    );
    assert!(archived.status.success(), "{}", stderr(&archived));

    let folders = run(root.path(), &["--json", "folder", "list"]);
    let json: Value = serde_json::from_slice(&folders.stdout).unwrap();
    assert_eq!(json["data"], serde_json::json!([]));
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
