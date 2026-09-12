use std::{
    collections::HashMap,
    fmt, fs, io,
    path::{Path, PathBuf},
};

use ignore::{
    Match,
    gitignore::{Gitignore, GitignoreBuilder, Glob},
};
use sha2::{Digest, Sha256};

use crate::{
    EffectiveProfileSnapshot, MatchDecision, MatchReason, MatchResult, Profile, ProfileRule,
    ResolvedGitignore, RuleSource, parse_profile,
};

/// Case behavior supplied by the filesystem adapter.
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileSystemCaseSensitivity {
    Sensitive,
    Insensitive,
}

/// A validated profile compiled for repeated path matching.
pub struct CompiledProfile {
    profile_id: crate::ProfileId,
    matcher: Gitignore,
    rule_by_source: HashMap<PathBuf, ProfileRule>,
    dynamic_matchers: Vec<DynamicMatcher>,
    dynamic_enabled: bool,
}

struct DynamicMatcher {
    directory: PathBuf,
    matcher: Gitignore,
}

impl CompiledProfile {
    /// Compiles rules using the selected source-filesystem case behavior.
    pub fn new(
        profile: &Profile,
        case_sensitivity: FileSystemCaseSensitivity,
    ) -> Result<Self, String> {
        let mut builder = GitignoreBuilder::new("");
        builder
            .case_insensitive(case_sensitivity == FileSystemCaseSensitivity::Insensitive)
            .map_err(|error| error.to_string())?;
        let mut rule_by_source = HashMap::new();

        for (index, rule) in profile.rules.iter().enumerate() {
            let source = PathBuf::from(format!(".foldry-rule-{index}"));
            builder
                .add_line(Some(source.clone()), &rule.original)
                .map_err(|error| error.to_string())?;
            rule_by_source.insert(source, rule.clone());
        }

        let matcher = builder.build().map_err(|error| error.to_string())?;
        Ok(Self {
            profile_id: profile.id,
            matcher,
            rule_by_source,
            dynamic_matchers: Vec::new(),
            dynamic_enabled: false,
        })
    }

    /// Compiles an immutable effective snapshot, including dynamic rule sources.
    pub fn from_snapshot(
        snapshot: &EffectiveProfileSnapshot,
        source: &Path,
        case_sensitivity: FileSystemCaseSensitivity,
    ) -> Result<Self, String> {
        let mut compiled = Self::new(&snapshot.profile, case_sensitivity)?;
        compiled.dynamic_enabled = !snapshot.profile.rule_sources.is_empty();
        for resolved in &snapshot.resolved_gitignores {
            let relative = normalize_relative_path(&resolved.relative_path)
                .map_err(|error| error.to_string())?;
            let relative = PathBuf::from(relative);
            if relative.file_name().is_none_or(|name| name != ".gitignore") {
                return Err("resolved gitignore path must name a .gitignore file".into());
            }
            let directory = relative.parent().unwrap_or_else(|| Path::new(""));
            let mut builder = GitignoreBuilder::new(source.join(directory));
            builder
                .case_insensitive(case_sensitivity == FileSystemCaseSensitivity::Insensitive)
                .map_err(|error| error.to_string())?;
            for line in resolved.contents.lines() {
                builder
                    .add_line(Some(source.join(&relative)), line)
                    .map_err(|error| error.to_string())?;
            }
            compiled.dynamic_matchers.push(DynamicMatcher {
                directory: directory.to_path_buf(),
                matcher: builder.build().map_err(|error| error.to_string())?,
            });
        }
        Ok(compiled)
    }

    #[must_use]
    pub fn requires_full_traversal(&self) -> bool {
        self.dynamic_enabled
    }

    /// Matches one relative path and returns the exact last effective rule.
    pub fn matched(&self, path: &str, is_dir: bool) -> Result<MatchResult, MatchPathError> {
        let normalized = normalize_relative_path(path)?;
        if self.dynamic_enabled && !self.gitignored(&normalized, is_dir) {
            return Ok(MatchResult {
                path: normalized,
                decision: MatchDecision::Exclude,
                reason: None,
            });
        }
        let components = normalized.split('/').collect::<Vec<_>>();
        let ancestor_count = components.len().saturating_sub(1);

        for end in 1..=ancestor_count {
            let ancestor = components[..end].join("/");
            if let Match::Ignore(glob) = self.matcher.matched(&ancestor, true) {
                return Ok(self.result(normalized, MatchDecision::Exclude, glob));
            }
        }

        let result = match self.matcher.matched(&normalized, is_dir) {
            Match::Ignore(glob) => self.result(normalized, MatchDecision::Exclude, glob),
            Match::Whitelist(glob) => self.result(normalized, MatchDecision::Include, glob),
            Match::None => MatchResult {
                path: normalized,
                decision: MatchDecision::Include,
                reason: None,
            },
        };
        Ok(result)
    }

    fn gitignored(&self, normalized: &str, is_dir: bool) -> bool {
        let path = Path::new(normalized);
        let mut ignored = false;
        for dynamic in &self.dynamic_matchers {
            let Ok(relative) = path.strip_prefix(&dynamic.directory) else {
                continue;
            };
            match dynamic
                .matcher
                .matched_path_or_any_parents(relative, is_dir)
            {
                Match::Ignore(_) => ignored = true,
                Match::Whitelist(_) => ignored = false,
                Match::None => {}
            }
        }
        ignored
    }

    fn result(&self, path: String, decision: MatchDecision, glob: &Glob) -> MatchResult {
        let rule = glob
            .from()
            .and_then(|source| self.rule_by_source.get(source));
        MatchResult {
            path,
            decision,
            reason: rule.map(|rule| MatchReason {
                profile_id: self.profile_id,
                line: rule.span.start.line,
                original_rule: rule.original.clone(),
                preset_id: rule.preset_id.clone(),
            }),
        }
    }
}

#[derive(Debug)]
pub enum EffectiveProfileError {
    InvalidProfile(String),
    Io { path: PathBuf, source: io::Error },
    EscapedSource(PathBuf),
}

impl fmt::Display for EffectiveProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProfile(message) => formatter.write_str(message),
            Self::Io { path, source } => {
                write!(formatter, "cannot read {}: {source}", path.display())
            }
            Self::EscapedSource(path) => write!(
                formatter,
                "dynamic rule source escaped Folder source: {}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for EffectiveProfileError {}

/// Resolves all dynamic sources now so queued Runs remain immutable.
pub fn resolve_effective_profile(
    profile_text: &str,
    source: &Path,
) -> Result<EffectiveProfileSnapshot, EffectiveProfileError> {
    let parsed = parse_profile(profile_text);
    let profile = parsed.profile.ok_or_else(|| {
        EffectiveProfileError::InvalidProfile(
            parsed
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.message.as_str())
                .collect::<Vec<_>>()
                .join("; "),
        )
    })?;
    let mut resolved_gitignores = Vec::new();
    if profile.rule_sources.iter().any(|source| {
        matches!(
            source,
            RuleSource::GitignoreExcludeUnignored { nested: true, .. }
        )
    }) {
        collect_gitignores(source, source, &mut resolved_gitignores)?;
        resolved_gitignores.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    }
    let mut hasher = Sha256::new();
    hasher.update(b"foldry-effective-profile\0v1\0");
    hash_field(&mut hasher, profile_text.as_bytes());
    for resolved in &resolved_gitignores {
        hash_field(&mut hasher, resolved.relative_path.as_bytes());
        hash_field(&mut hasher, resolved.contents.as_bytes());
    }
    Ok(EffectiveProfileSnapshot {
        profile,
        resolved_gitignores,
        hash: format!("{:x}", hasher.finalize()),
    })
}

fn collect_gitignores(
    root: &Path,
    directory: &Path,
    result: &mut Vec<ResolvedGitignore>,
) -> Result<(), EffectiveProfileError> {
    let canonical_root = fs::canonicalize(root).map_err(|source| EffectiveProfileError::Io {
        path: root.to_path_buf(),
        source,
    })?;
    let entries = fs::read_dir(directory).map_err(|source| EffectiveProfileError::Io {
        path: directory.to_path_buf(),
        source,
    })?;
    let mut entries =
        entries
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| EffectiveProfileError::Io {
                path: directory.to_path_buf(),
                source,
            })?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|source| EffectiveProfileError::Io {
            path: path.clone(),
            source,
        })?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            let canonical =
                fs::canonicalize(&path).map_err(|source| EffectiveProfileError::Io {
                    path: path.clone(),
                    source,
                })?;
            if !canonical.starts_with(&canonical_root) {
                return Err(EffectiveProfileError::EscapedSource(path));
            }
            collect_gitignores_with_root(&canonical_root, root, &path, result)?;
        } else if metadata.is_file() && entry.file_name() == ".gitignore" {
            let contents =
                fs::read_to_string(&path).map_err(|source| EffectiveProfileError::Io {
                    path: path.clone(),
                    source,
                })?;
            let relative = path
                .strip_prefix(root)
                .map_err(|_| EffectiveProfileError::EscapedSource(path.clone()))?;
            result.push(ResolvedGitignore {
                relative_path: relative.to_string_lossy().replace('\\', "/"),
                contents,
            });
        }
    }
    Ok(())
}

fn collect_gitignores_with_root(
    canonical_root: &Path,
    root: &Path,
    directory: &Path,
    result: &mut Vec<ResolvedGitignore>,
) -> Result<(), EffectiveProfileError> {
    let entries = fs::read_dir(directory).map_err(|source| EffectiveProfileError::Io {
        path: directory.to_path_buf(),
        source,
    })?;
    let mut entries =
        entries
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| EffectiveProfileError::Io {
                path: directory.to_path_buf(),
                source,
            })?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|source| EffectiveProfileError::Io {
            path: path.clone(),
            source,
        })?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            let canonical =
                fs::canonicalize(&path).map_err(|source| EffectiveProfileError::Io {
                    path: path.clone(),
                    source,
                })?;
            if !canonical.starts_with(canonical_root) {
                return Err(EffectiveProfileError::EscapedSource(path));
            }
            collect_gitignores_with_root(canonical_root, root, &path, result)?;
        } else if metadata.is_file() && entry.file_name() == ".gitignore" {
            let contents =
                fs::read_to_string(&path).map_err(|source| EffectiveProfileError::Io {
                    path: path.clone(),
                    source,
                })?;
            let relative = path
                .strip_prefix(root)
                .map_err(|_| EffectiveProfileError::EscapedSource(path.clone()))?;
            result.push(ResolvedGitignore {
                relative_path: relative.to_string_lossy().replace('\\', "/"),
                contents,
            });
        }
    }
    Ok(())
}

fn hash_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_le_bytes());
    hasher.update(value);
}

/// Invalid path at the normalized matcher boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MatchPathError {
    Absolute,
    ParentTraversal,
    Empty,
}

impl fmt::Display for MatchPathError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Absolute => formatter.write_str("match path must be relative"),
            Self::ParentTraversal => {
                formatter.write_str("match path must not contain parent traversal")
            }
            Self::Empty => formatter.write_str("match path must not be empty"),
        }
    }
}

impl std::error::Error for MatchPathError {}

/// Normalizes Windows and POSIX separators to the public `/` representation.
pub fn normalize_relative_path(path: &str) -> Result<String, MatchPathError> {
    let replaced = path.replace('\\', "/");
    if replaced.starts_with('/')
        || replaced
            .as_bytes()
            .get(1)
            .is_some_and(|separator| *separator == b':')
    {
        return Err(MatchPathError::Absolute);
    }

    let mut components = Vec::new();
    for component in replaced.split('/') {
        match component {
            "" | "." => {}
            ".." => return Err(MatchPathError::ParentTraversal),
            value => components.push(value),
        }
    }
    if components.is_empty() {
        return Err(MatchPathError::Empty);
    }
    Ok(components.join("/"))
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use tempfile::tempdir;

    use crate::{ProfileFormatVersion, ProfileId, parse_profile};

    use super::*;

    fn compiled(rules: &str, case: FileSystemCaseSensitivity) -> CompiledProfile {
        let text = format!(
            "# @profile-id {}\n# @profile-version 1\n# @profile-name Test\n{rules}",
            ProfileId::new()
        );
        let profile = parse_profile(&text).profile.unwrap();
        assert_eq!(profile.version, ProfileFormatVersion::V1);
        CompiledProfile::new(&profile, case).unwrap()
    }

    #[test]
    fn last_matching_rule_wins() {
        let matcher = compiled(
            "*.log\n!important.log\n",
            FileSystemCaseSensitivity::Sensitive,
        );

        assert_eq!(
            matcher.matched("debug.log", false).unwrap().decision,
            MatchDecision::Exclude
        );
        let included = matcher.matched("important.log", false).unwrap();
        assert_eq!(included.decision, MatchDecision::Include);
        assert_eq!(included.reason.unwrap().line, 5);
    }

    #[test]
    fn excluded_parent_must_be_reincluded_before_a_child() {
        let blocked = compiled(
            "build/\n!build/keep.txt\n",
            FileSystemCaseSensitivity::Sensitive,
        );
        assert_eq!(
            blocked.matched("build/keep.txt", false).unwrap().decision,
            MatchDecision::Exclude
        );

        let allowed = compiled(
            "build/\n!build/\n!build/keep.txt\n",
            FileSystemCaseSensitivity::Sensitive,
        );
        assert_eq!(
            allowed.matched("build/keep.txt", false).unwrap().decision,
            MatchDecision::Include
        );
    }

    #[test]
    fn anchored_directory_and_double_star_patterns_work() {
        let matcher = compiled(
            "/target/\n**/cache/*.tmp\n",
            FileSystemCaseSensitivity::Sensitive,
        );

        assert_eq!(
            matcher.matched("target/file", false).unwrap().decision,
            MatchDecision::Exclude
        );
        assert_eq!(
            matcher
                .matched("nested/target/file", false)
                .unwrap()
                .decision,
            MatchDecision::Include
        );
        assert_eq!(
            matcher
                .matched("a/cache/result.tmp", false)
                .unwrap()
                .decision,
            MatchDecision::Exclude
        );
    }

    #[test]
    fn case_behavior_is_selected_from_the_source_filesystem() {
        let sensitive = compiled("README.md\n", FileSystemCaseSensitivity::Sensitive);
        let insensitive = compiled("README.md\n", FileSystemCaseSensitivity::Insensitive);

        assert_eq!(
            sensitive.matched("readme.md", false).unwrap().decision,
            MatchDecision::Include
        );
        assert_eq!(
            insensitive.matched("readme.md", false).unwrap().decision,
            MatchDecision::Exclude
        );
    }

    #[test]
    fn dynamic_gitignore_inverts_git_selection_and_honors_nested_negation() {
        let source = tempdir().unwrap();
        std::fs::create_dir_all(source.path().join("nested")).unwrap();
        std::fs::write(source.path().join(".gitignore"), "*.log\nnested/*.tmp\n").unwrap();
        std::fs::write(source.path().join("nested/.gitignore"), "!keep.tmp\n").unwrap();
        let text = format!(
            "# @profile-id {}\n# @profile-version 2\n# @profile-name Development\n\
             # @rule-source gitignore mode=exclude-unignored nested=true\n",
            ProfileId::new()
        );
        let snapshot = resolve_effective_profile(&text, source.path()).unwrap();
        let matcher = CompiledProfile::from_snapshot(
            &snapshot,
            source.path(),
            FileSystemCaseSensitivity::Sensitive,
        )
        .unwrap();

        assert_eq!(
            matcher.matched("debug.log", false).unwrap().decision,
            MatchDecision::Include
        );
        assert_eq!(
            matcher.matched("readme.md", false).unwrap().decision,
            MatchDecision::Exclude
        );
        assert_eq!(
            matcher.matched("nested/drop.tmp", false).unwrap().decision,
            MatchDecision::Include
        );
        assert_eq!(
            matcher.matched("nested/keep.tmp", false).unwrap().decision,
            MatchDecision::Exclude
        );
        assert!(matcher.requires_full_traversal());
    }

    #[test]
    fn dynamic_snapshot_hash_changes_with_gitignore_contents() {
        let source = tempdir().unwrap();
        let text = format!(
            "# @profile-id {}\n# @profile-version 2\n# @profile-name Development\n\
             # @rule-source gitignore mode=exclude-unignored nested=true\n",
            ProfileId::new()
        );
        std::fs::write(source.path().join(".gitignore"), "target/\n").unwrap();
        let before = resolve_effective_profile(&text, source.path()).unwrap();
        std::fs::write(source.path().join(".gitignore"), "dist/\n").unwrap();
        let after = resolve_effective_profile(&text, source.path()).unwrap();

        assert_ne!(before.hash, after.hash);
        assert_eq!(before.resolved_gitignores[0].contents, "target/\n");
    }

    #[test]
    fn separators_and_unicode_are_normalized() {
        let matcher = compiled("данные/**\n", FileSystemCaseSensitivity::Sensitive);
        let result = matcher.matched(r".\данные\отчёт.txt", false).unwrap();

        assert_eq!(result.path, "данные/отчёт.txt");
        assert_eq!(result.decision, MatchDecision::Exclude);
    }

    proptest! {
        #[test]
        fn normalization_is_idempotent(
            components in prop::collection::vec("[a-zA-Z0-9_-]{1,12}", 1..8)
        ) {
            let input = components.join("\\");
            let normalized = normalize_relative_path(&input).unwrap();

            prop_assert_eq!(normalize_relative_path(&normalized).unwrap(), normalized);
        }
    }
}
