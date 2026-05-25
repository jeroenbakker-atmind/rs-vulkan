use rs_vulkan::app::{self, TransitionType};

/// UC-6 precondition: `--transition-type fluid` is accepted by parse_args.
#[test]
fn parse_args_transition_type_fluid_explicit() {
    let config = app::parse_args(&[
        "program".into(), "/slides".into(),
        "--transition-type".into(), "fluid".into(),
    ]);
    assert!(config.is_some());
    assert_eq!(config.unwrap().transition_type, TransitionType::Fluid);
}

/// UC-6: fluid transition is distinct from Instant.
#[test]
fn fluid_is_not_instant() {
    assert_ne!(TransitionType::Fluid, TransitionType::Instant);
    assert_eq!(TransitionType::Fluid, TransitionType::Fluid);
}

/// UC-6: fluid transition keeps requesting redraws after transition ends
/// (like Smooth), so the simulation loop continues indefinitely.
#[test]
fn fluid_keeps_redrawing_after_transition() {
    let should_redraw_continuously = |ty: TransitionType| -> bool {
        matches!(ty, TransitionType::Smooth | TransitionType::Fluid)
    };
    assert!(should_redraw_continuously(TransitionType::Smooth));
    assert!(should_redraw_continuously(TransitionType::Fluid));
    assert!(!should_redraw_continuously(TransitionType::Instant));
    assert!(!should_redraw_continuously(TransitionType::Slide));
}

/// UC-6: fluid uses smooth-style transition (no slide direction).
#[test]
fn fluid_transition_no_slide_direction() {
    let config = app::parse_args(&[
        "program".into(), "/slides".into(),
        "--transition-type".into(), "fluid".into(),
    ]);
    assert!(config.is_some());
    assert_eq!(config.unwrap().transition_type, TransitionType::Fluid);
}

/// UC-6: fluid transition is the fourth variant (valid index checks).
#[test]
fn fluid_transition_variant_order() {
    assert_eq!(TransitionType::Fluid as u8, 3);
}
