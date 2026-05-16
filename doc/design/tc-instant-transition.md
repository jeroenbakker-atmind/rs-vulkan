# TC-Instant: Instant Transition

## Level

Sea-level — describes the GPU resources, shaders, and rendering steps involved in an instant transition.

## GPU Resources

- **Slide texture array** — a 2D array image of format `R16G16B16A16_SFLOAT` containing all slides as individual layers.
- **Combined image sampler descriptor set** — exposes the slide array to the direct fragment shader.

No additional images, buffers, or descriptor sets are required. Feedback and ping-pong textures are not used.

## Shaders

The direct fragment shader samples the slide array at the given layer index using the fullscreen UV coordinates and outputs the result directly. No blending or accumulation is performed.

## Rendering Steps

1. The application acquires the next swapchain image.
2. The application sets the current layer to the target slide and clears the transition state.
3. A single command buffer is recorded:
   - A render pass clears the swapchain image to black.
   - The direct pipeline is bound with the slides descriptor set.
   - Push constants are written with the current slide layer index. The blend factor is set to 1.0, the blur radius to 0.0, and the slide offset to (0.0, 0.0).
   - Three vertices are drawn, producing a fullscreen triangle that samples the new slide.
4. The command buffer is submitted and the swapchain image is presented.
