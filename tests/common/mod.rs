#![allow(dead_code)]

use std::collections::HashMap;
use std::path::Path;

use rs_vulkan::texture::{parse_presenter_notes, SlideCollection, SlideKey, SlideMeta};

/// Create a solid-color RGBA PNG at the given path and dimensions.
pub fn create_test_image(path: &Path, width: u32, height: u32, r: u8, g: u8, b: u8) {
    let img = image::RgbaImage::from_fn(width, height, |_, _| image::Rgba([r, g, b, 255]));
    img.save(path).unwrap();
}

/// Create a temporary directory populated with PNG slides for the given
/// chapter/slide pairs. Each image is 64x64 and solid black.
pub fn setup_slide_dir(prefix: &str, pairs: &[(u32, u32)]) -> tempfile::TempDir {
    let dir = tempfile::TempDir::with_prefix(prefix).unwrap();
    for &(ch, sl) in pairs {
        let name = format!("{}_{}.png", ch, sl);
        create_test_image(&dir.path().join(&name), 64, 64, 0, 0, 0);
    }
    dir
}

/// Build a SlideCollection from a slice of (chapter, slide) key pairs.
/// Metadata is generated with default names ("Chapter N", "Slide C_S").
pub fn make_collection(keys: &[(u32, u32)]) -> SlideCollection {
    let slides: Vec<SlideKey> =
        keys.iter().map(|&(c, s)| SlideKey { chapter: c, slide: s }).collect();
    let mut metadata = HashMap::new();
    for &(c, s) in keys {
        metadata.insert(
            (c, s),
            SlideMeta {
                chapter_num: c,
                slide_num: s,
                chapter_name: format!("Chapter {}", c),
                slide_name: format!("Slide {}_{}", c, s),
                presenter_notes: String::new(),
            },
        );
    }
    SlideCollection { slides, metadata }
}

/// Parse presenter notes from a string. Thin wrapper over
/// `parse_presenter_notes`.
pub fn notes_from(s: &str) -> HashMap<(u32, u32), SlideMeta> {
    parse_presenter_notes(s)
}

/// The smoothstep blend factor used in smooth transitions: `u²(3-2u)` where
/// `u = clamp(t / fade_dur, 0, 1)`.
pub fn compute_new_alpha(t: f32, fade_dur: f32) -> f32 {
    let u = (t / fade_dur).min(1.0);
    u * u * (3.0 - 2.0 * u)
}

/// The blur radius during a smooth transition: returns the configured max
/// radius continuously (blur never stops).
pub fn compute_blur(_t: f32, _blur_dur: f32) -> f32 {
    20.0
}

/// The slide offset for slide transitions. Uses cubic ease-out:
/// `offset = direction * (1 - ease_out(u))` where `u = clamp(t / dur, 0, 1)`.
pub fn compute_slide_offset(t: f32, dur: f32, direction: (f32, f32)) -> (f32, f32) {
    let u = (t / dur).min(1.0);
    let ease_out = 1.0 - (1.0 - u) * (1.0 - u) * (1.0 - u);
    (direction.0 * (1.0 - ease_out), direction.1 * (1.0 - ease_out))
}
