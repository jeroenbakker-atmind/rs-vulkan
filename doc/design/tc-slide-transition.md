# TC-Slide: Slide Transition

## Level

Sea-level — describes the GPU resources, shaders, and rendering steps involved in a slide transition.

## GPU Resources

- **Slide texture array** — a 2D array image of format `R16G16B16A16_SFLOAT` containing all slides as individual layers.
- **Combined image sampler descriptor set** — exposes the slide array to the direct fragment shader.

No additional images, buffers, or descriptor sets are required. The animation is driven entirely by shader push constants.

## Shaders

The direct fragment shader implements the slide animation. When the slide offset is non-zero, the shader samples two layers: the previous slide at the original UV coordinates and the current slide at UV coordinates shifted by the offset. If the shifted UV falls within the unit square, the current slide pixel is shown; otherwise the previous slide pixel is visible. This creates the visual effect of the new slide sliding in from outside the viewport while the old slide remains static in the background.

## Push Constants

The slide offset is computed per frame on the CPU. The application stores a direction vector set by the navigation method:
- Next slide within a chapter: slides upward — direction (0.0, 1.0)
- Previous slide within a chapter: slides downward — direction (0.0, -1.0)
- Next chapter: slides leftward — direction (1.0, 0.0)
- Previous chapter: slides rightward — direction (-1.0, 0.0)

The offset is driven by a cubic ease-out curve. At transition time the offset starts at the full direction vector and animates toward (0.0, 0.0). The blend factor is always 1.0, the blur radius is 0.0, and the previous layer index references the outgoing slide.

## Rendering Steps

Each frame during the transition:

1. The application acquires the next swapchain image.
2. The application computes the animation progress using accumulated time and the configured transition duration. The progress is passed through a cubic ease-out function.
3. A single command buffer is recorded:
   - A render pass clears the swapchain image to black.
   - The direct pipeline is bound with the slides descriptor set.
   - Push constants are written with the current and previous layer indices and the slide offset for this frame.
   - Three vertices are drawn, producing a fullscreen triangle. The fragment shader composites the two slides based on the offset.
4. The command buffer is submitted and the swapchain image is presented.

When the transition completes, the application stops updating the slide offset and the shader renders only the current slide at the original UV. No additional cleanup is needed.
