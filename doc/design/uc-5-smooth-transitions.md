# UC-5: Navigate with Smooth Transitions

**Primary Actor:** Presenter  
**Level:** Sea-level

**Precondition:** The presenter has typed `rs-vulkan my-presentation --transition-type smooth --blur-radius 30`. The first slide is visible.

**Postcondition:** The previous slide content has been fully replaced by the new slide through a progressive blur and cross-fade.

## Main Success Scenario

1. The presenter presses a navigation key.
2. The application begins the transition by blending the new slide over a feedback texture using an alpha ramp.
3. On each rendered frame, the application applies a Gaussian blur to the feedback texture using a separable two-pass compute shader (horizontal then vertical).
4. The application composites the blurred feedback with the new slide, where the blend factor shifts over time from the old slide toward the new slide.
5. As the transition progresses, the old slide content becomes progressively more blurred while the new slide becomes more prominent.
6. After the transition completes, the new slide is displayed free of blur.
7. The application resets the feedback texture to clear state.

## Extensions

- 2a. A new navigation key is pressed before the transition completes:
  1. The application aborts the current transition and starts a new transition using the current visible result as the starting point.
