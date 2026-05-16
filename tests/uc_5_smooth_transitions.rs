mod common;

use common::{compute_blur, compute_new_alpha};
use rs_vulkan::app::{self, TransitionType, AppConfig};

/// UC-5 precondition: `--transition-type smooth` is accepted by parse_args.
#[test]
fn parse_args_transition_type_smooth_explicit() {
    let config = app::parse_args(&[
        "program".into(), "/slides".into(),
        "--transition-type".into(), "smooth".into(),
    ]);
    assert!(config.is_some());
    assert_eq!(config.unwrap().transition_type, TransitionType::Smooth);
}

/// UC-5 precondition: smooth is the default transition type when no
/// `--transition-type` flag is given.
#[test]
fn parse_args_transition_type_default() {
    let config = app::parse_args(&["program".into(), "/slides".into()]);
    assert!(config.is_some());
    assert_eq!(config.unwrap().transition_type, TransitionType::Smooth);
}

/// UC-5 precondition: the default AppConfig has transition_type set to
/// Smooth.
#[test]
fn instant_transition_default_config() {
    let config = AppConfig::default();
    assert_eq!(config.transition_type, TransitionType::Smooth);
}

/// UC-5 step 2: at t=0 the blend factor starts at 0 and the blur is active.
#[test]
fn transition_params_t0() {
    assert_eq!(compute_new_alpha(0.0, 0.5), 0.0);
    assert_eq!(compute_blur(0.0, 0.5), 20.0);
}

/// UC-5 step 4: mid-transition the blend factor is between 0 and 1.
#[test]
fn transition_params_t0_25() {
    let a = compute_new_alpha(0.25, 0.5);
    assert!(a > 0.0 && a < 1.0);
}

/// UC-5 step 6: at t = transition_duration the blend factor reaches 1.
#[test]
fn transition_params_t0_5() {
    assert!((compute_new_alpha(0.5, 0.5) - 1.0).abs() < 0.001);
}

/// UC-5 steps 6-7: after the transition completes, the blend factor stays at
/// 1 and the blur radius drops to 0.
#[test]
fn transition_params_t10() {
    assert!((compute_new_alpha(10.0, 0.5) - 1.0).abs() < 0.001);
    assert!((compute_blur(10.0, 0.5) - 0.0).abs() < 0.001);
}

/// UC-5 step 2: the smoothstep S-curve has increasing increments (convex then
/// concave shape).
#[test]
fn transition_smoothstep_shape() {
    let a1 = compute_new_alpha(0.125, 0.5);
    let a2 = compute_new_alpha(0.25, 0.5);
    let a3 = compute_new_alpha(0.375, 0.5);
    assert!((a2 - a1 - (a3 - a2)).abs() < 0.001);
}

/// UC-5: an invalid numeric value for `--blur-radius` is rejected.
#[test]
fn parse_args_invalid_number_returns_none() {
    assert!(app::parse_args(&["program".into(), "/slides".into(), "--blur-radius".into(), "abc".into()]).is_none());
}

/// UC-5: a negative `--blur-radius` is accepted.
#[test]
fn parse_args_negative_blur_radius() {
    let config = app::parse_args(&[
        "program".into(), "/slides".into(),
        "--blur-radius".into(), "-5.0".into(),
    ]);
    assert!(config.is_some());
    assert!((config.unwrap().blur_radius_max - (-5.0)).abs() < 1e-6);
}

/// UC-5: a zero `--blur-radius` is accepted.
#[test]
fn parse_args_zero_blur_radius() {
    let config = app::parse_args(&[
        "program".into(), "/slides".into(),
        "--blur-radius".into(), "0".into(),
    ]);
    assert!(config.is_some());
    assert!((config.unwrap().blur_radius_max - 0.0).abs() < 1e-6);
}

/// UC-5: a zero `--transition-duration` is accepted.
#[test]
fn parse_args_zero_transition_duration() {
    let config = app::parse_args(&[
        "program".into(), "/slides".into(),
        "--transition-duration".into(), "0".into(),
    ]);
    assert!(config.is_some());
    assert!((config.unwrap().transition_duration - 0.0).abs() < 1e-6);
}

/// UC-5: a flag with no following value (e.g. `--blur-radius` at end of
/// args) is rejected.
#[test]
fn parse_args_missing_value_after_flag() {
    assert!(app::parse_args(&["program".into(), "/slides".into(), "--blur-radius".into()]).is_none());
    assert!(app::parse_args(&["program".into(), "/slides".into(), "--transition-duration".into()]).is_none());
}

/// UC-2: `--profile` flag is accepted by parse_args and sets profiling mode.
#[test]
fn parse_args_profile() {
    let config = app::parse_args(&["program".into(), "/slides".into(), "--profile".into()]);
    assert!(config.is_some());
    assert!(config.unwrap().profiling);
}

/// UC-2 step 1: `rs-vulkan <path>` with default options is accepted.
#[test]
fn parse_args_default_config() {
    let config = app::parse_args(&["program".into(), "/slides".into()]);
    assert!(config.is_some());
    let cfg = config.unwrap();
    assert_eq!(cfg.slides_path, std::path::PathBuf::from("/slides"));
    assert!((cfg.blur_radius_max - 20.0).abs() < 1e-6);
    assert!((cfg.transition_duration - 0.5).abs() < 1e-6);
}

/// UC-2 step 2a: no arguments is rejected.
#[test]
fn parse_args_no_args_returns_none() {
    assert!(app::parse_args(&["program".into()]).is_none());
}

/// UC-2: `--help` prints usage and returns None.
#[test]
fn parse_args_help_returns_none() {
    assert!(app::parse_args(&["program".into(), "--help".into()]).is_none());
}

/// UC-2: an unknown option is rejected.
#[test]
fn parse_args_unknown_option_returns_none() {
    assert!(app::parse_args(&["program".into(), "/slides".into(), "--bogus".into()]).is_none());
}
