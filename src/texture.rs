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

