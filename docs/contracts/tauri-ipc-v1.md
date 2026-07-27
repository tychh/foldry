# Tauri IPC contract v1

## Boundary

The desktop webview can access Foldry only through the commands registered in
`foldry-tauri`. Commands expose application use cases and versioned transport DTOs;
storage repositories, SQLite connections, manifest paths and archive reservations
never cross the IPC boundary.

The generated TypeScript definitions in
`frontend/src/shared/contracts/generated.ts` are the source of frontend argument and
result types. `pnpm contracts:check` fails when the committed file is stale.

Errors have the stable shape:

```ts
type IpcError = {
  code: string;
  message: string;
  details: JsonValue | null;
};
```

Known codes include `invalid_request`, `invalid_id`, `invalid_path`, `not_found`,
`conflict`, `invalid_profile`, `filesystem_error`, `storage_error`,
`scheduler_error`, `preview_error`, `desktop_error`, `cancelled` and
`internal_error`.

## Reconnect and events

`bootstrap_snapshot` is the first command after every webview load or reconnect. It
returns contract version 1, current settings and plan, profiles, presets, current
scheduler records, recent history, available preview descriptors, filesystem roots
and application storage paths. Runs live in the Rust process, so reloading the
webview does not stop them.

Scheduler events are emitted as `foldry://run-event`. Every `RunEvent` contains a
`run_id`, `task_id` and monotonically increasing decimal-string `sequence`.
State/final events are immediate; progress remains throttled by the scheduler.
After a listener reconnects it must call `bootstrap_snapshot` before applying newer
events.

Browser and preview responses contain a monotonically increasing `generation`.
Starting a newer request for the same directory or task cancels and supersedes the
previous request.

## Commands

The registered v1 surface is:

- bootstrap and browser: `bootstrap_snapshot`, `browser_roots`,
  `browser_children`, `cancel_browser_request`;
- settings and plan: `save_settings`, `save_plan`;
- tasks and native drops: `add_task`, `add_dropped_sources`, `update_task`,
  `remove_task`;
- profiles: `create_profile`, `rename_profile`, `save_profile`,
  `delete_profile`, `restore_default_profile`;
- presets: `save_preset`, `delete_preset`, `reset_preset`;
- preview: `start_preview`, `preview_page`, `cancel_preview`;
- scheduler: `run_task`, `run_all_enabled`, `repeat_run`,
  `scheduler_snapshot`, `pause_run`, `resume_run`, `stop_run`, `pause_all`,
  `resume_all`, `stop_all`;
- history: `history_page`, `run_details`, `logs_page`, `export_run_logs`;
- desktop integration: `pick_folders`, `reveal_run_output`.

History, logs and preview pages accept between 1 and 1,000 records. A native drop
accepts at most 256 paths and ignores non-directories.

## Path and desktop-action safety

Filesystem paths received over IPC must be absolute, available directories. Rust
canonicalizes task, browser, dialog and drop paths before use. Plan saves also
canonicalize every task source and revalidate referenced profiles.

`reveal_run_output` accepts only a validated UUIDv7 `run_id`. The backend loads the
artifact path from persisted run history, verifies that it is an existing regular
non-symlink file and then asks the operating system to reveal it. The frontend
cannot pass an arbitrary path or shell command.

Folder dialogs and output reveal use the Rust APIs of the Tauri dialog/opener
plugins. Their JavaScript commands are not granted to the webview. Native
drag-and-drop is received by the frontend from Tauri window events, then directory
paths are sent to `add_dropped_sources` for backend validation.

## Capabilities and CSP

The main window capability grants only `core:default`. Dialog and opener guest
permissions are intentionally absent because custom Rust commands mediate those
operations. Automatic opener handling of web links is disabled.

Production HTML uses a restrictive content security policy: scripts and default
content are self-only, objects are disabled, images are limited to application
assets/data, and network connections are limited to Tauri IPC. Development CSP is
unset only for the local Vite development server. Tauri freezes
`Object.prototype` for the production custom protocol.

Bundled resources are mapped explicitly to `resources/profiles` and
`resources/presets`; runtime lookup never depends on accepting a resource path from
the frontend.
