# Missing Tests

Gaps identified by comparing each use case step and extension against the test matrix.

## UC-1: Initialize a New Presentation

| Missing | What is untested |
|---------|------------------|
| Extension 2a | `rs-vulkan init existing-dir` when the target directory already exists. `init_example_presentation` calls `create_dir_all` which does not fail on existing directories, but the behavior (overwriting files, producing doubles) is not asserted. |
| Step 5 | The success message `println!("Created example presentation at ...")` is not captured or asserted. |

## UC-2: Show a Presentation

| Missing | What is untested |
|---------|------------------|
| Extension 2b | Dimension mismatch validation inside `create_texture_array` (line 598—607 of `src/app.rs`) has no unit test. The height/width comparison branch that returns `Err("Image '...' is NxM, expected NxM")` is never exercised. |
| State machine | The `update()` method (line 1473 of `src/app.rs`) that accumulates delta time and clears `is_transitioning` is not unit tested. The `last_frame` timestamp reset in `navigate_to` is not independently verified. |
| Direction: next_slide cross-chapter | `next_slide()` sets direction to `(1.0, 0.0)` when crossing a chapter boundary (line 1439 of `src/app.rs`). Only same-chapter next-slide direction is tested. |
| Direction: prev_slide cross-chapter | `prev_slide()` sets direction to `(-1.0, 0.0)` when crossing a chapter boundary (line 1453 of `src/app.rs`). Not tested. |
| Direction override in navigate_to | For non-Slide transitions, `navigate_to` resets direction to `(0.0, 0.0)` at line 1418. This override is not tested. |
| `current_layer_idx` edge cases | Navigating when `current_layer_idx` is at bounds (first slide while already transitioning, last slide while already transitioning). |

## UC-3: Navigate with Instant Transitions

| Missing | What is untested |
|---------|------------------|
| navigate_to instant path | The branch at line 1410—1412 (`TransitionType::Instant` sets `is_transitioning = false`) is not tested. No test asserts that after `navigate_to`, `current_layer == target_layer` and `is_transitioning == false` for instant mode. |
| Instant + direction override | Instant transitions reset direction to (0, 0) via the non-Slide path in `navigate_to`. Not tested. |

## UC-4: Navigate with Slide Transitions

| Missing | What is untested |
|---------|------------------|
| Extension 3a | Abort-and-restart: when a new navigation occurs mid-transition, `navigate_to` snaps `current_layer` to `self.target_layer` before starting the new transition (line 1403—1405). No test asserts this snap happens correctly or that `previous_layer` is set to the snapped value. |
| Slide direction preserved | For Slide transitions, the direction set by `next_slide`/`prev_slide` should NOT be overridden by `navigate_to`. This is the `TransitionType::Slide` branch avoiding the reset at line 1418. Not independently tested. |

## UC-5: Navigate with Smooth Transitions

| Missing | What is untested |
|---------|------------------|
| Extension 2a | Same abort-and-restart gap as UC-4 Extension 3a. |
| update lifecycle | The `update()` → `transition_time += dt` → `is_transitioning = false` when `transition_time >= end_dur` path has no test. |
| end_dur branching | Line 1487—1490 derives `end_dur` from config; both Slide and non-Slide paths produce the same result now, but the branch is untested. |

## CLI Argument Parsing

| Missing | What is untested |
|---------|------------------|
| `--profile` | The `--profile` flag at line 359 of `src/app.rs` sets `config.profiling = true`. No test parses this flag and asserts the result. |
| Negative values | `--blur-radius -5` or `--transition-duration -1` are accepted by the parser (any valid float passes). No test verifies rejection or behavior with negative values. |
| Zero values | `--blur-radius 0` and `--transition-duration 0` parse successfully. Not tested. |
| Missing value after flag | `rs-vulkan slides --transition-type` (without value) — the `args.get(i)` returns `None`, and due to the `?` operator the function returns `None`. This is implicitly tested via the `Invalid number` path but not explicitly for missing transition-type values. |
| Flag repetition | `rs-vulkan slides --transition-type instant --transition-type slide` — last flag wins. Not tested. |
| `--blur-radius` without `--transition-type smooth` | Parser accepts blur-radius with any transition type. Not tested. |

## Integration / Cross-cutting

| Missing | What is untested |
|---------|------------------|
| init on existing directory | Calling `init_example_presentation` on a path that already has files. No test verifies overwrite behavior or warns. |
| Full navigation lifecycle | No test chains `next_slide()` → `update()` → `is_transitioning` check → `update()` → `is_transitioning` expired. Every test isolates a single method call. |
| Chapter 0 filenames | `0_1.png` is not validated against the documented convention (chapters start at 1). No test checks whether chapter 0 is accepted or rejected. |
