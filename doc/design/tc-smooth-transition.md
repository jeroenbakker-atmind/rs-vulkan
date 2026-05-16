# TC-Smooth: Smooth Transition

## Level

Sea-level — describes the GPU resources, shaders, and rendering steps involved in a smooth transition.

## GPU Resources

- **Slide texture array** — a 2D array image of format `R16G16B16A16_SFLOAT` containing all slides as individual layers.
- **Two feedback images** — each of format `R16G16B16A16_SFLOAT` at swapchain dimensions with color attachment, sampled, and storage usage flags. One feedback image is active per frame; the index alternates between frames.
- **One ping image** — of the same format and dimensions with storage and sampled usage, used as an intermediate buffer for the separable blur.
- **Slides descriptor set** — a combined image sampler binding the slide array.
- **Present descriptor sets** — one per feedback image, each binding the active feedback image and the slide array.
- **Blur descriptor sets** — one pair per feedback image: a horizontal set binding the feedback image as read-only storage and the ping image as write-only storage, and a vertical set binding the ping image as read-only storage and the feedback image as write-only storage.

All feedback and ping images use `R16G16B16A16_SFLOAT` to retain precision across the multiple accumulation and blur passes.

## Shaders

Three fragment shaders and two compute shaders work together in a four-pass sequence:

- **Blend fragment shader** — samples the current slide and outputs it with a variable alpha. The alpha is driven by the blend factor push constant. This shader writes to the active feedback image.
- **Horizontal blur compute shader** — reads the feedback image, applies a Gaussian kernel along the X axis, and writes the result to the ping image.
- **Vertical blur compute shader** — reads the ping image, applies a Gaussian kernel along the Y axis, and writes the blurred result back to the feedback image.
- **Present fragment shader** — samples the blurred feedback image and the current slide, then linearly interpolates between them using the blend factor.

The Gaussian kernel uses a sigma of one-third the blur radius. The blur radius is configured by the user and remains constant throughout the transition.

## Rendering Steps

Each frame during the transition, in sequence:

1. **Blend pass** — a graphics render pass targets the active feedback image with load and store operations, preserving its existing content. The blend pipeline renders the current slide with alpha equal to the blend factor. Source-over blending (`SrcAlpha`, `OneMinusSrcAlpha`) composites the slide onto whatever content is already in the feedback buffer.

2. **Horizontal blur pass** — a compute dispatch applies the separable Gaussian kernel horizontally. Each workgroup processes a 16-by-16 tile. For each pixel, the shader samples a neighborhood of pixels along the X axis within the blur radius, weights them by a Gaussian falloff, and stores the normalized sum in the ping image.

3. **Vertical blur pass** — a compute dispatch applies the same Gaussian kernel vertically, reading from the ping image and writing back to the active feedback image.

4. **Present pass** — a graphics render pass clears the swapchain image to black. The present pipeline samples the blurred feedback image and the current slide, then outputs their linear interpolation using the blend factor.

5. **Buffer swap** — the active feedback index is toggled so the next frame uses the other feedback image.

The IIR feedback loop works as follows: on the first frame the feedback buffer contains undefined content. The blend pass composites the new slide with partial alpha on top. The blur spreads this content. The present pass mixes the blurred result with the new slide. On subsequent frames the old content becomes increasingly blurred while the new slide accumulates through repeated blend passes.

If a new navigation request arrives while a transition is in progress, the current visible result in the feedback buffer becomes the starting point for the new transition. The current layer is snapped to the in-flight target and the new target is set. The blend factor resets to zero and the feedback buffer continues accumulating from its current state.
