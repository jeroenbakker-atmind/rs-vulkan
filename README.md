# RS-Vulkan Slides

A Vulkan-accelerated slideshow/presentation viewer. Renders PNG slides with GPU-accelerated transitions using the Vulkan API via `vulkano`.

## Usage

```text
rs-vulkan <slides-folder> [options]
rs-vulkan init <path>

Arguments:
  <slides-folder>    Directory containing chapter_slide.png files

Commands:
  init <path>        Create an example presentation at <path>

Options:
  --transition-type <type>     Transition style: smooth (default), instant, slide, or fluid
  --transition-duration <sec>  Transition duration in seconds (slide; default: 0.5)
  --help                       Show this help

Transition types:
  fluid   - Stable fluids advection for each color channel (default)
  smooth  - Compute-shader blur with single feedback buffer
  instant - No animation, immediate cut
  slide   - New slide slides in; from bottom for slides, from right for chapters
  fluid   - Stable fluids advection for each color channel
```

### Slide naming

Slides are PNG files named `{chapter}_{slide}.png` (e.g. `1_1.png`, `2_3.png`). Chapters and slides are sorted numerically for keyboard navigation.

## Examples

```text
# Create a new presentation
rs-vulkan init my-talk

# Default fluid transition (stable fluids advection)
rs-vulkan my-talk

# Slide transition, 3 second duration
rs-vulkan my-talk --transition-type slide --transition-duration 3

# Instant cuts (no animation)
rs-vulkan my-talk --transition-type instant

# Stable fluids transition (advects previous slide in background)
rs-vulkan my-talk --transition-type fluid

# Combine slide transition with custom timing
rs-vulkan my-talk --transition-type slide --transition-duration 2
```

## Technical overview

### Architecture

```mermaid
flowchart LR
    subgraph CPU [CPU - Rust]
        A[main.rs] -->|event loop| B[app.rs]
        B --> C[texture.rs]
        C -->|load slides| D[(PNG files)]
        B --> E[GPU Resources]
    end

    subgraph GPU [GPU - Vulkan]
        F[Vertex Shader] -->|fullscreen tri| G[Fragment Shaders]
        H[(Texture Array)] --> G
        I[(Feedback Textures)] <--> J[Compute Blur]
        G -->|framebuffer| K[Swapchain]
    end

    B -->|create pipeline| F
    B -->|upload| H
```

### Smooth transition (feedback + compute blur)

```mermaid
flowchart LR
    subgraph Frame [Per-frame smooth transition]
        A["Pass 0: blend previous slide into feedback\n(compute, only first frame of transition)"] -.-> B["Pass 1: horizontal blur\n(compute, feedback → ping)"]
        B --> C["Pass 2: vertical blur\n(compute, ping → feedback)"]
        C --> D["Pass 3: present feedback + slide\n(graphics, to swapchain)"]
    end
```

### Pipeline breakdown (smooth transition)

| Pass | Type | Source → Dest | Shader |
|------|------|---------------|--------|
| 0¹ | Compute | previous slide → feedback (alpha blend) | `cs_blend_slide` |
| 1 | Compute | feedback → ping (horizontal Gaussian) | `cs_blur_h` |
| 2 | Compute | ping → feedback (vertical Gaussian) | `cs_blur_v` |
| 3 | Graphics | feedback + slides → swapchain (CLEAR) | `fs_present` |

> ¹ Pass 0 runs only on the first frame of each transition. It seeds the feedback buffer with the departing (previous) slide content so the blur loop begins from a clean state. Subsequent frames skip this pass — the slide never re-enters the feedback loop.

A single feedback texture stores the IIR (infinite-impulse-response) accumulation. The separable blur reads from the feedback buffer, writes to a ping intermediate, then reads from ping and writes back to feedback — no texture swapping is needed.

### Non-smooth paths

For `instant` and `slide` transition types (and when not transitioning in `smooth` mode), a single graphics pass draws directly to the swapchain:

- `Instant`: just the current layer, fullscreen.
- `Slide`: previous layer stays, current layer slides in with cubic ease-out (`f(t) = 1 - (1-t)³`).

### Push constant layout

```rust
#[repr(C)]
struct PushConstants {
    current_layer: i32,     // texture array index of current (target) slide
    previous_layer: i32,    // texture array index of previous (source) slide
    blur_radius: f32,       // Gaussian blur kernel radius (compute shader)
    slide_offset_x: f32,    // horizontal UV offset for slide transition
    slide_offset_y: f32,    // vertical UV offset for slide transition
}
// Total: 20 bytes
```

## Transition types

| Type      | Description                                       | Config parameters            |
|-----------|---------------------------------------------------|------------------------------|
| `smooth`  | Compute-shader Gaussian blur with single feedback buffer | (none)                       |
| `instant` | Immediate cut, no animation                       | (none)                       |
| `slide`   | Slide new slide in with cubic ease-out            | `transition-duration`        |
| `fluid`   | Stable fluids advection of previous slide content in background | (none)                       |

### `smooth`

Uses a single feedback buffer with a separable Gaussian blur. Each frame during the transition:

1. (First frame only) The **previous** (departing) slide is alpha-blended into the feedback buffer to seed the IIR loop — `cs_blend_slide` compute shader
2. Horizontal blur: feedback → ping intermediate — `cs_blur_h` compute shader
3. Vertical blur: ping → feedback — `cs_blur_v` compute shader
4. The blurred feedback is drawn to the swapchain with the **target** slide composited on top — `fs_present` fragment shader

The blur radius is constant during the transition (default 20). Because the blur is applied every frame, the seeded previous-slide content progressively blurs out while the target slide is composited fresh each frame — it never re-enters the feedback loop.

### `instant`

No visual transition. `current_layer` switches immediately on navigation.

### `slide`

The incoming slide slides into view with a cubic ease-out curve (`f(t) = 1 - (1-t)³`). The outgoing slide remains stationary in the background.

| Navigation action | Direction of incoming slide |
|---|---|
| `next_slide` | Slides in from **bottom** (upward) |
| `prev_slide` | Slides in from **top** (downward) |
| `next_chapter` | Slides in from **right** (leftward) |
| `prev_chapter` | Slides in from **left** (rightward) |

Duration is controlled by `--transition-duration` (default 0.5s).

### `fluid`

A stable fluids simulation runs continuously in the background. On the first
frame of each transition, the **previous** (departing) slide is seeded into
the density field. Each subsequent frame:

1. **Buoyancy**: a velocity-driving force is computed from the density
   gradient and added to the 2D velocity field, with per-frame damping to
   prevent unbounded growth.
2. **Self-advection**: the velocity field advects itself via a semi-Lagrangian
   scheme using Catmull-Rom bicubic interpolation (reduces numerical diffusion
   compared to bilinear).
3. **Helmholtz-Hodge projection**: divergence → 10 Jacobi Poisson iterations → gradient
   subtraction, keeping the velocity divergence-free.
4. **Density advection**: the density field (feedback buffer) is advected by the
   velocity field (two substeps for CFL stability).
5. **Composite**: the current slide is composited over the advected density with
   alpha blending — colored pixels in the current slide replace the fluid
   underneath, while transparent background lets the swirling previous-slide
   content show through.

| Pass | Type | Source → Dest | Shader |
|------|------|---------------|--------|
| 0¹ | Compute | previous slide → feedback (alpha blend) | `cs_blend_slide` |
| 1 | Compute | feedback gradient → velocity | `cs_fluid_add_buoyancy` |
| 2 | Compute | velocity → velocity_ping (self-advection) | `cs_fluid_advect` |
| 3 | Compute | velocity_ping → divergence | `cs_fluid_divergence` |
| 4 | Compute | divergence + pressure ↔ pressure_ping (×10 Jacobi) | `cs_fluid_jacobi` |
| 5 | Compute | pressure + velocity_ping → velocity (project) | `cs_fluid_gradient_subtract` |
| 6a | Compute | feedback → ping (density advection) | `cs_fluid_advect` |
| 6b | Compute | ping → feedback (density advection) | `cs_fluid_advect` |
| 7 | Graphics | feedback + slides → swapchain (CLEAR) | `fs_present` |

> ¹ Pass 0 runs only on the first frame of each transition. It seeds the
> density field with the departing slide content. After the transition duration
> elapses the velocity continues to evolve, keeping the fluid alive
> indefinitely.

### Slide alpha convention

Generated slides (`init`) use a transparent black background with fully opaque
colored rectangles. In the fluid transition, the transparent background allows
the advected previous-slide content to bleed through, while opaque regions of
the current slide fully cover it.
