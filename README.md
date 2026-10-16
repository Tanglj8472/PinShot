# PinShot C++

PinShot C++ is the Windows implementation of a fast screenshot and image annotation tool.

## Features

- Capture a selected region of the desktop.
- Add rectangles, ellipses, lines, arrows, freehand marks and text.
- Edit annotations before copying or saving the image.
- Pin captured images above other windows.
- Use global shortcuts and a system tray workflow.

## Build

The project targets Windows and uses the Visual Studio C++ toolchain.

1. Open the C++ project files under `SnipClone/`.
2. Build the solution in Visual Studio with the desired configuration.
3. Run the generated application and use the tray icon or configured shortcut to start a capture.

## Project layout

- `SnipClone/AnnotationOverlay/` — annotation model, rendering and text editing.
- `SnipClone/Capture/` — desktop capture.
- `SnipClone/FloatingToolbar/` — capture toolbar and tool controls.
- `SnipClone/PinWindow/` — pinned image windows.
- `SnipClone/SelectionOverlay/` — region selection.
- `SnipClone/Common/` — shared Windows and image utilities.

## License

See the repository license and third-party notices for distribution terms.
