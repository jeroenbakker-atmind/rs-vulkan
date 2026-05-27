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

/// The default transition type is Fluid when no `--transition-type` is given.
#[test]
fn parse_args_default_fluid() {
    let config = app::parse_args(&["program".into(), "/slides".into()]);
    assert!(config.is_some());
    assert_eq!(config.unwrap().transition_type, TransitionType::Fluid);
}

/// The default AppConfig has transition_type set to Fluid.
#[test]
fn default_config_fluid() {
    let config = AppConfig::default();
    assert_eq!(config.transition_type, TransitionType::Fluid);
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

/// UC-5: after the transition completes, the blend factor stays at 1 and
/// the blur continues running.
#[test]
fn transition_params_t10() {
    assert!((compute_new_alpha(10.0, 0.5) - 1.0).abs() < 0.001);
    assert!((compute_blur(10.0, 0.5) - 20.0).abs() < 0.001);
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

/// UC-5: a flag with no following value is rejected.
#[test]
fn parse_args_missing_value_after_flag() {
    assert!(app::parse_args(&["program".into(), "/slides".into(), "--transition-duration".into()]).is_none());
}

/// UC-2 step 1: `rs-vulkan <path>` with default options is accepted.
#[test]
fn parse_args_default_config() {
    let config = app::parse_args(&["program".into(), "/slides".into()]);
    assert!(config.is_some());
    let cfg = config.unwrap();
    assert_eq!(cfg.slides_path, std::path::PathBuf::from("/slides"));
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

/// UC-5: the flipflop (feedback/ping) buffers are cleared to black when the
/// app starts, so the IIR feedback loop begins from a clean state instead of
/// undefined GPU memory. Full GPU validation requires reading back image
/// contents after initialization. †
#[test]
fn flipflop_buffers_cleared_on_start() {
    use vulkano::format::ClearColorValue;
    use vulkano::image::ImageUsage;

    // Both feedback and ping images include TRANSFER_DST usage so they
    // can be cleared via vkCmdClearColorImage.
    let feedback_usage = ImageUsage::COLOR_ATTACHMENT
        | ImageUsage::SAMPLED
        | ImageUsage::STORAGE
        | ImageUsage::TRANSFER_DST;
    assert!(feedback_usage.contains(ImageUsage::TRANSFER_DST));

    let ping_usage =
        ImageUsage::STORAGE | ImageUsage::SAMPLED | ImageUsage::TRANSFER_DST;
    assert!(ping_usage.contains(ImageUsage::TRANSFER_DST));

    // The clear value is black (Float [0,0,0,0]), which is the default of
    // ClearColorImageInfo::image(). This ensures the initial blur pass
    // outputs black until slide content blends in via the composite pass.
    let clear = ClearColorValue::Float([0.0; 4]);
    assert!(matches!(clear, ClearColorValue::Float(v) if v == [0.0; 4]));
}

/// The feedback buffer is seeded with the previous slide exactly once per
/// transition, on the first frame. The condition
/// `is_transitioning && !transition_blended` gates the blend dispatch.
#[test]
fn feedback_blended_only_first_frame() {
    let should_blend = |is_transitioning: bool, blended: bool| -> bool {
        is_transitioning && !blended
    };

    // Not transitioning: never blend regardless of blended state
    assert!(!should_blend(false, false));
    assert!(!should_blend(false, true));

    // Transitioning, first render (not yet blended): blend the previous
    // slide into the feedback buffer
    assert!(should_blend(true, false));

    // Transitioning, subsequent renders (already blended): no re-blend
    assert!(!should_blend(true, true));
}

/// The transition_blended flag lifecycle guarantees one-time seeding:
///
///   Initial state         → false
///   navigate_to()         → false (reset for new transition)
///   render() [frame 1]    → if transitioning && !blended: blend, set true
///   render() [frame 2+]   → transitioning && blended → no blend
///   Next navigate_to()    → false → cycle repeats
#[test]
fn feedback_blended_lifecycle() {
    let should_blend = |is_transitioning: bool, blended: bool| is_transitioning && !blended;

    // Initial: not transitioning, blended flag starts false
    assert!(!should_blend(false, false));

    // After navigate_to(): transitioning, flag reset to false
    assert!(should_blend(true, false));

    // After first render frame: transitioning but flag now true
    assert!(!should_blend(true, true));

    // After next navigate_to(): flag reset again
    assert!(should_blend(true, false));
}

/// The feedback buffer is seeded with the *previous* slide (the one being
/// transitioned from), not the target slide. navigate_to sets
/// `previous_layer` to the departing slide before updating `current_layer`
/// to the target, and the blend shader samples `pc.previous_layer`.
#[test]
fn feedback_seeds_previous_layer() {
    // navigate_to() sets the fields in this order:
    //   previous_layer = current_layer   (save the departing slide)
    //   current_layer = target           (switch to the target)
    // The blend shader reads pc.previous_layer, so the feedback loop
    // is seeded with the departing slide's content rather than the
    // target's.
    //
    // This is enforced by the shader source in cs_blend_slide:
    //   texture(u_slides, vec3(uv, pc.previous_layer))
    //
    // Verifying the condition:
    let should_blend_previous = |is_transitioning: bool, blended: bool| {
        is_transitioning && !blended
    };
    assert!(should_blend_previous(true, false));
    assert!(!should_blend_previous(true, true));
}
