use std::{collections::HashSet, path::PathBuf};

use ignore::gitignore::GitignoreBuilder;

use crate::{
    DiagnosticCode, DiagnosticSeverity, Extensions, ParserDiagnostic, PresetId, Profile,
    ProfileFormatVersion, ProfileId, ProfileRule, RulePattern, SourceLocation, SourceSpan,
};

/// Result of parsing editable profile text. Invalid input keeps diagnostics but is
/// never exposed as an executable profile.
#[derive(Clone, Debug)]
pub struct ProfileParseResult {
    pub profile: Option<Profile>,
    pub diagnostics: Vec<ParserDiagnostic>,
}

impl ProfileParseResult {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.profile.is_some()
    }
}

/// Parses profile metadata, preset markers, and Git-compatible rules.
#[must_use]
pub fn parse_profile(text: &str) -> ProfileParseResult {
    let mut diagnostics = Vec::new();
    let mut profile_id = None;
    let mut profile_version = None;
    let mut profile_name = None;
    let mut rules = Vec::new();
    let mut current_preset: Option<(PresetId, u16, u32)> = None;
    let mut seen_presets = HashSet::new();

    for (zero_index, raw_line) in text.lines().enumerate() {
        let line_number = u32::try_from(zero_index + 1).unwrap_or(u32::MAX);
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);

        if let Some(value) = line.strip_prefix("# @profile-id ") {
            if profile_id.is_some() {
                diagnostics.push(diagnostic(
                    DiagnosticCode::DuplicateMetadata,
                    "profile id is declared more than once",
                    line_number,
                    line,
                ));
            } else {
                match value.trim().parse::<ProfileId>() {
                    Ok(value) => profile_id = Some(value),
                    Err(error) => diagnostics.push(diagnostic(
                        DiagnosticCode::InvalidMetadata,
                        format!("invalid profile id: {error}"),
                        line_number,
                        line,
                    )),
                }
            }
            continue;
        }

        if let Some(value) = line.strip_prefix("# @profile-version ") {
            if profile_version.is_some() {
                diagnostics.push(diagnostic(
                    DiagnosticCode::DuplicateMetadata,
                    "profile version is declared more than once",
                    line_number,
                    line,
                ));
            } else {
                match value.trim().parse::<u16>() {
                    Ok(value) if value == ProfileFormatVersion::CURRENT.0 => {
                        profile_version = Some(ProfileFormatVersion(value));
                    }
                    Ok(value) => diagnostics.push(diagnostic(
                        DiagnosticCode::InvalidMetadata,
                        format!(
                            "profile version {value} is unsupported; current version is {}",
                            ProfileFormatVersion::CURRENT.0
                        ),
                        line_number,
                        line,
                    )),
                    Err(error) => diagnostics.push(diagnostic(
                        DiagnosticCode::InvalidMetadata,
                        format!("invalid profile version: {error}"),
                        line_number,
                        line,
                    )),
                }
            }
            continue;
        }

        if let Some(value) = line.strip_prefix("# @profile-name ") {
            if profile_name.is_some() {
                diagnostics.push(diagnostic(
                    DiagnosticCode::DuplicateMetadata,
                    "profile name is declared more than once",
                    line_number,
                    line,
                ));
            } else if value.trim().is_empty() {
                diagnostics.push(diagnostic(
                    DiagnosticCode::InvalidMetadata,
                    "profile name must not be empty",
                    line_number,
                    line,
                ));
            } else {
                profile_name = Some(value.trim().to_owned());
            }
            continue;
        }

        if let Some(attributes) = line.strip_prefix("# @preset-begin ") {
            match parse_preset_begin(attributes) {
                Ok((preset_id, version)) => {
                    if current_preset.is_some() {
                        diagnostics.push(diagnostic(
                            DiagnosticCode::DuplicatePresetBlock,
                            "preset blocks cannot be nested",
                            line_number,
                            line,
                        ));
                    } else if !seen_presets.insert(preset_id.clone()) {
                        diagnostics.push(diagnostic(
                            DiagnosticCode::DuplicatePresetBlock,
                            format!("preset `{preset_id}` occurs more than once"),
                            line_number,
                            line,
                        ));
                    } else {
                        current_preset = Some((preset_id, version, line_number));
                    }
                }
                Err(message) => diagnostics.push(diagnostic(
                    DiagnosticCode::InvalidMetadata,
                    message,
                    line_number,
                    line,
                )),
            }
            continue;
        }

        if let Some(attributes) = line.strip_prefix("# @preset-end ") {
            let end_id = parse_preset_end(attributes);
            match (&current_preset, end_id) {
                (Some((begin_id, _, _)), Ok(end_id)) if *begin_id == end_id => {
                    current_preset = None;
                }
                (Some((begin_id, _, _)), Ok(end_id)) => diagnostics.push(diagnostic(
                    DiagnosticCode::UnterminatedPresetBlock,
                    format!("preset block `{begin_id}` is closed as `{end_id}`"),
                    line_number,
                    line,
                )),
                (None, Ok(end_id)) => diagnostics.push(diagnostic(
                    DiagnosticCode::UnterminatedPresetBlock,
                    format!("preset end marker `{end_id}` has no matching begin marker"),
                    line_number,
                    line,
                )),
                (_, Err(message)) => diagnostics.push(diagnostic(
                    DiagnosticCode::InvalidMetadata,
                    message,
                    line_number,
                    line,
                )),
            }
            continue;
        }

        if line.trim().is_empty() || (line.starts_with('#') && !line.starts_with("\\#")) {
            continue;
        }

        let mut builder = GitignoreBuilder::new("");
        if let Err(error) =
            builder.add_line(Some(PathBuf::from(format!("line-{line_number}"))), line)
        {
            diagnostics.push(diagnostic(
                classify_rule_error(&error.to_string()),
                error.to_string(),
                line_number,
                line,
            ));
            continue;
        }

        rules.push(ProfileRule {
            pattern: rule_pattern(line),
            original: line.to_owned(),
            span: line_span(line_number, line),
            preset_id: current_preset.as_ref().map(|(id, _, _)| id.clone()),
            extensions: Extensions::new(),
        });
    }

    if let Some((preset_id, _, begin_line)) = current_preset {
        diagnostics.push(ParserDiagnostic {
            code: DiagnosticCode::UnterminatedPresetBlock,
            severity: DiagnosticSeverity::Error,
            message: format!("preset block `{preset_id}` has no end marker"),
            span: Some(SourceSpan {
                start: SourceLocation {
                    line: begin_line,
                    column: 1,
                },
                end: SourceLocation {
                    line: begin_line,
                    column: 1,
                },
            }),
            extensions: Extensions::new(),
        });
    }

    if profile_id.is_none() {
        diagnostics.push(missing_metadata("profile-id"));
    }
    if profile_version.is_none() {
        diagnostics.push(missing_metadata("profile-version"));
    }
    if profile_name.is_none() {
        diagnostics.push(missing_metadata("profile-name"));
    }

    let has_errors = diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error);
    let profile = if has_errors {
        None
    } else {
        Some(Profile {
            version: profile_version.expect("validated profile version"),
            id: profile_id.expect("validated profile id"),
            name: profile_name.expect("validated profile name"),
            rules,
            extensions: Extensions::new(),
        })
    };

    ProfileParseResult {
        profile,
        diagnostics,
    }
}

fn parse_preset_begin(attributes: &str) -> Result<(PresetId, u16), String> {
    let mut id = None;
    let mut version = None;
    for attribute in attributes.split_ascii_whitespace() {
        if let Some(value) = attribute.strip_prefix("id=") {
            id = Some(
                value
                    .parse()
                    .map_err(|error| format!("invalid preset id: {error}"))?,
            );
        } else if let Some(value) = attribute.strip_prefix("version=") {
            version = Some(
                value
                    .parse::<u16>()
                    .map_err(|error| format!("invalid preset version: {error}"))?,
            );
        } else {
            return Err(format!("unknown preset marker attribute `{attribute}`"));
        }
    }
    match (id, version) {
        (Some(id), Some(version)) if version > 0 => Ok((id, version)),
        (Some(_), Some(_)) => Err("preset version must be greater than zero".into()),
        _ => Err("preset begin marker requires `id` and `version`".into()),
    }
}

fn parse_preset_end(attributes: &str) -> Result<PresetId, String> {
    let Some(value) = attributes.trim().strip_prefix("id=") else {
        return Err("preset end marker requires `id`".into());
    };
    if value.contains(char::is_whitespace) {
        return Err("preset end marker accepts only `id`".into());
    }
    value
        .parse()
        .map_err(|error| format!("invalid preset id: {error}"))
}

fn rule_pattern(line: &str) -> RulePattern {
    let escaped_prefix = line.starts_with("\\!") || line.starts_with("\\#");
    let negated = !escaped_prefix && line.starts_with('!');
    let without_negation = if negated { &line[1..] } else { line };
    RulePattern {
        value: without_negation
            .trim_start_matches('/')
            .trim_end_matches('/')
            .to_owned(),
        negated,
        anchored: without_negation.starts_with('/'),
        directory_only: line.ends_with('/') && !line.ends_with("\\/"),
    }
}

fn classify_rule_error(message: &str) -> DiagnosticCode {
    if message.contains("unclosed character class") {
        DiagnosticCode::UnterminatedCharacterClass
    } else if message.contains("escape") || message.contains("dangling '\\'") {
        DiagnosticCode::InvalidEscape
    } else {
        DiagnosticCode::InvalidRule
    }
}

fn diagnostic(
    code: DiagnosticCode,
    message: impl Into<String>,
    line_number: u32,
    line: &str,
) -> ParserDiagnostic {
    ParserDiagnostic {
        code,
        severity: DiagnosticSeverity::Error,
        message: message.into(),
        span: Some(line_span(line_number, line)),
        extensions: Extensions::new(),
    }
}

fn missing_metadata(name: &str) -> ParserDiagnostic {
    ParserDiagnostic {
        code: DiagnosticCode::InvalidMetadata,
        severity: DiagnosticSeverity::Error,
        message: format!("required metadata `@{name}` is missing"),
        span: None,
        extensions: Extensions::new(),
    }
}

fn line_span(line_number: u32, line: &str) -> SourceSpan {
    SourceSpan {
        start: SourceLocation {
            line: line_number,
            column: 1,
        },
        end: SourceLocation {
            line: line_number,
            column: u32::try_from(line.chars().count() + 1).unwrap_or(u32::MAX),
        },
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    const ID: &str = "0190f5f0-7f8b-7d80-a120-4f4f9fe95c20";

    #[test]
    fn parses_metadata_rules_and_preset_provenance() {
        let text = format!(
            "# @profile-id {ID}\n# @profile-version 1\n# @profile-name Default\n\
             # @preset-begin id=python version=1\n__pycache__/\n*.py[cod]\n\
             # @preset-end id=python\n!important.py\n"
        );
        let result = parse_profile(&text);
        let profile = result.profile.unwrap();

        assert_eq!(profile.rules.len(), 3);
        assert_eq!(
            profile.rules[0].preset_id.as_ref().unwrap().as_str(),
            "python"
        );
        assert!(profile.rules[2].pattern.negated);
        assert_eq!(profile.rules[0].span.start.line, 5);
    }

    #[test]
    fn invalid_rule_returns_diagnostic_and_no_executable_profile() {
        let text = format!(
            "# @profile-id {ID}\n# @profile-version 1\n# @profile-name Broken\ndangling\\\n"
        );
        let result = parse_profile(&text);

        assert!(!result.is_valid());
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidEscape)
        );
    }

    #[test]
    fn duplicate_preset_block_is_invalid() {
        let text = format!(
            "# @profile-id {ID}\n# @profile-version 1\n# @profile-name Broken\n\
             # @preset-begin id=python version=1\n*.pyc\n# @preset-end id=python\n\
             # @preset-begin id=python version=1\n*.pyo\n# @preset-end id=python\n"
        );
        let result = parse_profile(&text);

        assert!(!result.is_valid());
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::DuplicatePresetBlock)
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 2_048,
            max_shrink_iters: 4_096,
            ..ProptestConfig::default()
        })]

        #[test]
        fn arbitrary_profile_text_never_panics_or_emits_unbounded_diagnostics(
            characters in prop::collection::vec(any::<char>(), 0..4_096),
        ) {
            let text = characters.into_iter().collect::<String>();
            let line_count = text.lines().count().max(1);
            let parsed = parse_profile(&text);
            prop_assert!(parsed.diagnostics.len() <= line_count.saturating_add(4));
        }
    }
}
