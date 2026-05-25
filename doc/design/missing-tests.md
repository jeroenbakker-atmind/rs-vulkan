# Missing Tests

Gaps identified by comparing each use case step and extension against the test matrix.

Tests requiring an `App` instance or `GpuResources` need a Vulkan device and cannot be written as regular unit or integration tests. They are marked with †.

## UC-1: Initialize a New Presentation

| Missing | What is untested |
|---------|------------------|
| Step 5 | The success message `println!("Created example presentation at ...")` is not captured or asserted. |

## UC-2: Show a Presentation

| Missing | What is untested |
|---------|------------------|
| Extension 2b † | Dimension mismatch validation inside `create_texture_array` (line 598—607 of `src/app.rs`) has no test. The height/width comparison branch that returns `Err("Image '...' is NxM, expected NxM")` is never exercised. Requires a Vulkan device. |
| State machine † | The `update()` method (line 1473 of `src/app.rs`) that accumulates delta time and clears `is_transitioning` is not tested. The `last_frame` timestamp reset in `navigate_to` is not independently verified. |
| Direction override in navigate_to † | For non-Slide transitions, `navigate_to` resets direction to `(0.0, 0.0)` at line 1418. This override is not tested. |
| `current_layer` edge cases † | Navigating when `current_layer` is at bounds (first slide while already transitioning, last slide while already transitioning). |

## UC-3: Navigate with Instant Transitions

| Missing | What is untested |
|---------|------------------|
| navigate_to instant path † | The branch at line 1410—1412 (`TransitionType::Instant` sets `is_transitioning = false`) is not tested. No test asserts that after `navigate_to`, `current_layer == target_layer` and `is_transitioning == false` for instant mode. |
| Instant + direction override † | Instant transitions reset direction to (0, 0) via the non-Slide path in `navigate_to`. Not tested. |

## UC-4: Navigate with Slide Transitions

| Missing | What is untested |
|---------|------------------|
| Extension 3a † | Abort-and-restart: when a new navigation occurs mid-transition, `navigate_to` snaps `current_layer` to `self.target_layer` before starting the new transition (line 1403—1405). No test asserts this snap happens correctly or that `previous_layer` is set to the snapped value. |
| Slide direction preserved † | For Slide transitions, the direction set by `next_slide`/`prev_slide` should NOT be overridden by `navigate_to`. This is the `TransitionType::Slide` branch avoiding the reset at line 1418. Not independently tested. |

## UC-5: Navigate with Smooth Transitions

| Missing | What is untested |
|---------|------------------|
| Extension 2a † | Same abort-and-restart gap as UC-4 Extension 3a. |
| update lifecycle † | The `update()` → `transition_time += dt` → `is_transitioning = false` when `transition_time >= end_dur` path has no test. |
| end_dur branching † | Line 1487—1490 derives `end_dur` from config; both Slide and non-Slide paths produce the same result now, but the branch is untested. |

## CLI Argument Parsing

| Missing | What is untested |
|---------|------------------|

## Integration / Cross-cutting

| Missing | What is untested |
|---------|------------------|
| Full navigation lifecycle † | No test chains `next_slide()` → `update()` → `is_transitioning` check → `update()` → `is_transitioning` expired. Every test isolates a single method call. |
| Chapter 0 filenames | `0_1.png` is not validated against the documented convention (chapters start at 1). No test checks whether chapter 0 is accepted or rejected. |

---

† Requires a Vulkan device to instantiate `App` / `GpuResources`. These tests would need GPU access to run.
