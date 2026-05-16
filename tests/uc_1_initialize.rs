mod common;

use common::create_test_image;
use rs_vulkan::app;
use rs_vulkan::texture::{load_slide_directory, SlideKey};

/// UC-1 steps 2-5: `rs-vulkan init <path>` creates directory, PNG slides with
/// placeholder rectangles, presenter notes file, and printable metadata.
#[test]
fn init_creates_valid_presentation() {
    let dir = tempfile::TempDir::with_prefix("init_create").unwrap();
    let path = dir.path().join("my_pres");
    app::init_example_presentation(&path);

    assert!(path.exists());
    assert!(path.is_dir());

    let expected_files = [
        "1_1.png", "1_2.png", "1_3.png",
        "2_1.png", "2_2.png",
        "3_1.png", "3_2.png",
        "presenter_notes.md",
    ];
    for f in &expected_files {
        assert!(path.join(f).exists(), "missing: {f}");
    }

    let (keys, meta, _paths) = load_slide_directory(&path).unwrap();
    assert_eq!(keys.len(), 7);
    assert_eq!(keys[0], SlideKey { chapter: 1, slide: 1 });
    assert_eq!(keys[6], SlideKey { chapter: 3, slide: 2 });

    let m = meta.get(&(1, 1)).unwrap();
    assert_eq!(m.chapter_name, "Getting Started");
    assert_eq!(m.slide_name, "Overview");
    assert!(m.presenter_notes.contains("RS-Vulkan presentation viewer"));

    let m = meta.get(&(2, 2)).unwrap();
    assert_eq!(m.chapter_name, "Advanced Topics");
    assert_eq!(m.slide_name, "Deployment");

    let m = meta.get(&(3, 1)).unwrap();
    assert_eq!(m.chapter_name, "Conclusion");
    assert_eq!(m.slide_name, "Summary");
}

/// UC-1 step 3: placeholder slides have a transparent background with visible
/// opaque rectangles drawn on top.
#[test]
fn init_slides_have_transparent_background() {
    let dir = tempfile::TempDir::with_prefix("init_transparent").unwrap();
    let path = dir.path().join("pres");
    app::init_example_presentation(&path);

    let img = image::open(path.join("1_1.png")).unwrap().to_rgba8();
    let pixel = img.get_pixel(0, 0);
    assert_eq!(pixel.0[3], 0, "background should be transparent");

    let mut has_opaque = false;
    for y in 0..img.height() {
        for x in 0..img.width() {
            if img.get_pixel(x, y).0[3] > 0 {
                has_opaque = true;
                break;
            }
        }
        if has_opaque { break; }
    }
    assert!(has_opaque, "slide should have non-transparent rectangles");
}

/// UC-1 step 3: all generated slides share the same pixel dimensions.
#[test]
fn init_slides_have_matching_dimensions() {
    let dir = tempfile::TempDir::with_prefix("init_dims").unwrap();
    let path = dir.path().join("pres");
    app::init_example_presentation(&path);

    let (_keys, _meta, paths) = load_slide_directory(&path).unwrap();
    let first = image::open(&paths[0]).unwrap().to_rgba8();
    let (w, h) = first.dimensions();
    for p in &paths {
        let img = image::open(p).unwrap().to_rgba8();
        assert_eq!(img.dimensions(), (w, h), "dimension mismatch in {}", p.display());
    }
}

/// UC-1 extension 2a: calling `rs-vulkan init` on a directory that already
/// exists succeeds and produces the expected output.
#[test]
fn init_existing_directory() {
    let dir = tempfile::TempDir::with_prefix("init_existing").unwrap();
    let path = dir.path().join("pres");
    std::fs::create_dir(&path).unwrap();
    app::init_example_presentation(&path);
    assert!(path.join("1_1.png").exists());
    assert!(path.join("presenter_notes.md").exists());
    let (keys, _, _) = load_slide_directory(&path).unwrap();
    assert_eq!(keys.len(), 7);
}

/// UC-1 extension 2a: calling `rs-vulkan init` on a directory that already
/// contains slide files overwrites them with the correct new dimensions.
#[test]
fn init_existing_directory_with_files() {
    let dir = tempfile::TempDir::with_prefix("init_overwrite").unwrap();
    let path = dir.path().join("pres");
    std::fs::create_dir(&path).unwrap();
    create_test_image(&path.join("1_1.png"), 10, 10, 255, 0, 0);
    app::init_example_presentation(&path);
    let (_, _, paths) = load_slide_directory(&path).unwrap();
    let img = image::open(&paths[0]).unwrap().to_rgba8();
    assert_eq!(img.dimensions(), (1920, 1080));
}

/// UC-1 step 1: `rs-vulkan init <path>` is accepted by parse_args (returns
/// None as init is handled before config parsing) and the example presentation
/// is written to disk.
#[test]
fn parse_args_init_returns_none() {
    let dir = tempfile::TempDir::with_prefix("init_test").unwrap();
    let path = dir.path().join("example");
    let result = app::parse_args(&["program".into(), "init".into(), path.to_string_lossy().into()]);
    assert!(result.is_none());
    assert!(path.join("presenter_notes.md").exists());
}

/// UC-1 extension 2a: `rs-vulkan init` without a path is rejected.
#[test]
fn parse_args_init_missing_path_returns_none() {
    let result = app::parse_args(&["program".into(), "init".into()]);
    assert!(result.is_none());
}
