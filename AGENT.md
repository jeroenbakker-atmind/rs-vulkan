# Implementation notes

## Smooth transition: feedback + compute blur

Uses two R16G16B16A16_SFLOAT feedback textures and a ping-pong intermediate.

### Per-frame pipeline (smooth, transitioning)

1. **Blend**: graphics pass, load input feedback, alpha-blend current slide on top (`fs_blend`).
2. **Blur H**: compute shader, storage images — input feedback → ping (`cs_blur_h`).
3. **Blur V**: compute shader, storage images — ping → output feedback (`cs_blur_v`).
4. **Present**: graphics pass, clear swapchain, draw output feedback + slide (`fs_present`).
5. **Swap**: `feedback_idx ^= 1` so next frame's input is this frame's output.

### Non-smooth paths
- `Instant` / `Slide` / idle: single graphics pass directly to swapchain (`fs_direct`).

### Push constants
6 × i32/f32 = 24 bytes: `{current_layer, previous_layer, new_alpha, blur_radius, slide_offset_x, slide_offset_y}`.

### Key files
- `src/app.rs` — all rendering pipelines + compute shaders
- `src/texture.rs` — slide loading / navigation
- `src/main.rs` — event loop / keyboard input

### Fields to be aware of
- `feedback_idx: usize` — toggles 0/1 each frame during smooth transitions
- No more `ghost_strength` or `blur_duration` — the IIR feedback loop naturally handles persistence
