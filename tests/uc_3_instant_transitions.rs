mod common;

use rs_vulkan::app::{self, TransitionType};

/// UC-3 precondition: `--transition-type instant` is accepted by parse_args.
#[test]
fn parse_args_transition_type_instant() {
    let config = app::parse_args(&[
        "program".into(), "/slides".into(),
        "--transition-type".into(), "instant".into(),
    ]);
    assert!(config.is_some());
    assert_eq!(config.unwrap().transition_type, TransitionType::Instant);
}

/// UC-3 precondition: alternative argument vector with "rs-vulkan" as program
/// name also parses correctly.
#[test]
fn parse_args_transition_type_instant2() {
    let args = vec![
        "rs-vulkan".to_string(),
        "/path/to/slides".to_string(),
        "--transition-type".to_string(),
        "instant".to_string(),
    ];
    let config = app::parse_args(&args).unwrap();
    assert_eq!(config.transition_type, TransitionType::Instant);
}

/// UC-3: an invalid transition type string is rejected.
#[test]
fn parse_args_transition_type_invalid() {
    let args = vec![
        "rs-vulkan".to_string(),
        "/path/to/slides".to_string(),
        "--transition-type".to_string(),
        "foo".to_string(),
    ];
    assert!(app::parse_args(&args).is_none());
}

/// UC-3: the string "bogus" as transition type is rejected.
#[test]
fn parse_args_transition_type_invalid2() {
    assert!(app::parse_args(&[
        "program".into(), "/slides".into(),
        "--transition-type".into(), "bogus".into(),
    ]).is_none());
}

/// UC-3: TransitionType enum equality and inequality are correct.
#[test]
fn transition_type_eq_ordering() {
    assert_eq!(TransitionType::Smooth, TransitionType::Smooth);
    assert_eq!(TransitionType::Instant, TransitionType::Instant);
    assert_eq!(TransitionType::Slide, TransitionType::Slide);
    assert_ne!(TransitionType::Smooth, TransitionType::Instant);
}

/// UC-3: when `--transition-type` is specified twice, the last value wins.
#[test]
fn parse_args_duplicate_transition_type() {
    let config = app::parse_args(&[
        "program".into(), "/slides".into(),
        "--transition-type".into(), "instant".into(),
        "--transition-type".into(), "slide".into(),
    ]);
    assert!(config.is_some());
    assert_eq!(config.unwrap().transition_type, TransitionType::Slide);
}

/// UC-3: `--transition-type` without a value is rejected.
#[test]
fn parse_args_missing_value_after_flag() {
    assert!(app::parse_args(&["program".into(), "/slides".into(), "--transition-type".into()]).is_none());
}
