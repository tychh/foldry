# Platform validation matrix

Текущий пооперационный checklist и результаты ручных запусков ведутся в
`notes/multiplatform-smoke.md`. Этот документ описывает стабильную политику
валидации; отметки выполнения не дублируются здесь.

## Automated coverage

`.github/workflows/ci.yml` runs formatting, lint/Clippy, contract drift,
TypeScript, all tests, workspace build, and the Tauri desktop build on
Ubuntu 22.04, `windows-latest`, and `macos-latest`. This proves compilation and
automated behavior on the three targets, but it does not replace interaction
with native dialogs, WebView, filesystem locks, network mounts, signing, or
packaging.

## Local result

On 2026-07-27 the full automated suite, release performance smoke, browser
interaction, light/dark axe WCAG A/AA checks, 400 px reflow, keyboard focus
order, and console checks passed on Linux. The browser had no horizontal page
overflow at 400×700 and exposed the expected task controls in keyboard order.

## Required manual release smoke

Use a non-production temporary source and output directory on each OS.

| Scenario                                                  | Linux                 | Windows                          | macOS            |
| --------------------------------------------------------- | --------------------- | -------------------------------- | ---------------- |
| Launch packaged app, native folder dialog, drag/drop      | Passed locally        | Required                         | Required         |
| ZIP/TAR.GZ/TAR.ZST create, reveal, independent extraction | Automated + local     | Required                         | Required         |
| Pause/resume/stop and restart recovery                    | Automated             | Required                         | Required         |
| Read-only/permission-denied output preserves old archive  | Automated fault paths | Required                         | Required         |
| Source disappears or changes during run                   | Automated             | Required                         | Required         |
| Locked source file                                        | N/A                   | Required                         | N/A              |
| Long path and Unicode normalization                       | Automated fixtures    | Required with long paths enabled | Required         |
| Symlink/junction is stored/skipped without traversal      | Automated symlink     | Required junction/reparse        | Required symlink |
| Local and network/removable output                        | Local required        | Required                         | Required         |
| Light/dark, 200% scaling/HiDPI, keyboard navigation       | Browser passed        | Required native                  | Required native  |
| Installer/signature/notarization and clean uninstall      | N/A for local run     | Required                         | Required         |

For every failure, save the app version, OS build, filesystem type, exact
scenario, logs export, and whether an old archive/temp/reservation remained.
Do not use `overwrite` with valuable data during smoke testing.
