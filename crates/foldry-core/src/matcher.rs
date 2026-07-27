use std::{collections::HashMap, fmt, path::PathBuf};

use ignore::{
    Match,
    gitignore::{Gitignore, GitignoreBuilder, Glob},
};

use crate::{MatchDecision, MatchReason, MatchResult, Profile, ProfileRule};

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
        })
    }

    /// Matches one relative path and returns the exact last effective rule.
    pub fn matched(&self, path: &str, is_dir: bool) -> Result<MatchResult, MatchPathError> {
        let normalized = normalize_relative_path(path)?;
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
