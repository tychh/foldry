# Troubleshooting

Start with:

```bash
foldry config path
foldry config show
```

These commands show the exact configuration, data, cache, and database locations
used by the desktop application and CLI.

## A folder cannot be added

Foldry accepts existing readable directories. It canonicalizes the path, so an
alias of an already remembered folder selects the existing configuration instead
of creating a duplicate.

Check permissions and confirm the path is a directory rather than a file or
broken link.

## Folder size is partial

Size calculation intentionally stops at symlinks, junctions, reparse points,
mounted subtrees, and unreadable entries. A partial result is a warning, not an
estimate of bytes that were not read.

## Preview does not match recent changes

Save the Ignore Profile and action settings, then start Preview again. Any source,
profile, or action change invalidates the cache key.

Execution always rescans. A stale Preview is never used as the run manifest.

## Output validation fails

The output directory cannot equal the source or be inside it. Verify that the
directory exists and is writable, the filename uses only `{folder}` and `{date}`
tokens, and the destination filesystem supports an atomic same-filesystem rename.

## No archive appears with conflict policy `skip`

This is expected when the destination already exists. Skip does not overwrite the
file and does not leave a temporary archive.

Use **increment** to produce a numbered destination or **replace safely** to keep
the old archive until the replacement is verified.

## A run becomes Interrupted after restart

Foldry reconciles unfinished database records at startup. A process that ended
without persisting a terminal state becomes Interrupted. Published archives are
never deleted during this recovery.

## Default profile disappeared

Restart Foldry or run any operation that loads profiles. The shipped Default
working copy is restored automatically. Missing profile references fall back to
Default.

## A folder stays hidden

Use the remembered-folders dialog or:

```bash
foldry folder remembered
```

Adding the same source path again also restores its existing configuration.

## Reset isolated development state

Do not delete normal application data while diagnosing a problem. Start an
isolated CLI layout instead:

```bash
foldry \
  --config-dir /tmp/foldry-test/config \
  --data-dir /tmp/foldry-test/data \
  --cache-dir /tmp/foldry-test/cache \
  config path
```

These override flags are intentionally hidden from normal help.

## Reporting a problem

Include:

- Foldry version and installation type;
- operating system and filesystem;
- the command or UI sequence;
- archive format and conflict/verification settings;
- exported logs or `foldry history logs <run-id>`;
- whether a previous archive, `.part`, or reservation file remained.

Do not attach real secrets or personal source paths to a public issue. Create a
minimal disposable reproduction whenever possible.
