# Presentation Organization

A presentation is stored as a directory containing:

## Slide Images

PNG files named `{chapter}_{slide}.png` (e.g., `1_1.png`, `2_3.png`). Chapter and slide numbers start at 1. All slide images must share the same pixel dimensions.

## Presenter Notes

Optionally, a file `presenter_notes.md` in the same directory. The file uses Markdown headers to structure notes per slide:

- `# {chapter}: {chapter_name}` — chapter header
- `## {chapter}_{slide}: {slide_title}` — slide header followed by presenter notes text

The application sorts slides alphanumerically by filename and groups them by chapter. The first slide of each chapter is treated as a chapter boundary for chapter-level navigation.
