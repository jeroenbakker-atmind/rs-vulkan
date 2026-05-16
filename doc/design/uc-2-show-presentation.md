# UC-2: Show a Presentation

**Primary Actor:** Presenter  
**Level:** Sea-level

**Precondition:** A presentation directory exists with valid PNG slides.

**Postcondition:** The presenter has viewed all slides from start to end.

## Main Success Scenario

1. The presenter types `rs-vulkan my-presentation`.
2. The application opens a window and displays the first slide at full size.
3. The application prints the chapter name, slide title, and presenter notes for the first slide to the terminal.
4. The presenter presses ArrowDown to advance to the next slide.
5. The application shows the next slide in sequence and prints its presenter notes to the terminal.
6. The presenter continues stepping through slides with ArrowDown and ArrowUp.
7. The presenter presses Escape or Q to close the window.
8. The application exits.

## Extensions

- 2a. The directory contains no valid slides:
  1. The application prints an error and exits.
- 2b. Slides have mismatched dimensions:
  1. The application prints an error and exits.
- 4a. The presenter presses ArrowRight to jump to the first slide of the next chapter:
  1. The application displays the first slide of the following chapter and prints its presenter notes.
- 4b. The presenter presses ArrowLeft to jump to the first slide of the previous chapter:
  1. The application displays the first slide of the preceding chapter and prints its presenter notes.
- 6a. The presenter is on the last slide and presses ArrowDown or ArrowRight:
  1. The application stays on the last slide; no action is taken.
