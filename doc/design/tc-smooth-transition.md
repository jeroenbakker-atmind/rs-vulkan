# TC-Smooth: Smooth Transition

## Level

Sea-level — describes the GPU resources, shaders, and rendering steps involved in a smooth transition.

## GPU Resources

- **Slide texture array** — a 2D array image of format `R16G16B16A16_SFLOAT` containing all slides as individual layers.
- **One feedback image** — of format `R16G16B16A16_SFLOAT` at swapchain dimensions with color attachment, sampled, and storage usage flags. This is the persistent IIR accumulation buffer.
- **One ping image** — of the same format and dimensions with storage and sampled usage, used as an intermediate buffer for the separable blur.
- **Slides descriptor set** — a combined image sampler binding the slide array.
- **Present descriptor set** — one set binding the feedback image (sampled) and the slide array (sampled).
- **Blur descriptor sets** — one pair: a horizontal set binding the feedback image as read-only storage and the ping image as write-only storage, and a vertical set binding the ping image as read-only storage and the feedback image as write-only storage.
- **Blend descriptor set** — one set binding the feedback image as read-write storage and the slide array as combined image sampler, used by the first-frame blend compute shader.

All feedback and ping images use `R16G16B16A16_SFLOAT` to retain precision across the multiple accumulation and blur passes.

## Shaders

One fragment shader and three compute shaders work together in a multi-pass sequence:

- **Blend compute shader** — reads the previous slide (departing slide) from the texture array and alpha-blends it into the feedback buffer (`mix(fb, slide.rgb, slide.a)`). Only dispatched on the first frame of a transition.
- **Horizontal blur compute shader** — reads the feedback image, applies a Gaussian kernel along the X axis, and writes the result to the ping image.
- **Vertical blur compute shader** — reads the ping image, applies a Gaussian kernel along the Y axis, and writes the blurred result back to the feedback image.
- **Present fragment shader** — composites the current slide over the blurred feedback using alpha-over compositing, then outputs the result to the swapchain.

The Gaussian kernel uses a sigma of one-third the blur radius. The blur radius is configured by the user and remains constant throughout the transition.

## Rendering Steps

Each frame during the transition, in sequence:

0. **Blend pass (first frame only)** — On the first frame of a new transition, a compute dispatch alpha-blends the previous (departing) slide into the feedback buffer. This seeds the IIR loop so the blur begins from the previous slide content rather than stale residual data. The `transition_blended` flag prevents re-blending on subsequent frames.

1. **Horizontal blur pass** — a compute dispatch applies the separable Gaussian kernel horizontally. Each workgroup processes a 16-by-16 tile. For each pixel, the shader samples a neighborhood of pixels along the X axis within the blur radius, weights them by a Gaussian falloff, and stores the normalized sum in the ping image.

2. **Vertical blur pass** — a compute dispatch applies the same Gaussian kernel vertically, reading from the ping image and writing back to the feedback image.

3. **Present pass** — a graphics render pass clears the swapchain image to black. The present pipeline samples the blurred feedback image and the current slide, then composites the slide over the feedback using alpha-over compositing (`mix(fb, slide.rgb, slide.a)`).

The IIR feedback loop works as follows: on the first frame the blend shader explicitly seeds the feedback buffer with the previous slide content. The blur spreads this content. The present pass composites the new slide on top using alpha-over. On subsequent frames the old content in the feedback buffer becomes increasingly blurred while the new slide is composited fresh each frame — it never feeds back into the blur loop.

If a new navigation request arrives while a transition is in progress, the current visible result in the feedback buffer becomes the starting point for the new transition. The current layer is snapped to the in-flight target and the new target is set.
