# UC-1: Initialize a New Presentation

**Primary Actor:** Presenter  
**Scope:** Presentation viewer application  
**Level:** Sea-level

**Precondition:** The presenter has the application installed and is at a command line.

**Postcondition:** A new presentation directory exists in the file system containing dummy slides and a presenter notes template.

## Main Success Scenario

1. The presenter types `rs-vulkan init my-presentation`.
2. The application creates the target directory.
3. The application generates a set of PNG slides with placeholder content.
4. The application creates a presenter notes file containing chapter and slide headers.
5. The application reports success and exits.
6. The presenter opens the directory and replaces the dummy PNGs with their own slides.
7. The presenter edits the presenter notes file to add speaker notes.

## Extensions

- 2a. The target directory already exists:
  1. The application reports the error and exits without modifying the existing directory.
