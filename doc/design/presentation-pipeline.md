# Presentation Pipeline

## Level

Sea-level — describes the technical architecture of the rendering pipeline and the GPU resources it uses.

## Overview

The presentation pipeline renders a fullscreen slideshow using the Vulkan API. All graphical work targets a swapchain that is presented to a window. There are two rendering paths: a direct path for static display, instant transitions, and slide transitions; and a four-pass path for smooth transitions.

## GPU Images

All slides are uploaded into a single GPU texture array of format `R16G16B16A16_SFLOAT` with one array layer per slide. A nearest-level sampler with clamp-to-edge addressing samples from this array using the desired layer index.

Two feedback images of the same floating-point format and at swapchain dimensions serve as persistent accumulation buffers for the smooth transition effect. These are created with color attachment, sampled, and storage usage flags because they are alternately used as render targets, compute shader inputs/outputs, and sampled textures. A single ping image of the same format and dimensions acts as an intermediate buffer for the separable blur.

A single combined image sampler descriptor set exposes the slide array to all rendering pipelines. The feedback and blur pipelines each have their own descriptor sets.

## Shaders

A vertex-less vertex shader generates a fullscreen triangle from the vertex index, covering the entire clip space. The output UV coordinates range from (0,0) at the top-left to (1,1) at the bottom-right.

Four fragment shaders and two compute shaders implement the different rendering modes:
- A direct fragment shader samples the slide array and outputs the result in a single pass. It handles both static display and slide-in animation.
- A blend fragment shader samples the current slide and outputs it with a variable alpha for compositing onto the feedback buffer.
- A present fragment shader mixes the blurred feedback texture with the current slide.
- Two compute shaders implement a separable Gaussian blur across the horizontal and vertical axes.

All shaders receive their parameters through a single 24-byte push constant block containing the current and previous slide layer indices, the blend factor, the blur radius, and the slide offset vector.

## Frame Lifecycle

Each frame the application acquires the next swapchain image, computes the push constant parameters based on the current transition state, records a command buffer, submits it to the graphics queue, and presents the result.

Synchronization between frames is handled by a single fence per frame: the previous frame's fence is waited on at the start of each new frame. The acquire, render, and present operations are chained as Vulkan futures.

When transitioning, the application requests a redraw every frame. The transition timer accumulates elapsed time and the transition is considered complete when the accumulated time reaches the configured duration. At that point the redraw requests stop.
