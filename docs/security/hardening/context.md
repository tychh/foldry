# Hardening evidence context

This review used the live workspace at
`/home/tychh/Code/accurate-backuper`. The repository is a from-scratch rewrite
with extensive uncommitted content, so the historical Git revision
`028b0749fa38313bef8ba2a47ab091b1d8b1aee2` is not an immutable identity for the
reviewed source. Source drift is therefore recorded as `present`.

The evidence collection is the ten security-relevant source and contract files
listed below. The collection digest is the SHA-256 of their ordered
`sha256sum` output:
`3bdd1729c77f4618b7d0bd95c289ddfc6ab74548824d71a4aef130eb352baeb4`.

| Evidence | Reader-facing title                | Path                                            | SHA-256                                                            |
| -------- | ---------------------------------- | ----------------------------------------------- | ------------------------------------------------------------------ |
| E001     | Archive execution boundary         | `crates/foldry-core/src/execution.rs`           | `15278a10f17d22f5b933bee5ac91d66e4033ddcfadde0a767a60145760a71bac` |
| E002     | Output reservation and publication | `crates/foldry-core/src/output.rs`              | `070653ce0427d654ea78497b9027d30f3e44256831dfb856eeb88539bf1ff8eb` |
| E003     | Filesystem scanner                 | `crates/foldry-core/src/filesystem.rs`          | `8eb6ba966dbe4ca3a18cafa53668469322d646583e95ac9bb3a5fef53190ca1d` |
| E004     | Private streaming manifest         | `crates/foldry-storage/src/manifest.rs`         | `4ba3677b5df412dec80040832f1ddc801b88e3b58ec0708744bcb252df95f4ae` |
| E005     | Crash reconciliation               | `crates/foldry-storage/src/reconciliation.rs`   | `588a7c3e591a99e94aae5e8172749475f6e312ff935fcc3aba46ba214c550045` |
| E006     | Desktop IPC boundary               | `crates/foldry-tauri/src/ipc.rs`                | `4dd00035dfed1ad404f8e2c554afc430a5ea361a50ac3fce5f8645f986c95d0e` |
| E007     | Tauri capabilities                 | `crates/foldry-tauri/capabilities/default.json` | `e423f3bd9964afb1a79847db7f590727f01c9824b34951178577e76c4ea5f7c1` |
| E008     | Webview CSP and bundle policy      | `crates/foldry-tauri/tauri.conf.json`           | `e5d7bcc9293ea437d6e8dc6c92be99456c4116c4299ea6ad3b7b5d8d56b356d2` |
| E009     | Archive execution contract         | `docs/contracts/archive-execution-v1.md`        | `e9ff5f16c7601121e2883117ec75f9d37bf2f608dcc4e93a0d2467c972b3ced5` |
| E010     | Desktop IPC contract               | `docs/contracts/tauri-ipc-v1.md`                | `7197da70a4d67ee8390fe59f555ef685c8ee660df0138cc55f59f8e677598e98` |

Additional validation evidence:

- six archive execution integration tests, including a manifest escape attempt
  and a scan-to-open symlink replacement;
- 2048 generated profile-parser cases, 2048 YAML cases, and 2048 archive-name
  cases;
- existing reservation, crash recovery, manifest ownership, scheduler stress,
  Unicode, long-entry, and million-entry tests;
- `pnpm audit --prod --audit-level high`: no known production dependency
  vulnerabilities on 2026-07-27;
- axe-core 4.12.1 WCAG A/AA runs in light/dark at 1440×900 and at 400×700.
