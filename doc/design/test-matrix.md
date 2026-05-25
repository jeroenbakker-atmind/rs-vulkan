# Test Matrix

Each use case is mapped to the automated tests that cover its success path and extensions. All tests are in `tests/`.

## UC-1: Initialize a New Presentation

| Test | File | What it covers |
|------|------|----------------|
| `parse_args_init_returns_none` | `uc_1_initialize.rs` | Main success: `rs-vulkan init <path>` is accepted |
| `parse_args_init_missing_path_returns_none` | `uc_1_initialize.rs` | Extension: missing path is rejected |
| `init_creates_valid_presentation` | `uc_1_initialize.rs` | All expected files created, metadata parsed correctly |
| `init_slides_have_transparent_background` | `uc_1_initialize.rs` | Placeholder slides have transparent backgrounds |
| `init_slides_have_matching_dimensions` | `uc_1_initialize.rs` | All generated slides share the same dimensions |
| `init_existing_directory` | `uc_1_initialize.rs` | Extension 2a: init on directory that already exists succeeds |
| `init_existing_directory_with_files` | `uc_1_initialize.rs` | Extension 2a: init on directory with existing files overwrites them |

## UC-2: Show a Presentation

| Test | File | What it covers |
|------|------|----------------|
| `parse_args_default_config` | `uc_5_smooth_transitions.rs` | Main success: `rs-vulkan <path>` is accepted with default options |
| `parse_args_no_args_returns_none` | `uc_5_smooth_transitions.rs` | Extension 2a: no path provided |
| `parse_args_help_returns_none` | `uc_5_smooth_transitions.rs` | `--help` shows help text and exits |
| `parse_args_unknown_option_returns_none` | `uc_5_smooth_transitions.rs` | Unknown option is rejected |
| `load_directory_basic` | `uc_2_show_presentation.rs` | Slides loaded, keys and metadata are correct |
| `load_directory_sorted` | `uc_2_show_presentation.rs` | Slides sorted by (chapter, slide) |
| `load_directory_empty_errors` | `uc_2_show_presentation.rs` | Extension 2a: no PNG files |
| `load_directory_nonexistent_errors` | `uc_2_show_presentation.rs` | Extension 2a: directory does not exist |
| `load_directory_duplicate_errors` | `uc_2_show_presentation.rs` | Extension 2a: duplicate filenames |
| `load_directory_with_presenter_notes` | `uc_2_show_presentation.rs` | Presenter notes file is loaded and parsed |
| `load_directory_missing_notes_file` | `uc_2_show_presentation.rs` | No presenter notes file — defaults used |
| `parse_basic`, `parse_large_numbers` | `uc_2_show_presentation.rs` | Step 3-4: filename parsing (valid) |
| `parse_no_underscore`, `parse_non_numeric`, `parse_too_many_parts`, `parse_missing_number`, `parse_missing_ext`, `parse_no_underscore_dot` | `uc_2_show_presentation.rs` | Extension 2a: invalid filenames filtered out |
| `parse_filename_integration` | `uc_2_show_presentation.rs` | Valid filenames parse correctly |
| `parse_filename_ignores_non_matching` | `uc_2_show_presentation.rs` | Non-matching files are filtered |
| `nav_next_slide_within_chapter` | `uc_2_show_presentation.rs` | Step 4: next slide advances |
| `nav_next_slide_cross_chapter` | `uc_2_show_presentation.rs` | Extension 4a: cross-chapter slide advance |
| `nav_next_slide_at_end` | `uc_2_show_presentation.rs` | Extension 6a: last slide stays |
| `nav_prev_slide_within_chapter` | `uc_2_show_presentation.rs` | Step 6: previous slide |
| `nav_prev_slide_at_start` | `uc_2_show_presentation.rs` | Extension 6a: first slide stays |
| `nav_prev_slide_cross_chapter` | `uc_2_show_presentation.rs` | Step 6: cross-chapter previous slide |
| `nav_next_chapter` | `uc_2_show_presentation.rs` | Extension 4a: next chapter |
| `nav_next_chapter_at_end` | `uc_2_show_presentation.rs` | Extension 6a: last chapter stays |
| `nav_prev_chapter` | `uc_2_show_presentation.rs` | Extension 4b: previous chapter |
| `nav_prev_chapter_at_start` | `uc_2_show_presentation.rs` | Extension 6a: first chapter stays |
| `nav_is_first_of_chapter`, `nav_is_last_of_chapter` | `uc_2_show_presentation.rs` | Chapter boundary detection |
| `nav_non_sequential_chapters` | `uc_2_show_presentation.rs` | Non-sequential chapter numbers |
| `nav_single_slide` | `uc_2_show_presentation.rs` | Single-slide presentation edge case |
| `navigation_single_chapter` | `uc_2_show_presentation.rs` | Navigation within one chapter |
| `navigation_multi_chapter_from_loaded_dir` | `uc_2_show_presentation.rs` | Full navigation after directory load |
| all presenter notes tests | `uc_2_show_presentation.rs` | Notes parsing, inheritance, blank lines, empty input, only chapters |
| all format display tests | `uc_2_show_presentation.rs` | Slide metadata display formatting |
| `next_slide_same_chapter_direction` | `uc_2_show_presentation.rs` | Step 4-5: direction (0, 1) when next slide is in the same chapter |
| `next_slide_cross_chapter_direction` | `uc_2_show_presentation.rs` | Extension 4a: direction (1, 0) when next slide crosses into next chapter |
| `prev_slide_same_chapter_direction` | `uc_2_show_presentation.rs` | Step 6: direction (0, -1) when prev slide is in the same chapter |
| `prev_slide_cross_chapter_direction` | `uc_2_show_presentation.rs` | Extension 4b: direction (-1, 0) when prev slide crosses into previous chapter |

## UC-3: Navigate with Instant Transitions

| Test | File | What it covers |
|------|------|----------------|
| `parse_args_transition_type_instant` | `uc_3_instant_transitions.rs` | Precondition: `--transition-type instant` is accepted |
| `parse_args_transition_type_invalid` | `uc_3_instant_transitions.rs` | Invalid transition type is rejected |
| `transition_type_eq_ordering` | `uc_3_instant_transitions.rs` | TransitionType enum equality |
| `parse_args_duplicate_transition_type` | `uc_3_instant_transitions.rs` | Duplicate `--transition-type` flags; last value wins |
| `parse_args_missing_value_after_flag` | `uc_3_instant_transitions.rs` | Flag without value is rejected |

## UC-4: Navigate with Slide Transitions

| Test | File | What it covers |
|------|------|----------------|
| `parse_args_transition_type_slide` | `uc_4_slide_transitions.rs` | Precondition: `--transition-type slide` is accepted |
| `parse_args_custom_values` | `uc_4_slide_transitions.rs` | Precondition: `--transition-duration` is accepted |
| `slide_offset_starts_at_direction` | `uc_4_slide_transitions.rs` | Step 3: offset starts at full direction vector |
| `slide_offset_ends_at_zero` | `uc_4_slide_transitions.rs` | Step 5: offset reaches zero at end |
| `slide_offset_chapter_direction` | `uc_4_slide_transitions.rs` | Step 2: next chapter slides leftward |
| `slide_offset_prev_direction` | `uc_4_slide_transitions.rs` | Step 2: previous slide moves downward |
| `slide_offset_prev_chapter_direction` | `uc_4_slide_transitions.rs` | Step 2: previous chapter slides rightward |
| `slide_ease_out_midway` | `uc_4_slide_transitions.rs` | Step 3: cubic ease-out curve shape |

## UC-5: Navigate with Smooth Transitions

| Test | File | What it covers |
|------|------|----------------|
| `parse_args_transition_type_smooth_explicit` | `uc_5_smooth_transitions.rs` | Precondition: `--transition-type smooth` is accepted |
| `parse_args_transition_type_default` | `uc_5_smooth_transitions.rs` | Precondition: smooth is the default type |
| `instant_transition_default_config` | `uc_5_smooth_transitions.rs` | Default config is Smooth |
| `parse_args_custom_values` | `uc_4_slide_transitions.rs` | Precondition: `--transition-duration` is accepted |
| `transition_params_t0` | `uc_5_smooth_transitions.rs` | Step 2: at t=0, slide is fully opaque, blur is active |
| `transition_params_t0_25` | `uc_5_smooth_transitions.rs` | Step 4: blur is progressing mid-transition |
| `transition_params_t0_5` | `uc_5_smooth_transitions.rs` | Step 5: at t=dur, blur reaches steady state |
| `transition_params_t10` | `uc_5_smooth_transitions.rs` | Steps 5-6: after transition, blur stops |
| `parse_args_zero_transition_duration` | `uc_5_smooth_transitions.rs` | Zero `--transition-duration` is accepted |
| `parse_args_missing_value_after_flag` | `uc_5_smooth_transitions.rs` | Flag without value is rejected |
