# Ignore Profiles

Ignore Profiles decide which source entries participate in Preview and Actions.
They are UTF-8 `.packignore` files with Git-style matching, stable Foldry metadata,
and optional preset blocks.

## A minimal profile

```text
# @profile-id 0190f5f0-7f8b-7d80-a120-4f4f9fe95c20
# @profile-version 1
# @profile-name Example

.DS_Store
Thumbs.db
```

The three metadata declarations are required. The UUID remains stable when the
profile is renamed.

## Rule order

Rules are evaluated from top to bottom. The last matching rule wins. A path that
does not match any rule is included.

```text
# Exclude all log files
*.log

# Keep one important log
!important.log
```

## Syntax

| Pattern     | Meaning                                  |
| ----------- | ---------------------------------------- |
| blank line  | ignored                                  |
| `# comment` | comment                                  |
| `\#literal` | name beginning with `#`                  |
| `!pattern`  | include a previously excluded path again |
| `/target`   | match from the source root               |
| `cache/`    | match directories only                   |
| `*`         | any characters inside one path component |
| `?`         | one character inside a path component    |
| `[a-z]`     | character class                          |
| `**`        | zero or more complete path components    |

Both `/` and `\` are normalized at the platform boundary. Absolute paths and
parent traversal with `..` are rejected.

## Re-including content

A file cannot be re-included while its parent directory remains excluded:

```text
build/
!build/keep.txt
```

The rule above still excludes `build/keep.txt`. Re-include the parent first:

```text
build/
!build/
!build/keep.txt
```

## Presets

Presets insert explicit versioned blocks:

```text
# @preset-begin id=python version=1
__pycache__/
*.py[cod]
# @preset-end id=python
```

Foldry classifies each block as absent, installed, modified, or outdated. A
modified block is never silently overwritten.

Presets marked **Sensitive** ignore secrets, credentials, private keys, dumps, or
other private material. Inserting one needs no confirmation because it removes
matching data from processing. Removing one requires confirmation because the
matching data will become eligible for processing.

## Default behavior

The shipped Default profile ignores only common operating-system metadata. It
does not automatically ignore build output, dependencies, secrets, media, or
repository internals.

Default may be edited but not deleted through the normal desktop UI. If its
working copy disappears, Foldry restores it. A missing profile reference falls
back to Default.

## Validation

The editor reports malformed metadata, unsupported versions, invalid patterns,
duplicate preset IDs, nested blocks, mismatched markers, and unclosed blocks.
Resolve all reported errors before using the profile in Preview or an Action.
