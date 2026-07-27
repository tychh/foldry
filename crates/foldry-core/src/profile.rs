use serde::{Deserialize, Serialize};

use crate::{Extensions, PresetId, ProfileId, SourceSpan};

/// Version of profile metadata and parser semantics.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ProfileFormatVersion(pub u16);

impl ProfileFormatVersion {
    pub const V1: Self = Self(1);
    pub const CURRENT: Self = Self::V1;
}

impl Default for ProfileFormatVersion {
    fn default() -> Self {
        Self::CURRENT
    }
}

/// Parsed representation of one editable `.packignore` file.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Profile {
    pub version: ProfileFormatVersion,
    pub id: ProfileId,
    pub name: String,
    pub rules: Vec<ProfileRule>,
    #[serde(default, skip_serializing_if = "Extensions::is_empty", flatten)]
    pub extensions: Extensions,
}

/// Pattern semantics extracted from one profile line.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RulePattern {
    pub value: String,
    pub negated: bool,
    pub anchored: bool,
    pub directory_only: bool,
}

/// One parsed filtering rule with provenance back to its source text.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ProfileRule {
    pub pattern: RulePattern,
    pub original: String,
    pub span: SourceSpan,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset_id: Option<PresetId>,
    #[serde(default, skip_serializing_if = "Extensions::is_empty", flatten)]
    pub extensions: Extensions,
}

/// Final inclusion decision after applying all matching rules.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchDecision {
    Include,
    Exclude,
}

/// Exact last rule responsible for a match decision.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MatchReason {
    pub profile_id: ProfileId,
    pub line: u32,
    pub original_rule: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset_id: Option<PresetId>,
}

/// Explainable matching result for one normalized relative path.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MatchResult {
    pub path: String,
    pub decision: MatchDecision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<MatchReason>,
}
