use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SlideKey {
    pub chapter: u32,
    pub slide: u32,
}

#[derive(Debug, Clone)]
pub struct SlideMeta {
    pub chapter_num: u32,
    pub slide_num: u32,
    pub chapter_name: String,
    pub slide_name: String,
    pub presenter_notes: String,
}

#[derive(Debug, Clone)]
pub struct SlideCollection {
    pub slides: Vec<SlideKey>,
    pub metadata: HashMap<(u32, u32), SlideMeta>,
}

#[allow(dead_code)]
impl SlideCollection {
    pub fn len(&self) -> usize {
        self.slides.len()
    }

    pub fn chapter_of(&self, layer: usize) -> Option<u32> {
        self.slides.get(layer).map(|s| s.chapter)
    }

    pub fn slide_num_of(&self, layer: usize) -> Option<u32> {
        self.slides.get(layer).map(|s| s.slide)
    }

    pub fn meta(&self, layer: usize) -> Option<&SlideMeta> {
        let key = self.slides.get(layer)?;
        self.metadata.get(&(key.chapter, key.slide))
    }

    pub fn is_first_of_chapter(&self, layer: usize) -> bool {
        if layer >= self.slides.len() {
            return false;
        }
        layer == 0 || self.slides[layer].chapter != self.slides[layer - 1].chapter
    }

    pub fn is_last_of_chapter(&self, layer: usize) -> bool {
        if layer >= self.slides.len() {
            return false;
        }
        layer == self.slides.len() - 1
            || self.slides[layer].chapter != self.slides[layer + 1].chapter
    }

    pub fn next_slide(&self, layer: usize) -> Option<usize> {
        let next = layer + 1;
        if next < self.slides.len() {
            Some(next)
        } else {
            None
        }
    }

    pub fn prev_slide(&self, layer: usize) -> Option<usize> {
        layer.checked_sub(1)
    }

    pub fn next_chapter(&self, layer: usize) -> Option<usize> {
        let ch = self.slides.get(layer)?.chapter;
        self.slides.iter().position(|s| s.chapter > ch)
    }

    pub fn prev_chapter(&self, layer: usize) -> Option<usize> {
        let ch = self.slides.get(layer)?.chapter;
        self.slides.iter().rposition(|s| s.chapter < ch)
    }
}

pub fn parse_slide_filename(name: &str) -> Option<SlideKey> {
    let stem = name.strip_suffix(".png")?;
    let parts: Vec<&str> = stem.split('_').collect();
    if parts.len() != 2 {
        return None;
    }
    let chapter = parts[0].parse().ok()?;
    let slide = parts[1].parse().ok()?;
    Some(SlideKey { chapter, slide })
}

pub fn parse_presenter_notes(content: &str) -> HashMap<(u32, u32), SlideMeta> {
    let mut result = HashMap::new();
    let mut chapter_name = String::new();
    let mut pending: Option<(u32, u32, String)> = None;
    let mut pending_notes = String::new();

    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("# ") {
            flush_entry(&mut result, &mut pending, &mut pending_notes, &chapter_name);
            if let Some((num_str, name)) = rest.split_once(':') {
                if let Ok(_n) = num_str.trim().parse::<u32>() {
                    chapter_name = name.trim().to_string();
                }
            }
        } else if let Some(rest) = line.strip_prefix("## ") {
            flush_entry(&mut result, &mut pending, &mut pending_notes, &chapter_name);
            if let Some((key_str, slide_name)) = rest.split_once(':') {
                let parts: Vec<&str> = key_str.split('_').collect();
                if parts.len() == 2 {
                    if let (Ok(ch), Ok(sl)) = (parts[0].parse(), parts[1].parse()) {
                        pending = Some((ch, sl, slide_name.trim().to_string()));
                    }
                }
            }
        } else {
            pending_notes.push_str(line);
            pending_notes.push('\n');
        }
    }
    flush_entry(&mut result, &mut pending, &mut pending_notes, &chapter_name);

    result
}

fn flush_entry(
    result: &mut HashMap<(u32, u32), SlideMeta>,
    pending: &mut Option<(u32, u32, String)>,
    notes: &mut String,
    chapter_name: &str,
) {
    if let Some((ch, sl, slide_name)) = pending.take() {
        let text = std::mem::take(notes);
        let trimmed = text.trim().to_string();
        result.insert(
            (ch, sl),
            SlideMeta {
                chapter_num: ch,
                slide_num: sl,
                chapter_name: chapter_name.to_string(),
                slide_name,
                presenter_notes: trimmed,
            },
        );
    }
}

pub fn load_slide_directory(dir: &Path) -> Result<(Vec<SlideKey>, HashMap<(u32, u32), SlideMeta>, Vec<std::path::PathBuf>), String> {
    let mut entries: Vec<(SlideKey, std::path::PathBuf)> = std::fs::read_dir(dir)
        .map_err(|e| format!("Failed to read directory '{}': {e}", dir.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map_or(false, |ext| ext == "png")
        })
        .filter_map(|e| {
            let name = e.file_name();
            let name_str = name.to_str()?;
            let key = parse_slide_filename(name_str)?;
            Some((key, e.path()))
        })
        .collect();

    if entries.is_empty() {
        return Err(format!(
            "No PNG files matching 'chapter_slide.png' found in '{}'",
            dir.display()
        ));
    }

    entries.sort_by_key(|(k, _)| (k.chapter, k.slide));

    // check duplicates
    for w in entries.windows(2) {
        if w[0].0 == w[1].0 {
            return Err(format!(
                "Duplicate slide: {}_{}.png",
                w[0].0.chapter, w[0].0.slide
            ));
        }
    }

    let slides: Vec<SlideKey> = entries.iter().map(|(k, _)| *k).collect();
    let paths: Vec<std::path::PathBuf> = entries.iter().map(|(_, p)| p.clone()).collect();

    let notes_path = dir.join("presenter_notes.md");
    let notes = if notes_path.exists() {
        std::fs::read_to_string(&notes_path)
            .ok()
            .map(|c| parse_presenter_notes(&c))
            .unwrap_or_default()
    } else {
        HashMap::new()
    };

    let mut metadata = HashMap::new();
    for key in &slides {
        let entry = notes.get(&(key.chapter, key.slide));
        metadata.insert(
            (key.chapter, key.slide),
            SlideMeta {
                chapter_num: key.chapter,
                slide_num: key.slide,
                chapter_name: entry
                    .map_or_else(|| format!("Chapter {}", key.chapter), |m| m.chapter_name.clone()),
                slide_name: entry.map_or_else(
                    || format!("Slide {}_{}", key.chapter, key.slide),
                    |m| m.slide_name.clone(),
                ),
                presenter_notes: entry.map_or(String::new(), |m| m.presenter_notes.clone()),
            },
        );
    }

    Ok((slides, metadata, paths))
}

pub fn format_slide_display(meta: &SlideMeta, _layer: usize, _total: usize) -> String {
    let mut out = format!(
        "--- Chapter {}: {} ---\nSlide {}_{}: {}",
        meta.chapter_num, meta.chapter_name, meta.chapter_num, meta.slide_num, meta.slide_name
    );
    if !meta.presenter_notes.is_empty() {
        out.push('\n');
        out.push_str(&meta.presenter_notes);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_slide_filename ---

    #[test]
    fn parse_basic() {
        assert_eq!(parse_slide_filename("1_1.png"), Some(SlideKey { chapter: 1, slide: 1 }));
    }

    #[test]
    fn parse_zero_padded() {
        assert_eq!(parse_slide_filename("01_02.png"), Some(SlideKey { chapter: 1, slide: 2 }));
    }

    #[test]
    fn parse_large_numbers() {
        assert_eq!(parse_slide_filename("10_100.png"), Some(SlideKey { chapter: 10, slide: 100 }));
    }

    #[test]
    fn parse_no_underscore() {
        assert_eq!(parse_slide_filename("slide.png"), None);
    }

    #[test]
    fn parse_non_numeric() {
        assert_eq!(parse_slide_filename("a_b.png"), None);
    }

    #[test]
    fn parse_too_many_parts() {
        assert_eq!(parse_slide_filename("1_2_3.png"), None);
    }

    #[test]
    fn parse_missing_number() {
        assert_eq!(parse_slide_filename("1_.png"), None);
    }

    #[test]
    fn parse_missing_ext() {
        assert_eq!(parse_slide_filename("1_2"), None);
    }

    #[test]
    fn parse_no_underscore_dot() {
        assert_eq!(parse_slide_filename("12.png"), None);
    }

    // --- parse_presenter_notes ---

    fn notes_from(s: &str) -> HashMap<(u32, u32), SlideMeta> {
        parse_presenter_notes(s)
    }

    #[test]
    fn notes_basic() {
        let n = notes_from("# 1: Intro\n## 1_1: Welcome\nHi");
        let e = n.get(&(1, 1)).unwrap();
        assert_eq!(e.chapter_name, "Intro");
        assert_eq!(e.slide_name, "Welcome");
        assert_eq!(e.presenter_notes, "Hi");
    }

    #[test]
    fn notes_multi_chapter() {
        let n = notes_from("# 1: A\n## 1_1: X\nN1\n# 2: B\n## 2_1: Y\nN2");
        assert_eq!(n.get(&(1, 1)).unwrap().chapter_name, "A");
        assert_eq!(n.get(&(2, 1)).unwrap().chapter_name, "B");
        assert_eq!(n.get(&(2, 1)).unwrap().presenter_notes, "N2");
    }

    #[test]
    fn notes_multi_line() {
        let n = notes_from("## 1_1: X\nL1\nL2\nL3");
        assert_eq!(n.get(&(1, 1)).unwrap().presenter_notes, "L1\nL2\nL3");
    }

    #[test]
    fn notes_empty() {
        let n = notes_from("## 1_1: X\n");
        assert_eq!(n.get(&(1, 1)).unwrap().presenter_notes, "");
    }

    #[test]
    fn notes_no_slides() {
        let n = notes_from("# 1: A\ntext");
        assert!(n.is_empty());
    }

    #[test]
    fn notes_duplicate_key() {
        let n = notes_from("## 1_1: A\nn1\n## 1_1: B\nn2");
        let e = n.get(&(1, 1)).unwrap();
        assert_eq!(e.slide_name, "B");
        assert_eq!(e.presenter_notes, "n2");
    }

    #[test]
    fn notes_chapter_name_with_colon() {
        let n = notes_from("# 1: Complex: Name\n## 1_1: X\nnotes");
        assert_eq!(n.get(&(1, 1)).unwrap().chapter_name, "Complex: Name");
    }

    #[test]
    fn notes_slide_name_with_colon() {
        let n = notes_from("## 1_1: Title: Subtitle\nnotes");
        assert_eq!(n.get(&(1, 1)).unwrap().slide_name, "Title: Subtitle");
    }

    #[test]
    fn notes_blank_lines() {
        let n = notes_from("## 1_1: X\nA\n\nB");
        assert_eq!(n.get(&(1, 1)).unwrap().presenter_notes, "A\n\nB");
    }

    // --- SlideCollection navigation ---

    fn make_collection(keys: &[(u32, u32)]) -> SlideCollection {
        let slides: Vec<SlideKey> = keys.iter().map(|&(c, s)| SlideKey { chapter: c, slide: s }).collect();
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

    #[test]
    fn nav_chapter_of() {
        let c = make_collection(&[(1, 1), (1, 2), (2, 1), (2, 2), (2, 3), (3, 1)]);
        assert_eq!(c.chapter_of(0), Some(1));
        assert_eq!(c.chapter_of(1), Some(1));
        assert_eq!(c.chapter_of(2), Some(2));
        assert_eq!(c.chapter_of(5), Some(3));
    }

    #[test]
    fn nav_is_first_of_chapter() {
        let c = make_collection(&[(1, 1), (1, 2), (2, 1)]);
        assert!(c.is_first_of_chapter(0));
        assert!(!c.is_first_of_chapter(1));
        assert!(c.is_first_of_chapter(2));
        assert!(!c.is_first_of_chapter(3));
    }

    #[test]
    fn nav_is_last_of_chapter() {
        let c = make_collection(&[(1, 1), (1, 2), (2, 1)]);
        assert!(!c.is_last_of_chapter(0));
        assert!(c.is_last_of_chapter(1));
        assert!(c.is_last_of_chapter(2));
        assert!(!c.is_last_of_chapter(3));
    }

    #[test]
    fn nav_next_slide_within_chapter() {
        let c = make_collection(&[(1, 1), (1, 2), (2, 1)]);
        assert_eq!(c.next_slide(0), Some(1));
    }

    #[test]
    fn nav_next_slide_cross_chapter() {
        let c = make_collection(&[(1, 1), (1, 2), (2, 1)]);
        assert_eq!(c.next_slide(1), Some(2));
    }

    #[test]
    fn nav_next_slide_at_end() {
        let c = make_collection(&[(1, 1), (1, 2)]);
        assert_eq!(c.next_slide(1), None);
    }

    #[test]
    fn nav_prev_slide_within_chapter() {
        let c = make_collection(&[(1, 1), (1, 2)]);
        assert_eq!(c.prev_slide(1), Some(0));
    }

    #[test]
    fn nav_prev_slide_at_start() {
        let c = make_collection(&[(1, 1), (1, 2)]);
        assert_eq!(c.prev_slide(0), None);
    }

    #[test]
    fn nav_prev_slide_cross_chapter() {
        let c = make_collection(&[(1, 1), (2, 1)]);
        assert_eq!(c.prev_slide(1), Some(0));
    }

    #[test]
    fn nav_next_chapter() {
        let c = make_collection(&[(1, 1), (1, 2), (2, 1), (3, 1)]);
        assert_eq!(c.next_chapter(0), Some(2));
        assert_eq!(c.next_chapter(1), Some(2));
        assert_eq!(c.next_chapter(2), Some(3));
    }

    #[test]
    fn nav_next_chapter_at_end() {
        let c = make_collection(&[(1, 1), (2, 1)]);
        assert_eq!(c.next_chapter(1), None);
    }

    #[test]
    fn nav_prev_chapter() {
        let c = make_collection(&[(1, 1), (2, 1), (2, 2), (3, 1)]);
        assert_eq!(c.prev_chapter(0), None);
        assert_eq!(c.prev_chapter(1), Some(0));
        assert_eq!(c.prev_chapter(3), Some(2));
    }

    #[test]
    fn nav_prev_chapter_at_start() {
        let c = make_collection(&[(1, 1)]);
        assert_eq!(c.prev_chapter(0), None);
    }

    #[test]
    fn nav_non_sequential_chapters() {
        let c = make_collection(&[(1, 1), (3, 1), (5, 1)]);
        assert_eq!(c.next_chapter(0), Some(1));
        assert_eq!(c.prev_chapter(2), Some(1));
        assert_eq!(c.prev_chapter(1), Some(0));
    }

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

    // --- format_slide_display ---

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
}
