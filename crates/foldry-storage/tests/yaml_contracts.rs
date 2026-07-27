use std::{fs, path::PathBuf};

use foldry_application::{ActionSpec, ContractValidation, ExecutionBlockerCode, ValidationCode};
use foldry_storage::{DocumentError, decode_plan, decode_settings, encode_plan, encode_settings};
use proptest::prelude::*;
use serde_json::Value;

#[test]
fn v1_plan_has_a_stable_golden_round_trip() {
    let source = fixture("formats/v1/plan.packplan.yaml");
    let plan = decode_plan(&source).unwrap();
    let encoded = encode_plan(&plan).unwrap();

    assert_eq!(encoded, source);
    assert!(plan.execution_blockers().is_empty());
}

#[test]
fn v1_settings_have_a_stable_golden_round_trip() {
    let source = fixture("formats/v1/settings.yaml");
    let settings = decode_settings(&source).unwrap();
    let encoded = encode_settings(&settings).unwrap();

    assert_eq!(encoded, source);
    assert!(settings.validate().is_empty());
}

#[test]
fn compatible_unknown_fields_survive_a_semantic_round_trip() {
    let source = fixture("formats/v1/plan-unknown-fields.packplan.yaml");
    let plan = decode_plan(&source).unwrap();
    let encoded = encode_plan(&plan).unwrap();
    let value: Value = serde_yaml_ng::from_str(&encoded).unwrap();

    assert_eq!(value["x_vendor"]["color"], "blue");
    assert_eq!(value["tasks"][0]["x_task_label"], "important");
    assert_eq!(
        value["tasks"][0]["steps"][0]["output"]["x_storage_class"],
        "local"
    );
    assert_eq!(value["tasks"][0]["steps"][0]["x_action_note"], "preserved");
}

#[test]
fn compatible_unknown_settings_fields_survive_a_semantic_round_trip() {
    let source = fixture("formats/v1/settings.yaml").replace(
        "execution:\n  max_parallel_runs: 2",
        "execution:\n  max_parallel_runs: 2\n  x_scheduler_mode: fair",
    );
    let settings = decode_settings(&source).unwrap();
    let encoded = encode_settings(&settings).unwrap();
    let value: Value = serde_yaml_ng::from_str(&encoded).unwrap();

    assert_eq!(value["execution"]["x_scheduler_mode"], "fair");
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 2_048,
        max_shrink_iters: 4_096,
        ..ProptestConfig::default()
    })]

    #[test]
    fn arbitrary_yaml_never_panics_and_successful_decodes_round_trip(
        characters in prop::collection::vec(any::<char>(), 0..4_096),
    ) {
        let source = characters.into_iter().collect::<String>();
        if let Ok(plan) = decode_plan(&source) {
            let encoded = encode_plan(&plan).expect("valid decoded plan must encode");
            prop_assert!(decode_plan(&encoded).is_ok());
        }
        if let Ok(settings) = decode_settings(&source) {
            let encoded = encode_settings(&settings).expect("valid decoded settings must encode");
            prop_assert!(decode_settings(&encoded).is_ok());
        }
    }
}

#[test]
fn unknown_action_is_preserved_but_blocked_from_execution() {
    let source = fixture("formats/v1/plan-unknown-action.packplan.yaml");
    let plan = decode_plan(&source).unwrap();
    let blockers = plan.execution_blockers();
    let encoded = encode_plan(&plan).unwrap();
    let value: Value = serde_yaml_ng::from_str(&encoded).unwrap();

    assert!(matches!(plan.tasks[0].steps[0], ActionSpec::Unsupported(_)));
    assert_eq!(
        blockers[0].code,
        ExecutionBlockerCode::UnsupportedActionType
    );
    assert_eq!(value["tasks"][0]["steps"][0]["type"], "upload");
    assert_eq!(value["tasks"][0]["steps"][0]["provider"], "future-cloud");
    assert_eq!(value["tasks"][0]["steps"][0]["retry"]["count"], 2);
}

#[test]
fn future_document_version_fails_without_panicking() {
    let source = fixture("formats/future/plan-v2.packplan.yaml");
    let error = decode_plan(&source).unwrap_err();

    assert!(matches!(
        error,
        DocumentError::UnsupportedVersion {
            found: 2,
            current: 1,
            ..
        }
    ));
}

#[test]
fn missing_document_version_reports_the_version_path() {
    let error = decode_plan("name: Missing version\ntasks: []\n").unwrap_err();

    assert!(matches!(&error, DocumentError::MissingVersion { .. }));
    assert!(error.to_string().contains("$.version"));
}

#[test]
fn malformed_yaml_reports_line_and_column() {
    let source = fixture("formats/invalid/settings-malformed.yaml");
    let error = decode_settings(&source).unwrap_err();

    match error {
        DocumentError::Syntax {
            line,
            column,
            message,
            ..
        } => {
            assert!(line >= 2);
            assert!(column >= 1);
            assert!(!message.is_empty());
        }
        other => panic!("expected syntax error, received {other:?}"),
    }
}

#[test]
fn invalid_known_field_reports_its_document_path() {
    let source = fixture("formats/invalid/plan-invalid-enum.packplan.yaml");
    let error = decode_plan(&source).unwrap_err();

    match error {
        DocumentError::Decode { path, message, .. } => {
            assert!(path.contains("tasks[0].steps[0]"), "{path}");
            assert!(message.contains("unknown variant `rar`"), "{message}");
        }
        other => panic!("expected field decode error, received {other:?}"),
    }
}

#[test]
fn non_v7_identifier_reports_its_document_path() {
    let source = fixture("formats/v1/plan.packplan.yaml").replace(
        "0190f5f0-7f8b-7d80-a120-4f4f9fe95c21",
        "550e8400-e29b-41d4-a716-446655440000",
    );
    let error = decode_plan(&source).unwrap_err();

    match error {
        DocumentError::Decode { path, message, .. } => {
            assert!(path.contains("tasks[0].id"), "{path}");
            assert!(message.contains("UUIDv7"), "{message}");
        }
        other => panic!("expected identifier decode error, received {other:?}"),
    }
}

#[test]
fn structural_validation_returns_every_duplicate_source() {
    let source = fixture("formats/v1/plan.packplan.yaml");
    let mut plan = decode_plan(&source).unwrap();
    let duplicate = plan.tasks[0].clone();
    plan.tasks.push(duplicate);

    let error = encode_plan(&plan).unwrap_err();
    let issues = error.validation_issues().unwrap();

    assert!(
        issues
            .iter()
            .any(|issue| issue.code == ValidationCode::DuplicateTaskId)
    );
    assert!(
        issues
            .iter()
            .any(|issue| issue.code == ValidationCode::DuplicateSource)
    );
}

fn fixture(relative: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(relative);
    fs::read_to_string(path).unwrap()
}
