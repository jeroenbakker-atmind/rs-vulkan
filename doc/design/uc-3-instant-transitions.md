# UC-3: Navigate with Instant Transitions

**Primary Actor:** Presenter  
**Level:** Sea-level

**Precondition:** The presenter has typed `rs-vulkan my-presentation --transition-type instant`. The first slide is visible.

**Postcondition:** The new slide is displayed without any visual animation.

## Main Success Scenario

1. The presenter presses a navigation key (ArrowDown, ArrowUp, ArrowLeft, ArrowRight).
2. The application immediately replaces the current slide with the target slide.
3. The new slide is rendered in the next frame with no intermediate frames.
