# 1: Getting Started
## 1_1: Overview
Welcome to this example presentation.

This slide deck demonstrates the RS-Vulkan presentation viewer.
Each slide is a PNG image with a transparent background and randomly placed rectangles.
## 1_2: Installation
To install RS-Vulkan:

1. Clone the repository
2. Run `cargo build --release`
3. Run `rs-vulkan <slides-folder>`

Make sure you have Vulkan drivers installed on your system.
## 1_3: Hello World
Create your first presentation:

```
rs-vulkan init my-presentation
rs-vulkan my-presentation
```

Navigate with arrow keys and Escape to quit.
# 2: Advanced Topics
## 2_1: Configuration
Customize the viewing experience:

- `--blur-radius`: Max Gaussian blur during transitions
- `--blur-duration`: How long the ghost dissolve lasts
- `--transition-duration`: Transition timing for smooth and slide

Default values work well for most presentations.
## 2_2: Deployment
Tips for presentations:

- Use consistent slide dimensions
- PNG files support transparency
- Use `chapter_slide.png` naming convention
- Add presenter notes via `presenter_notes.md`
# 3: Conclusion
## 3_1: Summary
RS-Vulkan features:

- Hardware-accelerated rendering via Vulkan
- Smooth transitions with blur and fade effects
- Presenter notes support
- Chapter-based navigation
- Fullscreen presentation mode
## 3_2: Next Steps
Ideas for extending:

- Add video slide support
- Implement smooth scrolling
- Add remote control support
- Build a slide editor UI
