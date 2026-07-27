# `.packignore` syntax v1

Foldry profiles are UTF-8 text files with Git-compatible ignore rules, stable
metadata, and optional preset markers. Writers use LF and one final newline.

## Required metadata

The first metadata declarations may be separated by comments or blank lines:

```text
# @profile-id 0190f5f0-7f8b-7d80-a120-4f4f9fe95c20
# @profile-version 1
# @profile-name Default
```

- `profile-id` is UUIDv7 and remains stable across rename.
- `profile-version` is currently `1`.
- `profile-name` is non-empty display text.
- Missing, duplicated, malformed, or unsupported metadata makes the profile invalid.

Invalid text remains saveable by the future repository/autosave layer, but
`parse_profile()` does not return an executable `Profile`.

## Rule syntax

Rules are evaluated from top to bottom. The last effective matching rule decides
the result. A path with no matching rule is included.

| Form              | Meaning                               |
| ----------------- | ------------------------------------- |
| empty line        | ignored                               |
| `# comment`       | comment                               |
| `\#literal`       | pattern beginning with `#`            |
| `!pattern`        | re-include a previously excluded path |
| `\!literal`       | pattern beginning with literal `!`    |
| `/target`         | anchored to the source root           |
| `cache/`          | directory-only match                  |
| `*`               | zero or more non-separator characters |
| `?`               | one non-separator character           |
| `[a-z]`, `[!0-9]` | character class or negated class      |
| `**`              | zero or more complete path components |

Trailing unescaped spaces are ignored as in Git. Backslash escapes a leading `#`
or `!`, a space, or another syntax character. A dangling escape is a parser error.

## Negation and pruning

Foldry follows Git-compatible directory pruning. A file cannot be re-included while
one of its parent directories remains excluded:

```text
build/
!build/keep.txt
```

`build/keep.txt` remains excluded. The parent must be re-included first:

```text
build/
!build/
!build/keep.txt
```

The matcher evaluates ancestors top-down and returns provenance from the effective
rule: profile ID, source line, original text, and preset ID when applicable.

## Preset blocks

Installed presets are explicit, non-nesting blocks:

```text
# @preset-begin id=python version=1
__pycache__/
*.py[cod]
# @preset-end id=python
```

`id` is a stable lowercase ASCII slug. Repeated IDs, nested blocks, mismatched end
markers, and unclosed blocks are errors. Marker lines are excluded from the content
hash.

Content is normalized to LF plus one final newline and hashed with SHA-256.
Comparison against current and historical catalog hashes produces:

- `absent`;
- `installed`;
- `outdated`;
- `modified`.

Insert/update/remove operations return a new complete profile string or an error;
the input is never partially changed. Sensitive presets require explicit approval
before insertion or update. Replacing or deleting a modified block requires a
separate confirmation.

## Paths and filesystem behavior

- Matcher input is a source-relative path.
- Both `\` and `/` are normalized to `/` at the contract boundary.
- `.` components are removed; absolute paths and `..` are rejected.
- Unicode is preserved without case folding or normalization.
- Case sensitivity is selected from the source filesystem, not the host process.
- Symlink traversal is unrelated to matching and is handled by the scanner.

## Differences from `.gitignore`

- Foldry reads one selected profile, not nested ignore files from a Git worktree.
- There is no Git repository discovery, global excludes file, or `.git/info/exclude`.
- Profile and preset marker comments are Foldry extensions.
- Results always include structured provenance and parser diagnostics.
- Filesystem case behavior is supplied explicitly by the scanner.

The compatibility matrix is
[`matcher-cases.json`](../../tests/fixtures/profiles/matcher-cases.json).

## Shipped preset catalog

Resources under `resources/presets/` carry ID, version, name, description, and
`safe` or `sensitive` classification. The loader validates every pattern before
publishing the catalog. Safe presets cover languages, frameworks, IDEs, operating
systems, tests, coverage, and build output. Sensitive presets cover secrets, local
configuration, certificates/keys, database dumps, private media, and deployment
credentials; none is installed into Default automatically.
