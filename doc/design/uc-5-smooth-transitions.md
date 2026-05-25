# UC-5: Navigate with Smooth Transitions

**Primary Actor:** Presenter  
**Level:** Sea-level

**Precondition:** The presenter has typed `rs-vulkan my-presentation --transition-type smooth`. The first slide is visible. A continuous Gaussian blur feedback loop is running in the background. Slides may have transparent areas (alpha channel).

**Postcondition:** The previous slide content has been progressively blurred into the background while the new slide appeared on top, using the slide's native alpha channel. The blur feedback loop continues running indefinitely.

## Main Success Scenario

1. The presenter presses a navigation key.
2. The blur feedback loop begins running on every frame, progressively softening the previous slide content stored in the feedback buffer.
3. On each rendered frame, the application applies a Gaussian blur to the feedback texture using a separable two-pass compute shader (horizontal then vertical).
4. The application composites the blurred feedback with the new slide using alpha-over (`mix(fb, slide.rgb, slide.a)`). The new slide is overlaid on top at full opacity using its native alpha channel. Transparent pixels in the slide reveal the continuously blurring background behind.
5. As the transition progresses, the old slide content in the feedback buffer becomes increasingly blurred while the new slide is composited fresh each frame — it never feeds into the blur loop. Transparent areas of the new slide let the blur show through at all times.
6. After the transition completes, the new slide remains fully visible on top of the continuously blurring background. The blur feedback loop continues running indefinitely.

## Extensions

- 2a. A new navigation key is pressed before the transition completes:
  1. The application aborts the current transition and starts a new transition using the current visible result as the starting point.
