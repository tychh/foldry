use std::{fs, path::PathBuf};

use foldry_core::{
    CompiledProfile, FileSystemCaseSensitivity, MatchDecision, ProfileId, parse_profile,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    profiles: Vec<ProfileFixture>,
}

#[derive(Deserialize)]
struct ProfileFixture {
    name: String,
    case_insensitive: bool,
    rules: Vec<String>,
    cases: Vec<MatchFixture>,
}

#[derive(Deserialize)]
struct MatchFixture {
    path: String,
    is_dir: bool,
    decision: String,
    rule: Option<String>,
}

#[test]
fn compatibility_matrix_has_stable_cross_platform_results_and_reasons() {
    let fixture: Fixture = serde_json::from_str(&fixture("matcher-cases.json")).unwrap();

    for profile_fixture in fixture.profiles {
        let text = format!(
            "# @profile-id {}\n# @profile-version 1\n# @profile-name {}\n{}\n",
            ProfileId::new(),
            profile_fixture.name,
            profile_fixture.rules.join("\n")
        );
        let profile = parse_profile(&text).profile.unwrap();
        let case_sensitivity = if profile_fixture.case_insensitive {
            FileSystemCaseSensitivity::Insensitive
        } else {
            FileSystemCaseSensitivity::Sensitive
        };
        let matcher = CompiledProfile::new(&profile, case_sensitivity).unwrap();

        for case in profile_fixture.cases {
            let result = matcher.matched(&case.path, case.is_dir).unwrap();
            let expected = match case.decision.as_str() {
                "include" => MatchDecision::Include,
                "exclude" => MatchDecision::Exclude,
                other => panic!("unknown fixture decision {other}"),
            };
            assert_eq!(
                result.decision, expected,
                "{}: {}",
                profile_fixture.name, case.path
            );
            assert_eq!(
                result
                    .reason
                    .as_ref()
                    .map(|reason| reason.original_rule.as_str()),
                case.rule.as_deref(),
                "{}: {}",
                profile_fixture.name,
                case.path
            );
        }
    }
}

#[test]
fn shipped_default_profile_is_valid_and_executable() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../resources/profiles/default.packignore");
    let source = fs::read_to_string(path).unwrap();
    let parsed = parse_profile(&source);

    assert!(
        parsed.is_valid(),
        "{:?}",
        parsed
            .diagnostics
            .iter()
            .map(|diagnostic| &diagnostic.message)
            .collect::<Vec<_>>()
    );
}

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/profiles")
        .join(name);
    fs::read_to_string(path).unwrap()
}
