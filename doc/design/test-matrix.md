# Test Matrix

Each use case is mapped to the automated tests that cover its success path and extensions.

## UC-1: Initialize a New Presentation

| Test | File | What it covers |
|------|------|----------------|
| `parse_args_init_returns_none` | `src/app.rs`, `tests/integration.rs` | Main success: `rs-vulkan init <path>` is accepted |
| `parse_args_init_missing_path_returns_none` | `src/app.rs`, `tests/integration.rs` | Extension: missing path is rejected |
| `init_creates_valid_presentation` | `tests/integration.rs` | All expected files created, metadata parsed correctly |
| `init_slides_have_transparent_background` | `tests/integration.rs` | Placeholder slides have transparent backgrounds |
| `init_slides_have_matching_dimensions` | `tests/integration.rs` | All generated slides share the same dimensions |

## UC-2: Show a Presentation

| Test | File | What it covers |
|------|------|----------------|
| `parse_args_default_config` | `src/app.rs`, `tests/integration.rs` | Main success: `rs-vulkan <path>` is accepted with default options |
| `parse_args_no_args_returns_none` | `src/app.rs`, `tests/integration.rs` | Extension 2a: no path provided |
| `parse_args_help_returns_none` | `src/app.rs`, `tests/integration.rs` | `--help` shows help text and exits |
| `parse_args_unknown_option_returns_none` | `src/app.rs`, `tests/integration.rs` | Unknown option is rejected |
| `load_directory_basic` | `tests/integration.rs` | Slides loaded, keys and metadata are correct |
| `load_directory_sorted` | `tests/integration.rs` | Slides sorted by (chapter, slide) |
| `load_directory_empty_errors` | `tests/integration.rs` | Extension 2a: no PNG files |
| `load_directory_nonexistent_errors` | `tests/integration.rs` | Extension 2a: directory does not exist |
| `load_directory_duplicate_errors` | `tests/integration.rs` | Extension 2a: duplicate filenames |
| `load_directory_with_presenter_notes` | `tests/integration.rs` | Presenter notes file is loaded and parsed |
| `load_directory_missing_notes_file` | `tests/integration.rs` | No presenter notes file — defaults used |
| `parse_basic`, `parse_large_numbers` | `src/texture.rs` | Step 3-4: filename parsing (valid) |
| `parse_no_underscore`, `parse_non_numeric`, `parse_too_many_parts`, `parse_missing_number`, `parse_missing_ext`, `parse_no_underscore_dot` | `src/texture.rs` | Extension 2a: invalid filenames filtered out |
| `parse_filename_integration` | `tests/integration.rs` | Valid filenames parse correctly |
| `parse_filename_ignores_non_matching` | `tests/integration.rs` | Non-matching files are filtered |
| `nav_next_slide_within_chapter` | `src/texture.rs` | Step 3: next slide advances |
| `nav_next_slide_cross_chapter` | `src/texture.rs` | Step 3a: cross-chapter slide advance |
| `nav_next_slide_at_end` | `src/texture.rs` | Extension 6a: last slide stays |
| `nav_prev_slide_within_chapter` | `src/texture.rs` | Step 5: previous slide |
| `nav_prev_slide_at_start` | `src/texture.rs` | Extension 6a: first slide stays |
| `nav_prev_slide_cross_chapter` | `src/texture.rs` | Step 5: cross-chapter previous slide |
| `nav_next_chapter` | `src/texture.rs` | Extension 3a: next chapter |
| `nav_next_chapter_at_end` | `src/texture.rs` | Extension 6a: last chapter stays |
| `nav_prev_chapter` | `src/texture.rs` | Extension 3b: previous chapter |
| `nav_prev_chapter_at_start` | `src/texture.rs` | Extension 6a: first chapter stays |
| `nav_is_first_of_chapter`, `nav_is_last_of_chapter` | `src/texture.rs` | Chapter boundary detection |
| `nav_non_sequential_chapters` | `src/texture.rs` | Non-sequential chapter numbers |
| `nav_single_slide` | `src/texture.rs` | Single-slide presentation edge case |
| `navigation_single_chapter` | `tests/integration.rs` | Navigation within one chapter |
| `navigation_multi_chapter_from_loaded_dir` | `tests/integration.rs` | Full navigation after directory load |
| `presenter notes tests` | `src/texture.rs`, `tests/integration.rs` | Notes parsing, inheritance, blank lines, empty input, only chapters |
| `format_slide_display_*` | `src/texture.rs` | Slide metadata display formatting |
| `display_formats_correctly`, `display_includes_notes` | `tests/integration.rs` | Display formatting integration |

## UC-3: Navigate with Instant Transitions

| Test | File | What it covers |
|------|------|----------------|
| `parse_args_transition_type_instant` | `src/app.rs`, `tests/integration.rs` | Precondition: `--transition-type instant` is accepted |
| `parse_args_transition_type_invalid` | `src/app.rs` | Invalid transition type is rejected |
| `transition_type_eq_ordering` | `src/app.rs` | TransitionType enum equality |

## UC-4: Navigate with Slide Transitions

| Test | File | What it covers |
|------|------|----------------|
| `parse_args_transition_type_slide` | `src/app.rs`, `tests/integration.rs` | Precondition: `--transition-type slide` is accepted |
| `parse_args_custom_values` | `src/app.rs`, `tests/integration.rs` | Precondition: `--transition-duration` is accepted |
| `slide_offset_starts_at_direction` | `src/app.rs` | Step 3: offset starts at full direction vector |
| `slide_offset_ends_at_zero` | `src/app.rs` | Step 5: offset reaches zero at end |
| `slide_offset_chapter_direction` | `src/app.rs` | Extension 3a: next chapter slides leftward |
| `slide_offset_prev_direction` | `src/app.rs` | Extension 3a: previous slide moves downward |
| `slide_offset_prev_chapter_direction` | `src/app.rs` | Extension 3a: previous chapter slides rightward |
| `slide_ease_out_midway` | `src/app.rs` | Step 3: cubic ease-out curve shape |

## UC-5: Navigate with Smooth Transitions

| Test | File | What it covers |
|------|------|----------------|
| `parse_args_transition_type_smooth_explicit` | `src/app.rs`, `tests/integration.rs` | Precondition: `--transition-type smooth` is accepted |
| `parse_args_transition_type_default` | `src/app.rs`, `tests/integration.rs` | Precondition: smooth is the default type |
| `instant_transition_default_config` | `src/app.rs` | Default config is Smooth |
| `parse_args_custom_values` | `src/app.rs`, `tests/integration.rs` | Precondition: `--blur-radius` is accepted |
| `parse_args_invalid_number_returns_none` | `src/app.rs`, `tests/integration.rs` | Invalid numeric argument is rejected |
| `transition_params_t0` | `src/app.rs` | Step 2: at t=0, blend starts at 0, blur is active |
| `transition_params_t0_25` | `src/app.rs` | Step 4: blend factor between 0 and 1 mid-transition |
| `transition_params_t0_5` | `src/app.rs` | Step 6: at t=dur, blend reaches 1 |
| `transition_params_t10` | `src/app.rs` | Step 6-7: after transition, blend is complete, blur stops |
| `transition_smoothstep_shape` | `src/app.rs` | Step 2: smoothstep has the correct S-curve shape |

## Tests Not Directly Mapping to Use Cases

These tests cover lower-level invariants and may support any use case that relies on metadata.

| Test | File | What it covers |
|------|------|----------------|
| `notes_basic` | `src/texture.rs` | Presenter notes: chapter, slide, notes parsing |
| `notes_multi_chapter` | `src/texture.rs` | Notes: chapter name inheritance across chapters |
| `notes_multi_line` | `src/texture.rs` | Notes: multi-line presenter notes preserved |
| `notes_empty` | `src/texture.rs` | Notes: empty notes content |
| `notes_no_slides` | `src/texture.rs` | Notes: only chapter headers, no slide entries |
| `notes_duplicate_key` | `src/texture.rs` | Notes: duplicate key is overwritten by last entry |
| `notes_chapter_name_with_colon` | `src/texture.rs` | Notes: colon in chapter name |
| `notes_slide_name_with_colon` | `src/texture.rs` | Notes: colon in slide name |
| `notes_blank_lines` | `src/texture.rs` | Notes: blank lines preserved in notes text |
| `parse_presenter_notes_empty_input` | `tests/integration.rs` | Integration: empty notes input |
| `parse_presenter_notes_only_chapters` | `tests/integration.rs` | Integration: chapter-only content |
| `parse_presenter_notes_multiple_slides` | `tests/integration.rs` | Integration: multiple slides parsed |
| `parse_presenter_notes_chapter_inheritance` | `tests/integration.rs` | Integration: chapter name inherited by subsequent slides |
| `parse_presenter_notes_preserves_blank_lines` | `tests/integration.rs` | Integration: blank lines in notes |
| `nav_chapter_of` | `src/texture.rs` | Navigation: chapter lookup by layer index |
