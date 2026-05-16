# UC-4: Navigate with Slide Transitions

**Primary Actor:** Presenter  
**Level:** Sea-level

**Precondition:** The presenter has typed `rs-vulkan my-presentation --transition-type slide --transition-duration 2.0`. The first slide is visible.

**Postcondition:** The new slide has animated into place; the old slide is no longer visible.

## Main Success Scenario

1. The presenter presses a navigation key.
2. The application determines the direction of motion based on the key:
   - ArrowDown (next slide): slides upward
   - ArrowUp (prev slide): slides downward
   - ArrowRight (next chapter): slides leftward
   - ArrowLeft (prev chapter): slides rightward
3. The application renders frames where the new slide moves from outside the viewport to its final position using a cubic ease-out animation curve.
4. The application holds the background slide static behind the moving slide.
5. After the transition duration elapses, the new slide fills the entire viewport.
6. The application continues rendering the new slide without movement.

## Extensions

- 3a. A new navigation key is pressed before the transition completes:
  1. The application aborts the current transition.
  2. The application starts a new transition from the current slide to the newly requested slide.
