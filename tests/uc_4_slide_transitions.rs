mod common;

use common::compute_slide_offset;
use rs_vulkan::app::{self, TransitionType};

/// UC-4 precondition: `--transition-type slide` is accepted by parse_args.
#[test]
fn parse_args_transition_type_slide() {
    let config = app::parse_args(&[
        "program".into(), "/slides".into(),
        "--transition-type".into(), "slide".into(),
    ]);
    assert!(config.is_some());
    assert_eq!(config.unwrap().transition_type, TransitionType::Slide);
}

/// UC-4 precondition: `--blur-radius` and `--transition-duration` custom
/// values are accepted.
#[test]
fn parse_args_custom_values() {
    let args: Vec<String> = vec![
        "program".into(), "/slides".into(),
        "--blur-radius".into(), "30.0".into(),
        "--transition-duration".into(), "1.0".into(),
    ];
    let config = app::parse_args(&args);
    assert!(config.is_some());
    let cfg = config.unwrap();
    assert!((cfg.blur_radius_max - 30.0).abs() < 1e-6);
    assert!((cfg.transition_duration - 1.0).abs() < 1e-6);
}

/// UC-4 step 3: slide offset starts at the full direction vector when the
/// transition begins (t=0).
#[test]
fn slide_offset_starts_at_direction() {
    let (ox, oy) = compute_slide_offset(0.0, 10.0, (0.0, 1.0));
    assert!((ox - 0.0).abs() < 0.001);
    assert!((oy - 1.0).abs() < 0.001);
}

/// UC-4 step 5: slide offset reaches (0, 0) when the transition completes.
#[test]
fn slide_offset_ends_at_zero() {
    let (ox, oy) = compute_slide_offset(10.0, 10.0, (0.0, 1.0));
    assert!((ox - 0.0).abs() < 0.001);
    assert!((oy - 0.0).abs() < 0.001);
}

/// UC-4 step 2: a chapter-next navigation uses direction (1.0, 0.0).
#[test]
fn slide_offset_chapter_direction() {
    let (ox, oy) = compute_slide_offset(0.0, 10.0, (1.0, 0.0));
    assert!((ox - 1.0).abs() < 0.001);
    assert!((oy - 0.0).abs() < 0.001);
}

/// UC-4 step 2: a previous-slide navigation uses direction (0.0, -1.0).
#[test]
fn slide_offset_prev_direction() {
    let (ox, oy) = compute_slide_offset(0.0, 10.0, (0.0, -1.0));
    assert!((ox - 0.0).abs() < 0.001);
    assert!((oy - (-1.0)).abs() < 0.001);
}

/// UC-4 step 2: a previous-chapter navigation uses direction (-1.0, 0.0).
#[test]
fn slide_offset_prev_chapter_direction() {
    let (ox, oy) = compute_slide_offset(0.0, 10.0, (-1.0, 0.0));
    assert!((ox - (-1.0)).abs() < 0.001);
    assert!((oy - 0.0).abs() < 0.001);
}

/// UC-4 step 3: the cubic ease-out curve produces the expected offset mid-way
/// through the transition.
#[test]
fn slide_ease_out_midway() {
    let u = 5.0 / 10.0;
    let ease_out = 1.0 - (1.0 - u) * (1.0 - u) * (1.0 - u);
    let (ox, oy) = compute_slide_offset(5.0, 10.0, (0.0, 1.0));
    assert!((ox - 0.0).abs() < 0.001);
    assert!((oy - (1.0 - ease_out)).abs() < 0.001);
}
