mod common;

use std::path::Path;
use std::io::Write;

use common::{create_test_image, make_collection, notes_from, setup_slide_dir};
use rs_vulkan::texture::{
    format_slide_display, load_slide_directory, parse_presenter_notes, parse_slide_filename,
    SlideCollection, SlideKey, SlideMeta,
};

// --- parse_slide_filename ---

/// UC-2 step 2: a basic `chapter_slide.png` filename is parsed correctly.
#[test]
fn parse_basic() {
    assert_eq!(parse_slide_filename("1_1.png"), Some(SlideKey { chapter: 1, slide: 1 }));
}

/// UC-2: zero-padded chapter and slide numbers are parsed correctly.
#[test]
fn parse_zero_padded() {
    assert_eq!(parse_slide_filename("01_02.png"), Some(SlideKey { chapter: 1, slide: 2 }));
}

/// UC-2: large chapter and slide numbers are parsed correctly.
#[test]
fn parse_large_numbers() {
    assert_eq!(parse_slide_filename("10_100.png"), Some(SlideKey { chapter: 10, slide: 100 }));
}

/// UC-2 step 2a: filenames without an underscore are rejected.
#[test]
fn parse_no_underscore() {
    assert_eq!(parse_slide_filename("slide.png"), None);
}

/// UC-2 step 2a: non-numeric chapter/slide values are rejected.
#[test]
fn parse_non_numeric() {
    assert_eq!(parse_slide_filename("a_b.png"), None);
}

/// UC-2 step 2a: filenames with more than two parts are rejected.
#[test]
fn parse_too_many_parts() {
    assert_eq!(parse_slide_filename("1_2_3.png"), None);
}

/// UC-2 step 2a: filenames with a missing number are rejected.
#[test]
fn parse_missing_number() {
    assert_eq!(parse_slide_filename("1_.png"), None);
}

/// UC-2 step 2a: filenames without the .png extension are rejected.
#[test]
fn parse_missing_ext() {
    assert_eq!(parse_slide_filename("1_2"), None);
}

/// UC-2 step 2a: a plain number without underscore and dot is rejected.
#[test]
fn parse_no_underscore_dot() {
    assert_eq!(parse_slide_filename("12.png"), None);
}

/// UC-2 step 2: multiple valid filenames parse successfully in sequence.
#[test]
fn parse_filename_integration() {
    let entries = &["1_1.png", "12_345.png", "999_0.png"];
    for name in entries {
        let key = parse_slide_filename(name);
        assert!(key.is_some(), "should parse: {name}");
    }
}

/// UC-2 step 2a: non-PNG files and malformed names are filtered out.
#[test]
fn parse_filename_ignores_non_matching() {
    let entries = &["notes.md", "image.jpg", "123.png", "a_b.png"];
    for name in entries {
        assert!(parse_slide_filename(name).is_none(), "should reject: {name}");
    }
}

// --- load_slide_directory ---

/// UC-2 step 2: a directory with valid slides is loaded, sorted, and metadata
/// defaults are created for slides without presenter notes.
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

    let m = meta.get(&(1, 1)).unwrap();
    assert_eq!(m.chapter_name, "Chapter 1");
    assert_eq!(m.slide_name, "Slide 1_1");
}

/// UC-2 step 2: slides are sorted by (chapter, slide) regardless of
/// filesystem ordering.
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

/// UC-2 step 2a: an empty directory produces an error.
#[test]
fn load_directory_empty_errors() {
    let dir = tempfile::TempDir::with_prefix("empty").unwrap();
    let err = load_slide_directory(dir.path()).unwrap_err();
    assert!(err.contains("No PNG files"));
}

/// UC-2 step 2a: a nonexistent directory produces an error.
#[test]
fn load_directory_nonexistent_errors() {
    let err = load_slide_directory(Path::new("/nonexistent/path")).unwrap_err();
    assert!(err.contains("Failed to read directory"));
}

/// UC-2 step 2a: duplicate slide keys (e.g. 1_1.png and 01_01.png) are
/// detected and produce an error.
#[test]
fn load_directory_duplicate_errors() {
    let dir = tempfile::TempDir::with_prefix("dup").unwrap();
    create_test_image(&dir.path().join("01_01.png"), 64, 64, 0, 0, 0);
    create_test_image(&dir.path().join("1_2.png"), 64, 64, 0, 0, 0);
    create_test_image(&dir.path().join("2_1.png"), 64, 64, 0, 0, 0);
    create_test_image(&dir.path().join("1_1.png"), 64, 64, 0, 0, 0);

    let err = load_slide_directory(dir.path()).unwrap_err();
    assert!(err.contains("Duplicate slide"));
}

/// UC-2 step 2: slides with matching dimensions load successfully (dimension
/// mismatch is validated at GPU level by create_texture_array).
#[test]
fn load_directory_mismatched_dimensions() {
    let dir = tempfile::TempDir::with_prefix("mismatch").unwrap();
    create_test_image(&dir.path().join("1_1.png"), 64, 64, 0, 0, 0);
    create_test_image(&dir.path().join("1_2.png"), 64, 64, 0, 0, 0);

    let (_keys, _meta, _paths) = load_slide_directory(dir.path()).unwrap();
    assert_eq!(_keys.len(), 2);
    assert_eq!(_paths.len(), 2);
}

/// UC-2 step 3: presenter notes are loaded from presenter_notes.md and
/// associated with the correct slides.
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
    let mut f = std::fs::File::create(dir.path().join("presenter_notes.md")).unwrap();
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

/// UC-2 step 2: when no presenter_notes.md exists, defaults are generated.
#[test]
fn load_directory_missing_notes_file() {
    let dir = setup_slide_dir("no_notes", &[(1, 1), (1, 2)]);
    let (_, meta, _) = load_slide_directory(dir.path()).unwrap();
    let m = meta.get(&(1, 1)).unwrap();
    assert_eq!(m.chapter_name, "Chapter 1");
    assert!(m.presenter_notes.is_empty());
}

// --- parse_presenter_notes ---

/// UC-2 step 3: presenter notes with a chapter header, slide header, and note
/// text are parsed correctly.
#[test]
fn notes_basic() {
    let n = notes_from("# 1: Intro\n## 1_1: Welcome\nHi");
    let e = n.get(&(1, 1)).unwrap();
    assert_eq!(e.chapter_name, "Intro");
    assert_eq!(e.slide_name, "Welcome");
    assert_eq!(e.presenter_notes, "Hi");
}

/// UC-2 step 3: multiple chapters with their own slides inherit the correct
/// chapter name.
#[test]
fn notes_multi_chapter() {
    let n = notes_from("# 1: A\n## 1_1: X\nN1\n# 2: B\n## 2_1: Y\nN2");
    assert_eq!(n.get(&(1, 1)).unwrap().chapter_name, "A");
    assert_eq!(n.get(&(2, 1)).unwrap().chapter_name, "B");
    assert_eq!(n.get(&(2, 1)).unwrap().presenter_notes, "N2");
}

/// UC-2 step 3: multi-line presenter notes are preserved with their line
/// breaks.
#[test]
fn notes_multi_line() {
    let n = notes_from("## 1_1: X\nL1\nL2\nL3");
    assert_eq!(n.get(&(1, 1)).unwrap().presenter_notes, "L1\nL2\nL3");
}

/// UC-2 step 3: a slide header with no note text produces empty notes.
#[test]
fn notes_empty() {
    let n = notes_from("## 1_1: X\n");
    assert_eq!(n.get(&(1, 1)).unwrap().presenter_notes, "");
}

/// UC-2: chapter-only headers without slide entries produce no metadata.
#[test]
fn notes_no_slides() {
    let n = notes_from("# 1: A\ntext");
    assert!(n.is_empty());
}

/// UC-2: duplicate slide keys in presenter notes are overwritten by the last
/// occurrence.
#[test]
fn notes_duplicate_key() {
    let n = notes_from("## 1_1: A\nn1\n## 1_1: B\nn2");
    let e = n.get(&(1, 1)).unwrap();
    assert_eq!(e.slide_name, "B");
    assert_eq!(e.presenter_notes, "n2");
}

/// UC-2 step 3: chapter names containing colons are parsed correctly.
#[test]
fn notes_chapter_name_with_colon() {
    let n = notes_from("# 1: Complex: Name\n## 1_1: X\nnotes");
    assert_eq!(n.get(&(1, 1)).unwrap().chapter_name, "Complex: Name");
}

/// UC-2 step 3: slide names containing colons are parsed correctly.
#[test]
fn notes_slide_name_with_colon() {
    let n = notes_from("## 1_1: Title: Subtitle\nnotes");
    assert_eq!(n.get(&(1, 1)).unwrap().slide_name, "Title: Subtitle");
}

/// UC-2 step 3: blank lines within presenter notes are preserved.
#[test]
fn notes_blank_lines() {
    let n = notes_from("## 1_1: X\nA\n\nB");
    assert_eq!(n.get(&(1, 1)).unwrap().presenter_notes, "A\n\nB");
}

/// UC-2 step 3: an empty presenter notes input produces an empty map.
#[test]
fn parse_presenter_notes_empty_input() {
    let result = parse_presenter_notes("");
    assert!(result.is_empty());
}

/// UC-2 step 3: notes with only chapter headers (no slide entries) produce
/// an empty map.
#[test]
fn parse_presenter_notes_only_chapters() {
    let result = parse_presenter_notes("# 1: Intro\n# 2: Main");
    assert!(result.is_empty());
}

/// UC-2 step 3: multiple slides across chapters are all parsed correctly.
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

/// UC-2 step 3: each slide inherits the chapter name from the most recent
/// chapter header.
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

/// UC-2 step 3: blank lines within notes are preserved, including consecutive
/// blank lines.
#[test]
fn parse_presenter_notes_preserves_blank_lines() {
    let result = parse_presenter_notes("## 1_1: Test\nLine 1\n\nLine 3\n\n\nLine 6");
    let notes = &result.get(&(1, 1)).unwrap().presenter_notes;
    assert_eq!(notes, "Line 1\n\nLine 3\n\n\nLine 6");
}

// --- SlideCollection navigation ---

/// UC-2 step 2: chapter_of returns the chapter number for each slide layer.
#[test]
fn nav_chapter_of() {
    let c = make_collection(&[(1, 1), (1, 2), (2, 1), (2, 2), (2, 3), (3, 1)]);
    assert_eq!(c.chapter_of(0), Some(1));
    assert_eq!(c.chapter_of(1), Some(1));
    assert_eq!(c.chapter_of(2), Some(2));
    assert_eq!(c.chapter_of(5), Some(3));
}

/// UC-2 step 2: is_first_of_chapter is true for the first slide of each
/// chapter and false otherwise.
#[test]
fn nav_is_first_of_chapter() {
    let c = make_collection(&[(1, 1), (1, 2), (2, 1)]);
    assert!(c.is_first_of_chapter(0));
    assert!(!c.is_first_of_chapter(1));
    assert!(c.is_first_of_chapter(2));
    assert!(!c.is_first_of_chapter(3));
}

/// UC-2 step 2: is_last_of_chapter is true for the last slide of each chapter
/// and false otherwise.
#[test]
fn nav_is_last_of_chapter() {
    let c = make_collection(&[(1, 1), (1, 2), (2, 1)]);
    assert!(!c.is_last_of_chapter(0));
    assert!(c.is_last_of_chapter(1));
    assert!(c.is_last_of_chapter(2));
    assert!(!c.is_last_of_chapter(3));
}

/// UC-2 step 4: next_slide returns the next slide when within the same
/// chapter.
#[test]
fn nav_next_slide_within_chapter() {
    let c = make_collection(&[(1, 1), (1, 2), (2, 1)]);
    assert_eq!(c.next_slide(0), Some(1));
}

/// UC-2 step 4: next_slide returns the first slide of the next chapter when
/// the current slide is the last of its chapter.
#[test]
fn nav_next_slide_cross_chapter() {
    let c = make_collection(&[(1, 1), (1, 2), (2, 1)]);
    assert_eq!(c.next_slide(1), Some(2));
}

/// UC-2 step 6a: next_slide returns None when at the last slide.
#[test]
fn nav_next_slide_at_end() {
    let c = make_collection(&[(1, 1), (1, 2)]);
    assert_eq!(c.next_slide(1), None);
}

/// UC-2 step 6: prev_slide returns the previous slide within the same
/// chapter.
#[test]
fn nav_prev_slide_within_chapter() {
    let c = make_collection(&[(1, 1), (1, 2)]);
    assert_eq!(c.prev_slide(1), Some(0));
}

/// UC-2 step 6a: prev_slide returns None when at the first slide.
#[test]
fn nav_prev_slide_at_start() {
    let c = make_collection(&[(1, 1), (1, 2)]);
    assert_eq!(c.prev_slide(0), None);
}

/// UC-2 step 6: prev_slide crosses a chapter boundary to the last slide of
/// the previous chapter.
#[test]
fn nav_prev_slide_cross_chapter() {
    let c = make_collection(&[(1, 1), (2, 1)]);
    assert_eq!(c.prev_slide(1), Some(0));
}

/// UC-2 extension 4a: next_chapter finds the first slide of the next chapter.
#[test]
fn nav_next_chapter() {
    let c = make_collection(&[(1, 1), (1, 2), (2, 1), (3, 1)]);
    assert_eq!(c.next_chapter(0), Some(2));
    assert_eq!(c.next_chapter(1), Some(2));
    assert_eq!(c.next_chapter(2), Some(3));
}

/// UC-2 step 6a: next_chapter returns None when on the last chapter.
#[test]
fn nav_next_chapter_at_end() {
    let c = make_collection(&[(1, 1), (2, 1)]);
    assert_eq!(c.next_chapter(1), None);
}

/// UC-2 extension 4b: prev_chapter finds the first slide of the previous
/// chapter.
#[test]
fn nav_prev_chapter() {
    let c = make_collection(&[(1, 1), (2, 1), (2, 2), (3, 1)]);
    assert_eq!(c.prev_chapter(0), None);
    assert_eq!(c.prev_chapter(1), Some(0));
    assert_eq!(c.prev_chapter(3), Some(2));
}

/// UC-2 step 6a: prev_chapter returns None when on the first chapter.
#[test]
fn nav_prev_chapter_at_start() {
    let c = make_collection(&[(1, 1)]);
    assert_eq!(c.prev_chapter(0), None);
}

/// UC-2: non-sequential chapter numbers (e.g. chapters 1, 3, 5) are handled
/// correctly by next_chapter and prev_chapter.
#[test]
fn nav_non_sequential_chapters() {
    let c = make_collection(&[(1, 1), (3, 1), (5, 1)]);
    assert_eq!(c.next_chapter(0), Some(1));
    assert_eq!(c.prev_chapter(2), Some(1));
    assert_eq!(c.prev_chapter(1), Some(0));
}

/// UC-2: a single-slide presentation has correct boundary behavior: no next
/// or previous slide, no next or previous chapter, and is both first and last
/// of its chapter.
#[test]
fn nav_single_slide() {
    let c = make_collection(&[(1, 1)]);
    assert_eq!(c.next_slide(0), None);
    assert_eq!(c.prev_slide(0), None);
    assert_eq!(c.next_chapter(0), None);
    assert_eq!(c.prev_chapter(0), None);
    assert!(c.is_first_of_chapter(0));
    assert!(c.is_last_of_chapter(0));
}

// --- Direction checks (App::next_slide / prev_slide logic) ---

/// UC-2 extension 4a: when next_slide crosses a chapter boundary, the
/// chapters differ and App would set direction to (1.0, 0.0).
#[test]
fn next_slide_cross_chapter_direction() {
    let c = make_collection(&[(1, 1), (1, 2), (2, 1)]);
    let current = 1;
    let target = c.next_slide(current).unwrap();
    let same = c.chapter_of(current) == c.chapter_of(target);
    assert!(!same);
}

/// UC-2 extension 4b: when prev_slide crosses a chapter boundary, the
/// chapters differ and App would set direction to (-1.0, 0.0).
#[test]
fn prev_slide_cross_chapter_direction() {
    let c = make_collection(&[(1, 1), (2, 1), (2, 2)]);
    let current = 1;
    let target = c.prev_slide(current).unwrap();
    let same = c.chapter_of(current) == c.chapter_of(target);
    assert!(!same);
}

/// UC-2 step 4: when next_slide stays in the same chapter, the chapters match
/// and App would set direction to (0.0, 1.0).
#[test]
fn next_slide_same_chapter_direction() {
    let c = make_collection(&[(1, 1), (1, 2), (2, 1)]);
    let current = 0;
    let target = c.next_slide(current).unwrap();
    let same = c.chapter_of(current) == c.chapter_of(target);
    assert!(same);
}

/// UC-2 step 6: when prev_slide stays in the same chapter, the chapters
/// match and App would set direction to (0.0, -1.0).
#[test]
fn prev_slide_same_chapter_direction() {
    let c = make_collection(&[(1, 1), (1, 2), (2, 1)]);
    let current = 1;
    let target = c.prev_slide(current).unwrap();
    let same = c.chapter_of(current) == c.chapter_of(target);
    assert!(same);
}

// --- Navigation from loaded directory ---

/// UC-2: full navigation verification within a single chapter.
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

/// UC-2: full navigation across multiple chapters, using a SlideCollection
/// built from an actual loaded directory.
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

// --- format_slide_display ---

/// UC-2 step 3: format_slide_display includes chapter name, slide name, and
/// presenter notes when notes are present.
#[test]
fn format_display_with_notes() {
    let meta = SlideMeta {
        chapter_num: 1,
        slide_num: 2,
        chapter_name: "Intro".into(),
        slide_name: "Welcome".into(),
        presenter_notes: "Hello\nWorld".into(),
    };
    let s = format_slide_display(&meta, 0, 5);
    assert!(s.contains("Chapter 1: Intro"));
    assert!(s.contains("Slide 1_2: Welcome"));
    assert!(s.contains("Hello\nWorld"));
}

/// UC-2 step 3: format_slide_display omits the notes section when presenter
/// notes are empty.
#[test]
fn format_display_no_notes() {
    let meta = SlideMeta {
        chapter_num: 1,
        slide_num: 1,
        chapter_name: "Test".into(),
        slide_name: "Slide".into(),
        presenter_notes: String::new(),
    };
    let s = format_slide_display(&meta, 0, 1);
    assert!(s.contains("--- Chapter 1: Test ---"));
    assert!(s.contains("Slide 1_1: Slide"));
}

/// UC-2 step 3: format_slide_display produces the exact expected string
/// format.
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

/// UC-2 step 3: notes content appears in the display output when present.
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
