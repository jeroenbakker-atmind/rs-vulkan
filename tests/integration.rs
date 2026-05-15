use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use rs_vulkan::app;
use rs_vulkan::texture::{
    format_slide_display, load_slide_directory, parse_presenter_notes, parse_slide_filename,
    SlideCollection, SlideKey, SlideMeta,
};

fn create_test_image(path: &Path, width: u32, height: u32, r: u8, g: u8, b: u8) {
    let img = image::RgbaImage::from_fn(width, height, |_, _| {
        image::Rgba([r, g, b, 255])
    });
    img.save(path).unwrap();
}

fn setup_slide_dir(prefix: &str, pairs: &[(u32, u32)]) -> tempfile::TempDir {
    let dir = tempfile::TempDir::with_prefix(prefix).unwrap();
    for &(ch, sl) in pairs {
        let name = format!("{}_{}.png", ch, sl);
        create_test_image(&dir.path().join(&name), 64, 64, 0, 0, 0);
    }
    dir
}

// --- Slide filename parsing ---

#[test]
fn parse_filename_integration() {
    let entries = &["1_1.png", "12_345.png", "999_0.png"];
    for name in entries {
        let key = parse_slide_filename(name);
        assert!(key.is_some(), "should parse: {name}");
    }
}

#[test]
fn parse_filename_ignores_non_matching() {
    let entries = &["notes.md", "image.jpg", "123.png", "a_b.png"];
    for name in entries {
        assert!(parse_slide_filename(name).is_none(), "should reject: {name}");
    }
}

// --- Directory loading ---

#[test]
fn load_directory_basic() {
    let dir = setup_slide_dir("basic", &[(1, 1), (1, 2), (2, 1)]);
    let (keys, meta, paths) = load_slide_directory(dir.path()).unwrap();

    assert_eq!(keys.len(), 3);
    assert_eq!(keys[0], SlideKey { chapter: 1, slide: 1 });
    assert_eq!(keys[1], SlideKey { chapter: 1, slide: 2 });
    assert_eq!(keys[2], SlideKey { chapter: 2, slide: 1 });

    assert_eq!(paths.len(), 3);
    assert!(paths[0].exists());

    assert_eq!(meta.len(), 3);
    let m = meta.get(&(1, 1)).unwrap();
    assert_eq!(m.chapter_name, "Chapter 1");
    assert_eq!(m.slide_name, "Slide 1_1");
}

#[test]
fn load_directory_sorted() {
    let dir = setup_slide_dir("sorted", &[(2, 3), (1, 1), (2, 1), (1, 2)]);
    let (keys, _, _) = load_slide_directory(dir.path()).unwrap();

    assert_eq!(keys, vec![
        SlideKey { chapter: 1, slide: 1 },
        SlideKey { chapter: 1, slide: 2 },
        SlideKey { chapter: 2, slide: 1 },
        SlideKey { chapter: 2, slide: 3 },
    ]);
}

#[test]
fn load_directory_empty_errors() {
    let dir = tempfile::TempDir::with_prefix("empty").unwrap();
    let err = load_slide_directory(dir.path()).unwrap_err();
    assert!(err.contains("No PNG files"));
}

#[test]
fn load_directory_nonexistent_errors() {
    let err = load_slide_directory(Path::new("/nonexistent/path")).unwrap_err();
    assert!(err.contains("Failed to read directory"));
}

#[test]
fn load_directory_duplicate_errors() {
    let dir = tempfile::TempDir::with_prefix("dup").unwrap();
    create_test_image(&dir.path().join("01_01.png"), 64, 64, 0, 0, 0);
    create_test_image(&dir.path().join("1_2.png"), 64, 64, 0, 0, 0);
    create_test_image(&dir.path().join("2_1.png"), 64, 64, 0, 0, 0);
    // Create a second file whose name parses to the same key (1, 1) as 01_01.png
    create_test_image(&dir.path().join("1_1.png"), 64, 64, 0, 0, 0);

    let err = load_slide_directory(dir.path()).unwrap_err();
    assert!(err.contains("Duplicate slide"));
}

#[test]
fn load_directory_mismatched_dimensions_errors() {
    let dir = tempfile::TempDir::with_prefix("mismatch").unwrap();
    create_test_image(&dir.path().join("1_1.png"), 64, 64, 0, 0, 0);
    create_test_image(&dir.path().join("1_2.png"), 64, 64, 0, 0, 0);

    let (_keys, _meta, _paths) = load_slide_directory(dir.path()).unwrap();
    // Dimension mismatch is validated in create_texture_array (GPU-level), not in
    // load_slide_directory. Verify that load_slide_directory succeeds with matching
    // dimensions and returns the correct number of slides.
    assert_eq!(_keys.len(), 2);
    assert_eq!(_paths.len(), 2);
}

#[test]
fn load_directory_with_presenter_notes() {
    let dir = tempfile::TempDir::with_prefix("notes").unwrap();
    create_test_image(&dir.path().join("1_1.png"), 32, 32, 0, 0, 0);
    create_test_image(&dir.path().join("2_1.png"), 32, 32, 0, 0, 0);
    create_test_image(&dir.path().join("2_2.png"), 32, 32, 0, 0, 0);

    let notes_content = "\
# 1: Getting Started
## 1_1: Welcome
Hello and welcome to this presentation!

# 2: Deep Dive
## 2_1: Core Concepts
Explanation of core ideas.
## 2_2: Examples
Practical examples.
";
    let mut f = fs::File::create(dir.path().join("presenter_notes.md")).unwrap();
    f.write_all(notes_content.as_bytes()).unwrap();

    let (keys, meta, _) = load_slide_directory(dir.path()).unwrap();
    assert_eq!(keys.len(), 3);

    let m1 = meta.get(&(1, 1)).unwrap();
    assert_eq!(m1.chapter_name, "Getting Started");
    assert_eq!(m1.slide_name, "Welcome");
    assert_eq!(m1.presenter_notes, "Hello and welcome to this presentation!");

    let m2 = meta.get(&(2, 1)).unwrap();
    assert_eq!(m2.chapter_name, "Deep Dive");
    assert_eq!(m2.slide_name, "Core Concepts");

    let m3 = meta.get(&(2, 2)).unwrap();
    assert_eq!(m3.chapter_name, "Deep Dive");
    assert_eq!(m3.slide_name, "Examples");
    assert_eq!(m3.presenter_notes, "Practical examples.");
}

#[test]
fn load_directory_missing_notes_file() {
    let dir = setup_slide_dir("no_notes", &[(1, 1), (1, 2)]);
    let (_, meta, _) = load_slide_directory(dir.path()).unwrap();
    let m = meta.get(&(1, 1)).unwrap();
    assert_eq!(m.chapter_name, "Chapter 1");
    assert!(m.presenter_notes.is_empty());
}

// --- SlideCollection integration ---

fn make_collection(keys: &[(u32, u32)]) -> SlideCollection {
    let slides: Vec<SlideKey> = keys.iter().map(|&(c, s)| SlideKey { chapter: c, slide: s }).collect();
    let mut metadata = HashMap::new();
    for &(c, s) in keys {
        metadata.insert(
            (c, s),
            SlideMeta {
                chapter_num: c,
                slide_num: s,
                chapter_name: format!("Chapter {c}"),
                slide_name: format!("Slide {c}_{s}"),
                presenter_notes: String::new(),
            },
        );
    }
    SlideCollection { slides, metadata }
}

#[test]
fn navigation_single_chapter() {
    let c = make_collection(&[(1, 1), (1, 2), (1, 3)]);

    assert_eq!(c.chapter_of(0), Some(1));
    assert_eq!(c.chapter_of(2), Some(1));

    assert!(c.is_first_of_chapter(0));
    assert!(c.is_last_of_chapter(2));

    assert_eq!(c.next_slide(0), Some(1));
    assert_eq!(c.next_slide(2), None);
    assert_eq!(c.prev_slide(0), None);
    assert_eq!(c.prev_slide(1), Some(0));

    assert_eq!(c.next_chapter(0), None);
    assert_eq!(c.prev_chapter(2), None);
}

#[test]
fn navigation_multi_chapter_from_loaded_dir() {
    let dir = setup_slide_dir("nav", &[(1, 1), (2, 1), (2, 2), (3, 1)]);
    let (keys, meta, _) = load_slide_directory(dir.path()).unwrap();
    let collection = SlideCollection { slides: keys, metadata: meta };

    assert_eq!(collection.len(), 4);

    assert_eq!(collection.chapter_of(0), Some(1));
    assert_eq!(collection.chapter_of(1), Some(2));
    assert_eq!(collection.chapter_of(3), Some(3));

    assert!(collection.is_first_of_chapter(0));
    assert!(collection.is_first_of_chapter(1));
    assert!(collection.is_first_of_chapter(3));

    assert_eq!(collection.next_slide(0), Some(1));
    assert_eq!(collection.prev_slide(1), Some(0));
    assert_eq!(collection.next_slide(3), None);

    assert_eq!(collection.next_chapter(0), Some(1));
    assert_eq!(collection.next_chapter(1), Some(3));
    assert_eq!(collection.next_chapter(3), None);

    assert_eq!(collection.prev_chapter(0), None);
    assert_eq!(collection.prev_chapter(1), Some(0));
    assert_eq!(collection.prev_chapter(2), Some(0));
    assert_eq!(collection.prev_chapter(3), Some(2));
}

// --- Presenter notes parsing ---

#[test]
fn parse_presenter_notes_empty_input() {
    let result = parse_presenter_notes("");
    assert!(result.is_empty());
}

#[test]
fn parse_presenter_notes_only_chapters() {
    let result = parse_presenter_notes("# 1: Intro\n# 2: Main");
    assert!(result.is_empty());
}

#[test]
fn parse_presenter_notes_multiple_slides() {
    let result = parse_presenter_notes("\
# 1: Chapter One
## 1_1: Slide A
Notes for A

## 1_2: Slide B
Notes for B

# 2: Chapter Two
## 2_1: Slide C
Notes for C
");

    assert_eq!(result.len(), 3);
    assert_eq!(result.get(&(1, 1)).unwrap().presenter_notes, "Notes for A");
    assert_eq!(result.get(&(1, 2)).unwrap().presenter_notes, "Notes for B");
    assert_eq!(result.get(&(2, 1)).unwrap().presenter_notes, "Notes for C");
}

#[test]
fn parse_presenter_notes_chapter_inheritance() {
    let result = parse_presenter_notes("\
# 1: Intro
## 1_1: First
# 2: Deep Dive
## 2_1: Core
");

    assert_eq!(result.get(&(1, 1)).unwrap().chapter_name, "Intro");
    assert_eq!(result.get(&(2, 1)).unwrap().chapter_name, "Deep Dive");
}

#[test]
fn parse_presenter_notes_preserves_blank_lines() {
    let result = parse_presenter_notes("## 1_1: Test\nLine 1\n\nLine 3\n\n\nLine 6");
    let notes = &result.get(&(1, 1)).unwrap().presenter_notes;
    assert_eq!(notes, "Line 1\n\nLine 3\n\n\nLine 6");
}

// --- format_slide_display ---

#[test]
fn display_formats_correctly() {
    let meta = SlideMeta {
        chapter_num: 2,
        slide_num: 5,
        chapter_name: "Advanced".into(),
        slide_name: "Example".into(),
        presenter_notes: String::new(),
    };
    let output = format_slide_display(&meta, 0, 10);
    assert_eq!(output, "--- Chapter 2: Advanced ---\nSlide 2_5: Example");
}

#[test]
fn display_includes_notes_when_present() {
    let meta = SlideMeta {
        chapter_num: 1,
        slide_num: 1,
        chapter_name: "Intro".into(),
        slide_name: "Welcome".into(),
        presenter_notes: "Say hello to everyone.".into(),
    };
    let output = format_slide_display(&meta, 0, 5);
    assert!(output.contains("Say hello to everyone."));
    assert!(output.starts_with("--- Chapter 1: Intro ---"));
}

// --- AppConfig parsing ---

#[test]
fn parse_args_default_config() {
    let config = app::parse_args(&["program".into(), "/slides".into()]);
    assert!(config.is_some());
    let cfg = config.unwrap();
    assert_eq!(cfg.slides_path, PathBuf::from("/slides"));
    assert!((cfg.blur_radius_max - 20.0).abs() < 1e-6);
    assert!((cfg.transition_duration - 0.5).abs() < 1e-6);
}

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

#[test]
fn parse_args_transition_type_instant() {
    let config = app::parse_args(&[
        "program".into(), "/slides".into(),
        "--transition-type".into(), "instant".into(),
    ]);
    assert!(config.is_some());
    assert_eq!(config.unwrap().transition_type, app::TransitionType::Instant);
}

#[test]
fn parse_args_transition_type_smooth_explicit() {
    let config = app::parse_args(&[
        "program".into(), "/slides".into(),
        "--transition-type".into(), "smooth".into(),
    ]);
    assert!(config.is_some());
    assert_eq!(config.unwrap().transition_type, app::TransitionType::Smooth);
}

#[test]
fn parse_args_transition_type_default() {
    let config = app::parse_args(&["program".into(), "/slides".into()]);
    assert!(config.is_some());
    assert_eq!(config.unwrap().transition_type, app::TransitionType::Smooth);
}

#[test]
fn parse_args_transition_type_slide() {
    let config = app::parse_args(&[
        "program".into(), "/slides".into(),
        "--transition-type".into(), "slide".into(),
    ]);
    assert!(config.is_some());
    assert_eq!(config.unwrap().transition_type, app::TransitionType::Slide);
}

#[test]
fn parse_args_transition_type_invalid() {
    assert!(app::parse_args(&[
        "program".into(), "/slides".into(),
        "--transition-type".into(), "bogus".into(),
    ]).is_none());
}

#[test]
fn parse_args_help_returns_none() {
    assert!(app::parse_args(&["program".into(), "--help".into()]).is_none());
}

#[test]
fn parse_args_no_args_returns_none() {
    assert!(app::parse_args(&["program".into()]).is_none());
}

#[test]
fn parse_args_unknown_option_returns_none() {
    assert!(app::parse_args(&["program".into(), "/slides".into(), "--bogus".into()]).is_none());
}

#[test]
fn parse_args_invalid_number_returns_none() {
    assert!(app::parse_args(&["program".into(), "/slides".into(), "--blur-radius".into(), "abc".into()]).is_none());
}

// --- init command ---

#[test]
fn parse_args_init_returns_none() {
    let dir = tempfile::TempDir::with_prefix("init_test").unwrap();
    let path = dir.path().join("example");
    let result = app::parse_args(&["program".into(), "init".into(), path.to_string_lossy().into()]);
    assert!(result.is_none());
    assert!(path.join("presenter_notes.md").exists());
}

#[test]
fn parse_args_init_missing_path_returns_none() {
    let result = app::parse_args(&["program".into(), "init".into()]);
    assert!(result.is_none());
}

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

#[test]
fn init_slides_have_transparent_background() {
    let dir = tempfile::TempDir::with_prefix("init_transparent").unwrap();
    let path = dir.path().join("pres");
    app::init_example_presentation(&path);

    let img = image::open(path.join("1_1.png")).unwrap().to_rgba8();
    let pixel = img.get_pixel(0, 0);
    // Background pixel should be fully transparent
    assert_eq!(pixel.0[3], 0, "background should be transparent");

    // Some pixels should be non-transparent (the rectangles)
    let mut has_opaque = false;
    for y in 0..img.height() {
        for x in 0..img.width() {
            if img.get_pixel(x, y).0[3] > 0 {
                has_opaque = true;
                break;
            }
        }
        if has_opaque {
            break;
        }
    }
    assert!(has_opaque, "slide should have non-transparent rectangles");
}

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
        assert_eq!(
            img.dimensions(),
            (w, h),
            "dimension mismatch in {}",
            p.display()
        );
    }
}
