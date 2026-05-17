# UC-5: Navigate with Smooth Transitions

**Primary Actor:** Presenter  
**Level:** Sea-level

**Precondition:** The presenter has typed `rs-vulkan my-presentation --transition-type smooth`. The first slide is visible. A continuous Gaussian blur feedback loop is running in the background. Slides may have transparent areas (alpha channel).

**Postcondition:** The previous slide content has been progressively blurred into the background while the new slide appeared on top, using the slide's native alpha channel. The blur feedback loop continues running indefinitely.

## Main Success Scenario

1. The presenter presses a navigation key.
2. The application begins blending the new slide onto the feedback texture. The blend uses the slide's own alpha channel, modulated by an increasing alpha ramp (`new_alpha`). Meanwhile, the feedback loop continues running on every frame.
3. On each rendered frame, the application applies a Gaussian blur to the feedback texture using a separable two-pass compute shader (horizontal then vertical).
4. The application composites the blurred feedback with the new slide. The new slide is overlaid on top using its native alpha channel, further modulated by the transition alpha ramp. Transparent pixels in the slide reveal the continuously blurring background behind.
5. As the transition progresses, the old slide content becomes progressively more blurred while the new slide is overlaid with increasing opacity. Transparent areas of the new slide let the blur show through at all times.
6. After the transition completes, the new slide is fully visible on top of the continuously blurring background. The blur feedback loop continues running indefinitely.

## Extensions

- 2a. A new navigation key is pressed before the transition completes:
  1. The application aborts the current transition and starts a new transition using the current visible result as the starting point.
