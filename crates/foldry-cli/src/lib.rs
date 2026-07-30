#![forbid(unsafe_code)]

use std::{
    collections::HashMap,
    ffi::OsString,
    fs,
    io::{self, IsTerminal},
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use clap::{Args, Parser, Subcommand, ValueEnum};
use foldry_application::{
    ActionId, ActionSpec, ActionVersion, ActivePlanRepository, ApplicationPorts,
    ApplicationServices, ArchiveActionSpec, ArchiveFormat, ArchiveOutputDirectory,
    ArchiveOutputSpec, ChecksumAlgorithm, Clock, CompiledProfile, CompressionLevel, ConflictPolicy,
    Extensions, Folder, FolderAction, FolderId, FolderSnapshot, LogRepository, PageRequest, Plan,
    PlanVersion, PresetId, ProfileId, ResultSummary, RunEvent, RunEventKind, RunEventSink,
    RunHistoryRepository, RunId, RunOutcome, RunRecord, RunSnapshot, RunState, Scheduler,
    SchedulerPorts, Settings, SystemClock, UuidIdGenerator, VerificationMode, VerificationSpec,
    detect_case_sensitivity, parse_profile,
};
use foldry_storage::{
    AppDirectories, ArchiveRunExecutor, DirectoryOverrides, FileActivePlanRepository,
    FilePresetRepository, FileProfileRepository, FileSettingsRepository, SqliteRepository,
    SystemProcessProbe, initialize_resource_copies, reconcile_startup, scan_to_manifest,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const EXIT_SUCCESS: i32 = 0;
const EXIT_USAGE: i32 = 2;
const EXIT_VALIDATION: i32 = 3;
const EXIT_IO: i32 = 4;
const EXIT_PARTIAL: i32 = 5;
const EXIT_CONFIG: i32 = 6;
const EXIT_INTERNAL: i32 = 10;
const EXIT_CANCELLED: i32 = 130;

#[derive(Debug, Parser)]
#[command(
    name = "foldry",
    version,
    about = "Prepare and archive folders with reusable Ignore Profiles",
    after_help = "Examples:
  foldry archive ./project --format tar-zst --checksum
  foldry folder add ./project
  foldry folder list
  foldry run all

Use `foldry <command> --help` for command-specific options."
)]
struct Cli {
    /// Emit one stable JSON result object.
    #[arg(long, global = true)]
    json: bool,
    /// Development/recovery config-directory override.
    #[arg(long, global = true, hide = true)]
    config_dir: Option<PathBuf>,
    /// Development/recovery data-directory override.
    #[arg(long, global = true, hide = true)]
    data_dir: Option<PathBuf>,
    /// Development/recovery cache-directory override.
    #[arg(long, global = true, hide = true)]
    cache_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Manage filtering profiles.
    Profile {
        #[command(subcommand)]
        command: ProfileCommand,
    },
    /// Manage packaged preset working copies.
    Preset {
        #[command(subcommand)]
        command: PresetCommand,
    },
    /// Manage remembered folders.
    Folder {
        #[command(subcommand)]
        command: FolderCommand,
    },
    /// Manage independent actions attached to folders.
    Action {
        #[command(subcommand)]
        command: ActionCommand,
    },
    /// Preview one configured folder action.
    Preview(PreviewArgs),
    /// Create one archive immediately.
    Archive(ArchiveArgs),
    /// Run configured folders and repeat immutable snapshots.
    Run {
        #[command(subcommand)]
        command: RunCommand,
    },
    /// Read persisted run history.
    History {
        #[command(subcommand)]
        command: HistoryCommand,
    },
    /// Show configuration values and paths.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

#[derive(Debug, Subcommand)]
enum ProfileCommand {
    /// List installed Ignore Profiles.
    List,
    /// Print one profile, including its rules.
    Show {
        /// Profile UUID.
        id: String,
    },
    /// Create an empty Ignore Profile.
    Create {
        /// Human-readable profile name.
        #[arg(long)]
        name: String,
        /// File name under the Foldry profiles directory.
        #[arg(long)]
        filename: Option<String>,
    },
    /// Replace a profile with the contents of a .packignore file.
    Edit {
        /// Profile UUID. The replacement must preserve this @profile-id.
        id: String,
        /// Path to the replacement .packignore file.
        #[arg(long = "from")]
        source: PathBuf,
    },
    /// Delete an Ignore Profile.
    Delete {
        /// Profile UUID.
        id: String,
    },
    /// Validate a .packignore file without installing it.
    Validate {
        /// Path to the .packignore file.
        path: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum PresetCommand {
    /// List available preset working copies.
    List,
    /// Install or reset a preset working copy.
    Install {
        /// Preset ID shown by `foldry preset list`.
        id: String,
    },
    /// Remove an installed preset working copy.
    Remove {
        /// Preset ID shown by `foldry preset list`.
        id: String,
    },
}

#[derive(Debug, Args)]
struct PreviewArgs {
    /// Folder UUID shown by `foldry folder list`.
    folder_id: String,
    /// Action UUID shown by `foldry action list <FOLDER_ID>`.
    action_id: String,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum FormatArg {
    Zip,
    TarGz,
    TarZst,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CompressionArg {
    Fast,
    Balanced,
    Maximum,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ConflictArg {
    Skip,
    Overwrite,
    Increment,
}

#[derive(Debug, Args)]
struct ArchiveArgs {
    /// Folder to archive.
    source: PathBuf,
    /// Ignore Profile UUID, file name, or exact profile name.
    #[arg(long)]
    profile: Option<String>,
    /// Output directory. Defaults to the source folder's parent.
    #[arg(long)]
    output: Option<PathBuf>,
    /// Archive file name without the format extension.
    #[arg(long)]
    name: Option<String>,
    /// Archive format.
    #[arg(long, value_enum)]
    format: Option<FormatArg>,
    /// Compression level.
    #[arg(long, value_enum)]
    compression: Option<CompressionArg>,
    /// Behavior when the destination already exists.
    #[arg(long, value_enum)]
    conflict: Option<ConflictArg>,
    /// Store the source contents at the archive root.
    #[arg(long)]
    no_include_root: bool,
    /// Read every archived entry back after writing.
    #[arg(long)]
    full_verify: bool,
    /// Calculate and report a SHA-256 checksum.
    #[arg(long)]
    checksum: bool,
}

#[derive(Debug, Subcommand)]
enum FolderCommand {
    /// List folders currently shown in Foldry.
    List,
    /// Remember a folder and create its first Archive action.
    Add {
        /// Existing folder to add.
        source: PathBuf,
        /// Default Ignore Profile UUID, file name, or exact name.
        #[arg(long)]
        profile: Option<String>,
    },
    /// Remove a folder from the main list but keep its configuration.
    Unlist {
        /// Folder UUID.
        folder_id: String,
    },
    /// List remembered folders that are not in the main list.
    Remembered,
    /// Permanently forget one or more unlisted folders.
    Forget {
        /// Folder UUIDs. Only unlisted folders can be forgotten.
        folder_ids: Vec<String>,
    },
    /// Include a folder in `foldry run all`.
    Enable {
        /// Folder UUID.
        folder_id: String,
    },
    /// Exclude a folder from `foldry run all`.
    Disable {
        /// Folder UUID.
        folder_id: String,
    },
}

#[derive(Debug, Subcommand)]
enum ActionCommand {
    /// List actions configured for a folder.
    List {
        /// Folder UUID.
        folder_id: String,
    },
    /// Add an Archive action to a folder.
    Add {
        /// Folder UUID.
        folder_id: String,
        /// Create the action enabled. New actions are disabled by default.
        #[arg(long)]
        enabled: bool,
        /// Override the folder's Ignore Profile by UUID, file name, or exact name.
        #[arg(long)]
        profile: Option<String>,
    },
    /// Remove an action from a folder.
    Remove {
        /// Folder UUID.
        folder_id: String,
        /// Action UUID.
        action_id: String,
    },
    /// Replace an action configuration from a JSON file.
    Update {
        /// Folder UUID.
        folder_id: String,
        /// Action UUID. The JSON file must preserve this ID.
        action_id: String,
        /// Path to a serialized FolderAction JSON object.
        #[arg(long = "from")]
        source: PathBuf,
    },
    /// Enable an action.
    Enable {
        /// Folder UUID.
        folder_id: String,
        /// Action UUID.
        action_id: String,
    },
    /// Disable an action.
    Disable {
        /// Folder UUID.
        folder_id: String,
        /// Action UUID.
        action_id: String,
    },
    /// Run one configured action immediately.
    Run {
        /// Folder UUID.
        folder_id: String,
        /// Action UUID.
        action_id: String,
    },
    /// Set the action order for a folder.
    Reorder {
        /// Folder UUID.
        folder_id: String,
        /// Every action UUID, in the desired order.
        action_ids: Vec<String>,
    },
}

#[derive(Debug, Subcommand)]
enum RunCommand {
    /// Run all enabled actions for one folder.
    Folder {
        /// Folder UUID.
        folder_id: String,
    },
    /// Run enabled actions for every enabled folder.
    All,
    /// Repeat an immutable historical run snapshot.
    Repeat {
        /// Run UUID shown by `foldry history list`.
        run_id: String,
    },
}

#[derive(Debug, Subcommand)]
enum HistoryCommand {
    /// List recent runs, newest first.
    List {
        /// Maximum number of runs to return.
        #[arg(long, default_value_t = 20)]
        limit: u32,
        /// Number of runs to skip.
        #[arg(long, default_value_t = 0)]
        offset: u64,
        /// Filter by folder UUID.
        #[arg(long)]
        folder: Option<String>,
        /// Filter by action UUID.
        #[arg(long)]
        action: Option<String>,
    },
    /// Show one complete persisted run record.
    Show {
        /// Run UUID.
        run_id: String,
    },
    /// Read persisted log entries for a run.
    Logs {
        /// Run UUID.
        run_id: String,
        /// Maximum number of log entries to return.
        #[arg(long, default_value_t = 200)]
        limit: u32,
        /// Number of log entries to skip.
        #[arg(long, default_value_t = 0)]
        offset: u64,
    },
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    /// Print effective settings.
    Show,
    /// Print Foldry's config, data, cache, and database paths.
    Path,
}

pub fn run(args: impl IntoIterator<Item = OsString>) -> i32 {
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(error) => {
            let code = if error.use_stderr() {
                EXIT_USAGE
            } else {
                EXIT_SUCCESS
            };
            let _ = error.print();
            return code;
        }
    };
    let json_output = cli.json;
    match execute(cli) {
        Ok(result) => {
            print_success(json_output, &result.data, &result.human);
            result.exit_code
        }
        Err(error) => {
            print_error(json_output, &error);
            error.exit_code
        }
    }
}

struct CommandResult {
    exit_code: i32,
    data: Value,
    human: String,
}

#[derive(Debug)]
struct CliError {
    exit_code: i32,
    code: &'static str,
    message: String,
}

impl CliError {
    fn validation(message: impl Into<String>) -> Self {
        Self {
            exit_code: EXIT_VALIDATION,
            code: "validation_error",
            message: message.into(),
        }
    }

    fn io(message: impl Into<String>) -> Self {
        Self {
            exit_code: EXIT_IO,
            code: "io_error",
            message: message.into(),
        }
    }

    fn config(message: impl Into<String>) -> Self {
        Self {
            exit_code: EXIT_CONFIG,
            code: "config_error",
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            exit_code: EXIT_INTERNAL,
            code: "internal_error",
            message: message.into(),
        }
    }
}

fn execute(cli: Cli) -> Result<CommandResult, CliError> {
    interrupt_flag()?.store(false, Ordering::Release);
    let runtime = Runtime::open(&cli)?;
    match cli.command {
        Command::Profile { command } => profile_command(&runtime, command),
        Command::Preset { command } => preset_command(&runtime, command),
        Command::Folder { command } => folder_command(&runtime, command),
        Command::Action { command } => action_command(&runtime, command, cli.json),
        Command::Preview(args) => preview_command(&runtime, args),
        Command::Archive(args) => archive_command(&runtime, args, cli.json),
        Command::Run { command } => run_command(&runtime, command, cli.json),
        Command::History { command } => history_command(&runtime, command),
        Command::Config { command } => config_command(&runtime, command),
    }
}

struct Runtime {
    directories: AppDirectories,
    resources: PathBuf,
    services: ApplicationServices,
}

impl Runtime {
    fn open(cli: &Cli) -> Result<Self, CliError> {
        let directories = AppDirectories::resolve(&DirectoryOverrides {
            config: cli.config_dir.clone(),
            data: cli.data_dir.clone(),
            cache: cli.cache_dir.clone(),
        })
        .map_err(|error| CliError::config(error.to_string()))?;
        directories
            .ensure_layout()
            .map_err(|error| CliError::io(error.to_string()))?;
        let resources = resource_directory();
        initialize_resource_copies(&resources, &directories.config)
            .map_err(|error| CliError::io(error.to_string()))?;

        let reconciliation_db =
            SqliteRepository::open(&directories.database()).map_err(repository_error)?;
        let active_repository = FileActivePlanRepository::new(directories.active_plan());
        let plan = active_repository
            .load()
            .map_err(repository_error)?
            .unwrap_or_else(empty_plan);
        let output_directories = output_directories(&plan);
        reconcile_startup(
            &reconciliation_db,
            SystemClock.now(),
            &output_directories,
            &directories.manifests(),
            24 * 60 * 60,
            &SystemProcessProbe,
        )
        .map_err(repository_error)?;

        let services = ApplicationServices::bootstrap(ApplicationPorts {
            settings: Box::new(FileSettingsRepository::new(directories.settings())),
            active_plan: Box::new(active_repository),
            profiles: Box::new(FileProfileRepository::new(
                directories.profiles(),
                resources.join("profiles/default.packignore"),
            )),
            presets: Box::new(FilePresetRepository::new(
                directories.presets(),
                resources.join("presets"),
            )),
            history: Box::new(
                SqliteRepository::open(&directories.database()).map_err(repository_error)?,
            ),
            logs: Box::new(
                SqliteRepository::open(&directories.database()).map_err(repository_error)?,
            ),
            clock: Box::new(SystemClock),
            ids: Box::new(UuidIdGenerator),
        })
        .map_err(|error| CliError::config(error.to_string()))?;
        services
            .apply_retention()
            .map_err(|error| CliError::io(error.to_string()))?;
        Ok(Self {
            directories,
            resources,
            services,
        })
    }

    fn resolve_profile(
        &self,
        selector: Option<&str>,
    ) -> Result<foldry_application::StoredProfile, CliError> {
        let profiles = self
            .services
            .profiles()
            .map_err(|error| CliError::io(error.to_string()))?;
        let profile = if let Some(selector) = selector {
            let id = selector.parse::<ProfileId>().ok();
            profiles
                .iter()
                .find(|profile| profile.id == id)
                .cloned()
                .or_else(|| {
                    profiles
                        .iter()
                        .find(|profile| {
                            profile
                                .path
                                .file_name()
                                .is_some_and(|name| name == selector)
                        })
                        .cloned()
                })
                .or_else(|| {
                    let matches = profiles
                        .iter()
                        .filter(|profile| profile.name == selector)
                        .cloned()
                        .collect::<Vec<_>>();
                    (matches.len() == 1).then(|| matches[0].clone())
                })
        } else {
            let default_id = self
                .services
                .state()
                .map_err(|error| CliError::config(error.to_string()))?
                .settings
                .default_profile_id;
            profiles
                .iter()
                .find(|profile| profile.id == default_id)
                .cloned()
                .or_else(|| {
                    profiles
                        .iter()
                        .find(|profile| profile.name == "Default")
                        .cloned()
                })
                .or_else(|| profiles.into_iter().next())
        };
        let profile =
            profile.ok_or_else(|| CliError::config("no matching profile is installed"))?;
        if !profile.valid {
            return Err(CliError::validation(format!(
                "profile `{}` is invalid: {}",
                profile.name,
                profile
                    .diagnostics
                    .iter()
                    .map(|diagnostic| diagnostic.message.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            )));
        }
        Ok(profile)
    }
}

fn profile_command(runtime: &Runtime, command: ProfileCommand) -> Result<CommandResult, CliError> {
    match command {
        ProfileCommand::List => {
            let profiles = runtime
                .services
                .profiles()
                .map_err(|error| CliError::io(error.to_string()))?;
            let data = profiles.iter().map(profile_json).collect::<Vec<_>>();
            let human = if profiles.is_empty() {
                "No profiles.".into()
            } else {
                profiles
                    .iter()
                    .map(|profile| {
                        format!(
                            "{}\t{}\t{}",
                            profile.id.map_or_else(|| "-".into(), |id| id.to_string()),
                            profile.name,
                            if profile.valid { "valid" } else { "invalid" }
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            ok(Value::Array(data), human)
        }
        ProfileCommand::Show { id } => {
            let id = parse_profile_id(&id)?;
            let profile = runtime
                .services
                .profiles()
                .map_err(|error| CliError::io(error.to_string()))?
                .into_iter()
                .find(|profile| profile.id == Some(id))
                .ok_or_else(|| CliError::validation(format!("profile {id} not found")))?;
            ok(profile_json(&profile), profile.text)
        }
        ProfileCommand::Create { name, filename } => {
            if name.trim().is_empty() {
                return Err(CliError::validation("profile name cannot be empty"));
            }
            let filename = filename.unwrap_or_else(|| format!("{}.packignore", slug(&name)));
            if runtime
                .services
                .profiles()
                .map_err(|error| CliError::io(error.to_string()))?
                .iter()
                .any(|profile| {
                    profile
                        .path
                        .file_name()
                        .is_some_and(|value| value.to_string_lossy() == filename)
                })
            {
                return Err(CliError::validation(format!(
                    "profile file `{filename}` already exists"
                )));
            }
            let text = format!(
                "# @profile-id {}\n# @profile-version 1\n# @profile-name {}\n",
                ProfileId::new(),
                name.trim()
            );
            let profile = runtime
                .services
                .save_profile_text(&filename, &text)
                .map_err(|error| CliError::validation(error.to_string()))?;
            ok(
                profile_json(&profile),
                format!("Created profile {}.", profile.name),
            )
        }
        ProfileCommand::Edit { id, source } => {
            let id = parse_profile_id(&id)?;
            let profiles = runtime
                .services
                .profiles()
                .map_err(|error| CliError::io(error.to_string()))?;
            let existing = profiles
                .into_iter()
                .find(|profile| profile.id == Some(id))
                .ok_or_else(|| CliError::validation(format!("profile {id} not found")))?;
            let text = fs::read_to_string(&source).map_err(|error| {
                CliError::io(format!("cannot read {}: {error}", source.display()))
            })?;
            let metadata_id = parse_profile(&text)
                .profile
                .map(|profile| profile.id)
                .or_else(|| {
                    text.lines()
                        .find_map(|line| line.strip_prefix("# @profile-id "))
                        .and_then(|value| value.trim().parse().ok())
                });
            if metadata_id != Some(id) {
                return Err(CliError::validation(
                    "edited profile must preserve its @profile-id",
                ));
            }
            let filename = existing
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| CliError::config("profile filename is not UTF-8"))?;
            let profile = runtime
                .services
                .save_profile_text(filename, &text)
                .map_err(|error| CliError::io(error.to_string()))?;
            ok(profile_json(&profile), format!("Saved {}.", profile.name))
        }
        ProfileCommand::Delete { id } => {
            let id = parse_profile_id(&id)?;
            let deleted = runtime
                .services
                .delete_profile(id)
                .map_err(|error| CliError::validation(error.to_string()))?;
            if !deleted {
                return Err(CliError::validation(format!("profile {id} not found")));
            }
            ok(json!({"deleted": id}), format!("Deleted profile {id}."))
        }
        ProfileCommand::Validate { path } => {
            let text = fs::read_to_string(&path).map_err(|error| {
                CliError::io(format!("cannot read {}: {error}", path.display()))
            })?;
            let parsed = parse_profile(&text);
            let data = json!({
                "valid": parsed.profile.is_some(),
                "diagnostics": parsed.diagnostics,
            });
            let human = if parsed.profile.is_some() {
                "Profile is valid.".into()
            } else {
                parsed
                    .diagnostics
                    .iter()
                    .map(|diagnostic| diagnostic.message.clone())
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            if parsed.profile.is_some() {
                ok(data, human)
            } else {
                Ok(CommandResult {
                    exit_code: EXIT_VALIDATION,
                    data,
                    human,
                })
            }
        }
    }
}

fn preset_command(runtime: &Runtime, command: PresetCommand) -> Result<CommandResult, CliError> {
    match command {
        PresetCommand::List => {
            let presets = runtime
                .services
                .presets()
                .map_err(|error| CliError::io(error.to_string()))?;
            let data = presets
                .iter()
                .map(|preset| {
                    json!({
                        "id": preset.id,
                        "path": preset.path,
                        "resource_version": preset.resource_version,
                    })
                })
                .collect::<Vec<_>>();
            let human = presets
                .iter()
                .map(|preset| preset.id.to_string())
                .collect::<Vec<_>>()
                .join("\n");
            ok(Value::Array(data), human)
        }
        PresetCommand::Install { id } => {
            let id = parse_preset_id(&id)?;
            let preset = runtime
                .services
                .reset_preset(&id)
                .map_err(|error| CliError::validation(error.to_string()))?;
            ok(
                json!({"id": preset.id, "path": preset.path}),
                format!("Installed preset {}.", preset.id),
            )
        }
        PresetCommand::Remove { id } => {
            let id = parse_preset_id(&id)?;
            let deleted = runtime
                .services
                .delete_preset(&id)
                .map_err(|error| CliError::io(error.to_string()))?;
            if !deleted {
                return Err(CliError::validation(format!(
                    "preset {id} is not installed"
                )));
            }
            ok(json!({"deleted": id}), format!("Removed preset {id}."))
        }
    }
}

fn folder_command(runtime: &Runtime, command: FolderCommand) -> Result<CommandResult, CliError> {
    match command {
        FolderCommand::List => {
            let folders = runtime
                .services
                .state()
                .map_err(|error| CliError::config(error.to_string()))?
                .active_plan
                .folders
                .into_iter()
                .filter(|folder| folder.listed)
                .collect::<Vec<_>>();
            folder_list_result(folders)
        }
        FolderCommand::Add { source, profile } => {
            let profile_id = runtime
                .resolve_profile(profile.as_deref())?
                .id
                .ok_or_else(|| CliError::config("profile is missing an ID"))?;
            let folder = runtime
                .services
                .add_folder(source, Some(profile_id))
                .map_err(|error| CliError::validation(error.to_string()))?;
            ok(
                serde_json::to_value(&folder).map_err(json_error)?,
                format!("Added folder {}.", folder.source.display()),
            )
        }
        FolderCommand::Unlist { folder_id } => {
            let folder_id = parse_folder_id(&folder_id)?;
            let changed = runtime
                .services
                .unlist_folder(folder_id)
                .map_err(|error| CliError::validation(error.to_string()))?;
            ok(
                json!({"folder_id": folder_id, "unlisted": changed}),
                if changed {
                    format!("Unlisted folder {folder_id}.")
                } else {
                    format!("Folder {folder_id} was already unlisted.")
                },
            )
        }
        FolderCommand::Remembered => {
            let folders = runtime
                .services
                .unlisted_folders()
                .map_err(|error| CliError::io(error.to_string()))?;
            folder_list_result(folders)
        }
        FolderCommand::Forget { folder_ids } => {
            if folder_ids.is_empty() {
                return Err(CliError::validation(
                    "provide at least one remembered folder ID",
                ));
            }
            let folder_ids = folder_ids
                .iter()
                .map(|id| parse_folder_id(id))
                .collect::<Result<Vec<_>, _>>()?;
            let removed = runtime
                .services
                .forget_folders(&folder_ids)
                .map_err(|error| CliError::validation(error.to_string()))?;
            ok(
                json!({"forgotten": removed, "folder_ids": folder_ids}),
                format!("Forgot {removed} folder(s)."),
            )
        }
        FolderCommand::Enable { folder_id } => set_folder_enabled(runtime, &folder_id, true),
        FolderCommand::Disable { folder_id } => set_folder_enabled(runtime, &folder_id, false),
    }
}

fn action_command(
    runtime: &Runtime,
    command: ActionCommand,
    json_output: bool,
) -> Result<CommandResult, CliError> {
    match command {
        ActionCommand::List { folder_id } => {
            let folder = configured_folder(runtime, parse_folder_id(&folder_id)?)?;
            let human = folder
                .actions
                .iter()
                .map(|action| {
                    format!(
                        "{}\t{}\t{}",
                        action.id,
                        action.spec.action_type(),
                        if action.enabled {
                            "enabled"
                        } else {
                            "disabled"
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            ok(
                serde_json::to_value(&folder.actions).map_err(json_error)?,
                human,
            )
        }
        ActionCommand::Add {
            folder_id,
            enabled,
            profile,
        } => {
            let folder_id = parse_folder_id(&folder_id)?;
            let profile_id = profile
                .as_deref()
                .map(|selector| {
                    runtime
                        .resolve_profile(Some(selector))?
                        .id
                        .ok_or_else(|| CliError::config("profile is missing an ID"))
                })
                .transpose()?;
            let action = runtime
                .services
                .add_archive_action(folder_id, enabled, profile_id)
                .map_err(|error| CliError::validation(error.to_string()))?;
            ok(
                serde_json::to_value(&action).map_err(json_error)?,
                format!("Added Archive action {}.", action.id),
            )
        }
        ActionCommand::Remove {
            folder_id,
            action_id,
        } => {
            let folder_id = parse_folder_id(&folder_id)?;
            let action_id = parse_action_id(&action_id)?;
            let removed = runtime
                .services
                .remove_action(folder_id, action_id)
                .map_err(|error| CliError::validation(error.to_string()))?;
            if !removed {
                return Err(CliError::validation(format!(
                    "action {action_id} not found"
                )));
            }
            ok(
                json!({"folder_id": folder_id, "removed": action_id}),
                format!("Removed action {action_id}."),
            )
        }
        ActionCommand::Update {
            folder_id,
            action_id,
            source,
        } => {
            let folder_id = parse_folder_id(&folder_id)?;
            let action_id = parse_action_id(&action_id)?;
            let text = fs::read_to_string(&source).map_err(|error| {
                CliError::io(format!("cannot read {}: {error}", source.display()))
            })?;
            let action: FolderAction = serde_json::from_str(&text).map_err(|error| {
                CliError::validation(format!("invalid FolderAction JSON: {error}"))
            })?;
            if action.id != action_id {
                return Err(CliError::validation(
                    "updated action JSON must preserve its action ID",
                ));
            }
            runtime
                .services
                .update_action(folder_id, action.clone())
                .map_err(|error| CliError::validation(error.to_string()))?;
            ok(
                serde_json::to_value(&action).map_err(json_error)?,
                format!("Updated action {action_id}."),
            )
        }
        ActionCommand::Enable {
            folder_id,
            action_id,
        } => set_action_enabled(runtime, &folder_id, &action_id, true),
        ActionCommand::Disable {
            folder_id,
            action_id,
        } => set_action_enabled(runtime, &folder_id, &action_id, false),
        ActionCommand::Run {
            folder_id,
            action_id,
        } => {
            let run = runtime
                .services
                .prepare_run_current(parse_folder_id(&folder_id)?, parse_action_id(&action_id)?)
                .map_err(|error| CliError::validation(error.to_string()))?;
            result_from_summaries(execute_runs(runtime, vec![run], json_output)?)
        }
        ActionCommand::Reorder {
            folder_id,
            action_ids,
        } => {
            let folder_id = parse_folder_id(&folder_id)?;
            let action_ids = action_ids
                .iter()
                .map(|id| parse_action_id(id))
                .collect::<Result<Vec<_>, _>>()?;
            runtime
                .services
                .reorder_actions(folder_id, &action_ids)
                .map_err(|error| CliError::validation(error.to_string()))?;
            ok(
                json!({"folder_id": folder_id, "action_ids": action_ids}),
                format!("Reordered actions for folder {folder_id}."),
            )
        }
    }
}

fn folder_list_result(folders: Vec<Folder>) -> Result<CommandResult, CliError> {
    let human = folders
        .iter()
        .map(|folder| {
            format!(
                "{}\t{}\t{}\t{} action(s)",
                folder.id,
                folder.source.display(),
                if folder.enabled {
                    "enabled"
                } else {
                    "disabled"
                },
                folder.actions.len()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    ok(serde_json::to_value(folders).map_err(json_error)?, human)
}

fn configured_folder(runtime: &Runtime, folder_id: FolderId) -> Result<Folder, CliError> {
    runtime
        .services
        .state()
        .map_err(|error| CliError::config(error.to_string()))?
        .active_plan
        .folders
        .into_iter()
        .find(|folder| folder.id == folder_id)
        .ok_or_else(|| CliError::validation(format!("folder {folder_id} not found")))
}

fn set_folder_enabled(
    runtime: &Runtime,
    folder_id: &str,
    enabled: bool,
) -> Result<CommandResult, CliError> {
    let folder_id = parse_folder_id(folder_id)?;
    let mut folder = configured_folder(runtime, folder_id)?;
    folder.enabled = enabled;
    runtime
        .services
        .update_folder(folder)
        .map_err(|error| CliError::validation(error.to_string()))?;
    ok(
        json!({"folder_id": folder_id, "enabled": enabled}),
        format!(
            "{} folder {folder_id}.",
            if enabled { "Enabled" } else { "Disabled" }
        ),
    )
}

fn set_action_enabled(
    runtime: &Runtime,
    folder_id: &str,
    action_id: &str,
    enabled: bool,
) -> Result<CommandResult, CliError> {
    let folder_id = parse_folder_id(folder_id)?;
    let action_id = parse_action_id(action_id)?;
    let mut action = configured_folder(runtime, folder_id)?
        .actions
        .into_iter()
        .find(|action| action.id == action_id)
        .ok_or_else(|| CliError::validation(format!("action {action_id} not found")))?;
    action.enabled = enabled;
    runtime
        .services
        .update_action(folder_id, action)
        .map_err(|error| CliError::validation(error.to_string()))?;
    ok(
        json!({"folder_id": folder_id, "action_id": action_id, "enabled": enabled}),
        format!(
            "{} action {action_id}.",
            if enabled { "Enabled" } else { "Disabled" }
        ),
    )
}

fn preview_command(runtime: &Runtime, args: PreviewArgs) -> Result<CommandResult, CliError> {
    let preview = runtime
        .services
        .prepare_preview(
            parse_folder_id(&args.folder_id)?,
            parse_action_id(&args.action_id)?,
        )
        .map_err(|error| CliError::validation(error.to_string()))?;
    let profile = preview.profile;
    let parsed = parse_profile(&profile.text);
    let profile_contract = parsed.profile.expect("validated repository profile");
    let case = detect_case_sensitivity(&preview.folder.source)
        .map_err(|error| CliError::io(error.to_string()))?;
    let matcher =
        CompiledProfile::new(&profile_contract, case.value).map_err(CliError::validation)?;
    let cancellation = foldry_application::CancellationToken::default();
    let interrupt = interrupt_flag()?;
    let cancellation_clone = cancellation.clone();
    let finished = Arc::new(AtomicBool::new(false));
    let watcher_finished = Arc::clone(&finished);
    let watcher = std::thread::spawn(move || {
        while !interrupt.load(Ordering::Acquire) && !watcher_finished.load(Ordering::Acquire) {
            std::thread::sleep(Duration::from_millis(25));
        }
        if interrupt.load(Ordering::Acquire) {
            cancellation_clone.cancel();
        }
    });
    let manifest_id = RunId::new().to_string();
    let result = scan_to_manifest(
        &runtime.directories.manifests(),
        &manifest_id,
        &preview.folder.source,
        &matcher,
        &cancellation,
    );
    finished.store(true, Ordering::Release);
    let _ = watcher.join();
    let (manifest, summary) = result.map_err(|error| {
        if cancellation.is_cancelled() {
            CliError {
                exit_code: EXIT_CANCELLED,
                code: "cancelled",
                message: "preview cancelled".into(),
            }
        } else {
            CliError::io(error.to_string())
        }
    })?;
    manifest
        .remove()
        .map_err(|error| CliError::io(error.to_string()))?;
    ok(
        json!({
            "folder_id": preview.folder.id,
            "action_id": preview.action.id,
            "effective_profile_id": profile.id,
            "summary": summary,
        }),
        format!(
            "Included {} entries ({} bytes); excluded {}; skipped {}.",
            summary.included_entries,
            summary.included_bytes,
            summary.excluded_entries,
            summary.skipped_entries
        ),
    )
}

fn archive_command(
    runtime: &Runtime,
    args: ArchiveArgs,
    json_output: bool,
) -> Result<CommandResult, CliError> {
    let profile = runtime.resolve_profile(args.profile.as_deref())?;
    let settings = runtime
        .services
        .state()
        .map_err(|error| CliError::config(error.to_string()))?
        .settings;
    let source = fs::canonicalize(&args.source)
        .map_err(|error| CliError::io(format!("cannot access source: {error}")))?;
    if !source.is_dir() {
        return Err(CliError::validation("archive source must be a directory"));
    }
    let defaults = settings.archive_defaults.clone();
    let format = args.format.map_or(defaults.format, Into::into);
    let filename = args.name.unwrap_or_else(|| {
        source.file_name().map_or_else(
            || "archive".into(),
            |name| name.to_string_lossy().into_owned(),
        )
    });
    let profile_id = profile.id.expect("valid profile has ID");
    let action = FolderAction {
        id: ActionId::new(),
        enabled: true,
        profile_id_override: None,
        spec: ActionSpec::Archive(ArchiveActionSpec {
            version: ActionVersion::V1,
            output: ArchiveOutputSpec {
                directory: args.output.map_or(ArchiveOutputDirectory::Parent, |path| {
                    ArchiveOutputDirectory::Custom { path }
                }),
                filename,
                format,
                compression: args.compression.map_or(defaults.compression, Into::into),
                conflict_policy: args.conflict.map_or(defaults.conflict_policy, Into::into),
                extensions: Extensions::new(),
            },
            include_root: !args.no_include_root,
            unreadable_policy: defaults.unreadable_policy,
            verification: VerificationSpec {
                mode: if args.full_verify {
                    VerificationMode::Full
                } else {
                    defaults.verification_mode
                },
                checksum: if args.checksum {
                    ChecksumAlgorithm::Sha256
                } else {
                    defaults.checksum
                },
                extensions: Extensions::new(),
            },
            extensions: Extensions::new(),
        }),
        extensions: Extensions::new(),
    };
    let folder = Folder {
        id: FolderId::new(),
        source,
        listed: true,
        enabled: true,
        default_profile_id: profile_id,
        actions: vec![action.clone()],
        extensions: Extensions::new(),
    };
    let run = queued_run(folder, action, profile_id, settings, profile.text);
    let summaries = execute_runs(runtime, vec![run], json_output)?;
    result_from_summaries(summaries)
}

fn run_command(
    runtime: &Runtime,
    command: RunCommand,
    json_output: bool,
) -> Result<CommandResult, CliError> {
    let runs = match command {
        RunCommand::Folder { folder_id } => runtime
            .services
            .prepare_folder_enabled(parse_folder_id(&folder_id)?)
            .map_err(|error| CliError::validation(error.to_string()))?,
        RunCommand::All => runtime
            .services
            .prepare_all_enabled()
            .map_err(|error| CliError::validation(error.to_string()))?,
        RunCommand::Repeat { run_id } => vec![
            runtime
                .services
                .repeat_run(parse_run_id(&run_id)?)
                .map_err(|error| CliError::validation(error.to_string()))?,
        ],
    };
    if runs.is_empty() {
        return ok(json!([]), "No enabled actions to run.".into());
    }
    result_from_summaries(execute_runs(runtime, runs, json_output)?)
}

fn history_command(runtime: &Runtime, command: HistoryCommand) -> Result<CommandResult, CliError> {
    match command {
        HistoryCommand::List {
            limit,
            offset,
            folder,
            action,
        } => {
            let runs = runtime
                .services
                .history_filtered(
                    PageRequest { offset, limit },
                    folder.as_deref().map(parse_folder_id).transpose()?,
                    action.as_deref().map(parse_action_id).transpose()?,
                )
                .map_err(|error| CliError::io(error.to_string()))?;
            let human = runs
                .iter()
                .map(|run| {
                    format!(
                        "{}\t{:?}\t{}",
                        run.run_id,
                        run.state,
                        run.snapshot.folder.source.display()
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            ok(serde_json::to_value(runs).map_err(json_error)?, human)
        }
        HistoryCommand::Show { run_id } => {
            let run_id = run_id
                .parse()
                .map_err(|error| CliError::validation(format!("invalid run ID: {error}")))?;
            let run = runtime
                .services
                .run(run_id)
                .map_err(|error| CliError::io(error.to_string()))?
                .ok_or_else(|| CliError::validation(format!("run {run_id} not found")))?;
            ok(
                serde_json::to_value(&run).map_err(json_error)?,
                format!("{:#?}", run),
            )
        }
        HistoryCommand::Logs {
            run_id,
            limit,
            offset,
        } => {
            let run_id = parse_run_id(&run_id)?;
            let logs = runtime
                .services
                .logs(run_id, PageRequest { offset, limit })
                .map_err(|error| CliError::io(error.to_string()))?;
            let human = logs
                .iter()
                .map(|log| format!("{}\t{:?}\t{}", log.sequence, log.level, log.message))
                .collect::<Vec<_>>()
                .join("\n");
            ok(serde_json::to_value(logs).map_err(json_error)?, human)
        }
    }
}

fn config_command(runtime: &Runtime, command: ConfigCommand) -> Result<CommandResult, CliError> {
    match command {
        ConfigCommand::Show => {
            let settings = runtime
                .services
                .state()
                .map_err(|error| CliError::config(error.to_string()))?
                .settings;
            ok(
                serde_json::to_value(&settings).map_err(json_error)?,
                serde_json::to_string_pretty(&settings).map_err(json_error)?,
            )
        }
        ConfigCommand::Path => ok(
            json!({
                "config": runtime.directories.config,
                "data": runtime.directories.data,
                "cache": runtime.directories.cache,
                "settings": runtime.directories.settings(),
                "active_plan": runtime.directories.active_plan(),
                "database": runtime.directories.database(),
                "resources": runtime.resources,
            }),
            format!(
                "config: {}\ndata: {}\ncache: {}",
                runtime.directories.config.display(),
                runtime.directories.data.display(),
                runtime.directories.cache.display()
            ),
        ),
    }
}

#[derive(Default)]
struct CliEvents {
    summaries: Mutex<HashMap<RunId, ResultSummary>>,
    changed: Condvar,
    human_progress: bool,
}

impl RunEventSink for CliEvents {
    fn publish(&self, event: RunEvent) {
        match event.event {
            RunEventKind::Progress { progress } if self.human_progress => {
                eprintln!(
                    "{} {:?}: {}/{} entries",
                    event.run_id,
                    progress.phase,
                    progress.completed_entries,
                    progress
                        .total_entries
                        .map_or_else(|| "?".into(), |total| total.to_string())
                );
            }
            RunEventKind::StateChanged { state } if self.human_progress => {
                eprintln!("{}: {state:?}", event.run_id);
            }
            RunEventKind::Warning { ref warning } if self.human_progress => {
                eprintln!("{} warning: {}", event.run_id, warning.message);
            }
            RunEventKind::Completed { summary } => {
                self.summaries
                    .lock()
                    .expect("CLI summaries")
                    .insert(event.run_id, summary);
                self.changed.notify_all();
            }
            _ => {}
        }
    }
}

fn execute_runs(
    runtime: &Runtime,
    runs: Vec<RunRecord>,
    json_output: bool,
) -> Result<Vec<ResultSummary>, CliError> {
    let repository = Arc::new(
        SqliteRepository::open(&runtime.directories.database()).map_err(repository_error)?,
    );
    let history: Arc<dyn RunHistoryRepository> = repository.clone();
    let logs: Arc<dyn LogRepository> = repository;
    let events = Arc::new(CliEvents {
        summaries: Mutex::new(HashMap::new()),
        changed: Condvar::new(),
        human_progress: !json_output && io::stderr().is_terminal(),
    });
    let event_sink: Arc<dyn RunEventSink> = events.clone();
    let limit = runtime
        .services
        .state()
        .map_err(|error| CliError::config(error.to_string()))?
        .settings
        .execution
        .max_parallel_runs;
    let scheduler = Scheduler::start(
        SchedulerPorts {
            history,
            logs,
            clock: Arc::new(SystemClock),
            executor: Arc::new(ArchiveRunExecutor::new(runtime.directories.manifests())),
            events: event_sink,
        },
        limit,
    )
    .map_err(|error| CliError::internal(error.to_string()))?;
    let run_ids = runs.iter().map(|run| run.run_id).collect::<Vec<_>>();
    for run in runs {
        scheduler
            .enqueue(run)
            .map_err(|error| CliError::internal(error.to_string()))?;
    }
    let interrupt = interrupt_flag()?;
    let deadline = Instant::now() + Duration::from_secs(7 * 24 * 60 * 60);
    let mut stopped = false;
    let mut summaries = events.summaries.lock().expect("CLI summaries");
    while summaries.len() < run_ids.len() {
        if interrupt.load(Ordering::Acquire) && !stopped {
            scheduler
                .stop_all()
                .map_err(|error| CliError::internal(error.to_string()))?;
            stopped = true;
        }
        if Instant::now() >= deadline {
            return Err(CliError::internal("scheduler wait deadline exceeded"));
        }
        summaries = events
            .changed
            .wait_timeout(summaries, Duration::from_millis(50))
            .expect("CLI summaries")
            .0;
    }
    Ok(run_ids
        .into_iter()
        .filter_map(|run_id| summaries.remove(&run_id))
        .collect())
}

fn result_from_summaries(summaries: Vec<ResultSummary>) -> Result<CommandResult, CliError> {
    let stopped = summaries
        .iter()
        .any(|summary| summary.outcome == RunOutcome::Stopped);
    let failed = summaries
        .iter()
        .any(|summary| summary.outcome == RunOutcome::Failed);
    let partial = summaries
        .iter()
        .any(|summary| summary.outcome == RunOutcome::SucceededWithWarnings)
        || (failed
            && summaries.iter().any(|summary| {
                matches!(
                    summary.outcome,
                    RunOutcome::Succeeded | RunOutcome::SucceededWithWarnings
                )
            }));
    let exit_code = if stopped {
        EXIT_CANCELLED
    } else if partial {
        EXIT_PARTIAL
    } else if failed {
        EXIT_IO
    } else {
        EXIT_SUCCESS
    };
    let human = summaries
        .iter()
        .map(|summary| {
            summary.artifact.as_ref().map_or_else(
                || format!("{:?}", summary.outcome),
                |artifact| {
                    format!(
                        "{:?}: {} ({} bytes)",
                        summary.outcome,
                        artifact.path.display(),
                        artifact.size_bytes
                    )
                },
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(CommandResult {
        exit_code,
        data: serde_json::to_value(summaries).map_err(json_error)?,
        human,
    })
}

fn queued_run(
    folder: Folder,
    action: FolderAction,
    effective_profile_id: ProfileId,
    settings: Settings,
    profile_text: String,
) -> RunRecord {
    RunRecord {
        run_id: RunId::new(),
        folder_id: folder.id,
        action_id: action.id,
        state: RunState::Queued,
        started_at: SystemClock.now(),
        finished_at: None,
        snapshot: RunSnapshot {
            folder: FolderSnapshot {
                id: folder.id,
                source: folder.source,
            },
            action,
            effective_profile_id,
            settings,
            profile_hash: format!("{:x}", Sha256::digest(profile_text.as_bytes())),
            profile_text,
        },
        summary: None,
    }
}

fn profile_json(profile: &foldry_application::StoredProfile) -> Value {
    json!({
        "id": profile.id,
        "name": profile.name,
        "path": profile.path,
        "valid": profile.valid,
        "diagnostics": profile.diagnostics,
        "text": profile.text,
    })
}

fn empty_plan() -> Plan {
    Plan {
        version: PlanVersion::CURRENT,
        name: "Active plan".into(),
        folders: Vec::new(),
        extensions: Extensions::new(),
    }
}

fn output_directories(plan: &Plan) -> Vec<PathBuf> {
    plan.folders
        .iter()
        .flat_map(|folder| {
            folder
                .actions
                .iter()
                .filter_map(|action| match &action.spec {
                    ActionSpec::Archive(action) => action.output.directory.resolve(&folder.source),
                    ActionSpec::Unsupported(_) => None,
                })
        })
        .collect()
}

fn resource_directory() -> PathBuf {
    std::env::var_os("FOLDRY_RESOURCE_DIR").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../resources"),
        PathBuf::from,
    )
}

fn interrupt_flag() -> Result<Arc<AtomicBool>, CliError> {
    static INTERRUPTED: OnceLock<Arc<AtomicBool>> = OnceLock::new();
    if let Some(flag) = INTERRUPTED.get() {
        return Ok(Arc::clone(flag));
    }
    let flag = Arc::new(AtomicBool::new(false));
    let handler_flag = Arc::clone(&flag);
    ctrlc::set_handler(move || handler_flag.store(true, Ordering::Release))
        .map_err(|error| CliError::internal(format!("cannot install Ctrl+C handler: {error}")))?;
    let _ = INTERRUPTED.set(Arc::clone(&flag));
    Ok(flag)
}

fn print_success(json_output: bool, data: &Value, human: &str) {
    if json_output {
        println!(
            "{}",
            serde_json::to_string(&json!({"version": 1, "ok": true, "data": data}))
                .expect("JSON output")
        );
    } else if !human.is_empty() {
        println!("{human}");
    }
}

fn print_error(json_output: bool, error: &CliError) {
    if json_output {
        println!(
            "{}",
            serde_json::to_string(&json!({
                "version": 1,
                "ok": false,
                "error": {"code": error.code, "message": error.message}
            }))
            .expect("JSON error")
        );
    } else {
        eprintln!("error: {}", error.message);
    }
}

fn ok(data: Value, human: String) -> Result<CommandResult, CliError> {
    Ok(CommandResult {
        exit_code: EXIT_SUCCESS,
        data,
        human,
    })
}

fn parse_profile_id(value: &str) -> Result<ProfileId, CliError> {
    value
        .parse()
        .map_err(|error| CliError::validation(format!("invalid profile ID: {error}")))
}

fn parse_folder_id(value: &str) -> Result<FolderId, CliError> {
    value
        .parse()
        .map_err(|error| CliError::validation(format!("invalid folder ID: {error}")))
}

fn parse_action_id(value: &str) -> Result<ActionId, CliError> {
    value
        .parse()
        .map_err(|error| CliError::validation(format!("invalid action ID: {error}")))
}

fn parse_run_id(value: &str) -> Result<RunId, CliError> {
    value
        .parse()
        .map_err(|error| CliError::validation(format!("invalid run ID: {error}")))
}

fn parse_preset_id(value: &str) -> Result<PresetId, CliError> {
    value
        .parse()
        .map_err(|error| CliError::validation(format!("invalid preset ID: {error}")))
}

fn slug(value: &str) -> String {
    let slug = value
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let slug = slug
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        "profile".into()
    } else {
        slug
    }
}

fn repository_error(error: foldry_application::RepositoryError) -> CliError {
    CliError::io(error.to_string())
}

fn json_error(error: serde_json::Error) -> CliError {
    CliError::internal(error.to_string())
}

impl From<FormatArg> for ArchiveFormat {
    fn from(value: FormatArg) -> Self {
        match value {
            FormatArg::Zip => Self::Zip,
            FormatArg::TarGz => Self::TarGz,
            FormatArg::TarZst => Self::TarZst,
        }
    }
}

impl From<CompressionArg> for CompressionLevel {
    fn from(value: CompressionArg) -> Self {
        match value {
            CompressionArg::Fast => Self::Fast,
            CompressionArg::Balanced => Self::Balanced,
            CompressionArg::Maximum => Self::Maximum,
        }
    }
}

impl From<ConflictArg> for ConflictPolicy {
    fn from(value: ConflictArg) -> Self {
        match value {
            ConflictArg::Skip => Self::Skip,
            ConflictArg::Overwrite => Self::Overwrite,
            ConflictArg::Increment => Self::Increment,
        }
    }
}
