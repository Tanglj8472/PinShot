# PinShot (MVP)

Clean-room screenshot + pin tool, built only from `docs/design.md`
and general open-source Qt practice. Python 3 + PySide6 (P0 stack per the doc).

Win32 / Direct2D **SnipClone** lives on branch [`cursor/snipclone-17f2`](https://github.com/Tanglj8472/pinshot/tree/cursor/snipclone-17f2) and is not part of this tree.

## Install & run
```sh
# from the repository root
python3 -m venv .venv && .venv/bin/pip install -r requirements.txt
./run.sh            # or: .venv/bin/python -m pinshot
./run.sh --snip     # start a snip immediately
./run.sh --selftest # smoke test: color copy, nudge, snip->clipboard->pin, all 6 tools, undo/redo, history replay
```
Qt 6.5+ on X11 needs system libs: `libegl1 libxcb-cursor0 libxkbcommon-x11-0 libxcb-icccm4
libxcb-keysyms1 libxcb-render-util0 libxcb-image0 libxcb-xkb1`. On this box (no root) they were
extracted from Debian .debs into `.syslibs/`; `run.sh` adds them to `LD_LIBRARY_PATH` automatically.

## Tests
- `./run.sh --selftest` – quick smoke test.
- Parity QTest suite on the real display (in-process events only, 120+ checks (parity + regressions + settings/groups + click-through/autostart/multi-mon/delay/color), screenshots `parity_*.png`):
  `LD_LIBRARY_PATH=.syslibs/usr/lib/x86_64-linux-gnu .venv/bin/python -X faulthandler tests/qt_parity.py`
- `tests/ext_clip_owner.py` – separate process owning the X11 clipboard with only `image/png` (Chrome-like).

**Note on `.syslibs`**: extracted libraries must match the system ones. `libxkbcommon-x11-0` must be the
same version as the installed `libxkbcommon0` (here 1.7.0) or every real key press segfaults inside xkbcommon.

## Usage
- Control window + tray icon (tray only if a system tray exists). F1 snip, F3 paste/pin clipboard,
  Shift+F3 hide/show all pins (in-app; global via optional `pynput`, X11 only).

### Snip overlay
Before dragging, the window under the cursor (X11 `_NET_CLIENT_LIST_STACKING` + frame extents) is highlighted
and its size shown (whole screen = `1920 × 1200 px`); a **click without drag selects it**. **Tab** toggles
**window** vs **element** detection: element mode uses AT-SPI2 when `python-atspi`/`PyGObject` is installed,
otherwise a depth-limited `XQueryTree` walk of mapped child windows (documented fallback). The magnifier
(15×15 px grid at 8×, crosshair, coordinates, RGB/HEX, hints) follows the cursor while selecting.
A shortcut panel at the bottom-left changes with the active tool.

| Key / mouse | Action |
|---|---|
| drag | new selection · drag inside = move · 8 handles = resize |
| right-click | finish current shape → reset selection → exit |
| Esc | "Abort snipping?" confirm (Yes/No, "Do not show again") when a selection exists |
| Enter / Ctrl+C / double-click | copy (with annotations) |
| Ctrl+T / F3 · Ctrl+S | pin · save |
| C · Shift · Alt | copy color **and exit** · RGB↔HEX · force magnifier |
| W A S D · arrows · Ctrl/Shift+arrows | cursor 1 px · move selection · grow / shrink |
| Ctrl+A · Shift+R · `,` `.` | full screen · last area · history |
| Tab · `` ` `` | window/element detection · captured-cursor toggle (stub) |
| N · V | numbered marker · Select tool |
| 1 / 2 / wheel | width (or font size / mosaic size) of active tool |
| Ctrl+Z / Ctrl+Y | undo / redo |

Snip start: PinShot grabs the X pointer/keyboard immediately (before hiding the control window and capturing),
so early clicks/drags never reach the app underneath; the overlay's own grab replaces it once visible.

**Toolbar** tools: Select (V), Rect, Ellipse, Line, Pen, Marker, Mosaic, Text, **Number (N)**, Eraser.
Numbered markers place sequential circles (`1`, `2`, …) per document; undo renumbers. Select shows handles —
move, resize, Del, double-click text to re-edit (undoable).

**Toolbar** (fixed size/position below the selection, right-aligned; flips above if no room; the per-tool
sub-panel is a separate floating panel under it, so switching tools never moves the toolbar):
Rect · Ellipse · Line/Arrow · Pencil · Marker · Mosaic/Blur · Text · Eraser | Undo · Redo | Cancel · Pin · Save · Copy.
Sub-panels: rect/ellipse (outline/filled, width, 20-swatch palette + current color); line (solid/dash/dot,
arrow toggle, width, palette; Shift+drag continues a polyline); pencil/marker (width, palette; freehand records every move + 8ms cursor sample, works even when X11 delivers moves with buttons()=0); mosaic
(brush/rect, mosaic/blur, size slider default 6); text (B, I, outline, font, size, palette; typing never
triggers shortcuts; Enter commits, Esc discards); eraser (brush/rect, width; restores original pixels).
Shift while drawing rect/ellipse = square/circle. All icons are drawn by PinShot (`icons.py`).

### Pin window
Green glow on creation fading to a thin blue border; drop shadow (needs a compositor; toggle in menu).

| Key / mouse | Action |
|---|---|
| drag · wheel · Ctrl+wheel | move · zoom anchored top-left (tooltip) · opacity (tooltip "Opacity: 80%") |
| double-click · Shift+double-click | close (first time: one-off tip instead) · thumbnail square ↔ normal |
| Space | toolbar under the pin (same tools, ✓ finish instead of Pin) |
| R E A P K M T X | jump to rect/ellipse/arrow/pen/marker/mosaic/text/eraser |
| 1 2 3 4 | rotate L/R, flip H/V (or width when a tool is active) |
| Ctrl+C · Ctrl+S · Ctrl+Z/Y · Esc/Ctrl+W | copy · save · undo/redo · close |

Right-click menu (Esc closes the deepest open (sub)menu): Copy image, Save image as…, Show toolbar, Shadow ✓, Zoom ▸, Image processing ▸ (Rotate
left/right, Horizontal/Vertical flip), Paste, Replace by file…, Move to group ▸, View in folder, Close, Destroy, `W × H` ▸.

### Pin clipboard (F3 / button / tray)
Images pin near the cursor (cascading). Text: a color (`#ff8800`, `#f80`, `255,136,0`, `rgb(…)`) becomes a
**color card** (swatch + RGB, RGB float, HSV, HSL, HSI, HEX); any other text becomes a **text card**; cards are centered (cascading) and moved clear of the control window.
Reads the clipboard fresh each time; when another app owns the X11 CLIPBOARD it asks the X server directly
(`x11clip.py`, python-xlib, INCR support, prefers `image/png`), else Qt mime data (image, `image/*` bytes,
file URLs, `<img>` in HTML). Formats found are logged to stdout.

## Layout
`pinshot/main.py` (app, tray, hotkeys, single-instance CLI), `ipc.py` (QLocalServer), `capture.py` (Qt grab → mss fallback),
`overlay.py` (snip UI), `pin_window.py` (pins + annotation), `clipboard.py`, `x11clip.py`, `annotate.py` (shared shapes/renderer/input), `toolbar.py` (toolbar + sub-panels), `icons.py`, `winlist.py`, `cards.py`, `settings.py`; tests in `tests/`.

## Preferences (Ctrl+Shift+P)
Tabs: **General**, **Hotkeys**, **Control / Mouse**, **Output / Save**. Stored in QSettings (`PinShot`/`PinShot`).

- General: run at login *(stub — use your desktop startup apps)*, start minimized, abort-confirm toggle,
  double-click pin action (close / hide / none), snip history size, recoverable closed-pins cap, pin groups editor.
- Hotkeys: Snip, Pin clipboard, Hide/show all, Restore last closed pin (Ctrl+Shift+Z), **Click-through** (toggles pin under cursor; clears all if none).
  Global hotkeys use **X11 GrabKey** via python-xlib on this box (`pynput` failed to build: evdev).
  They do **not** work on pure Wayland.
- Output: quick-save folder, JPEG quality.

## Groups & session
- Pin context menu **Move to group ▸** (New / Rename); control window & tray list groups (double-click hide/show).
- Group names are unique (case-insensitive; stored duplicates healed on load). Creating/renaming from a pin refreshes the control-window list and tray at once.
- Open pins + group names persist across restarts (`session/*` in QSettings).

## Close vs Destroy
- **Close** (Esc / menu): snapshot pushed to an in-memory stack (size = Preferences → Recoverable closed pins).
  Restore with **Ctrl+Shift+Z** (global hotkey, rebindable in Preferences → Hotkeys), control-window button, or tray.
- **Destroy**: gone permanently (not stacked).

## Snip history (images)
`,` / `.` during snip step through the last N successful snips (**selection + full screenshot QImage**),
then back to the live capture. N is Preferences → Snip history size (default 10).

## Click-through
Hotkey (Preferences) or pin menu. On Linux/X11: `Qt.WindowTransparentForInput` **plus** an empty
XShape **ShapeInput** on the client window and its WM frame (Qt's flag alone leaves the frame eating clicks).
Visual cue: purple border + wide "CLICK-THRU" badge and opacity ≤ 55%. macOS/Windows: Qt flag only (no XShape).

## Multi-monitor
Snips capture the **virtual desktop** (`QGuiApplication.screens()` composed into one pixmap). The overlay
covers that virtual geometry so selections can span monitors. Pins use global coordinates across screens.
(This CI box has one 1920×1200 screen; the code path is the same for N>1.)

## Delayed snip
Control-window delay combo and Preferences → Snip delay (0/1/2/3/5 s). A countdown stays on screen so you
can open menus, then the snip starts.

## Custom annotation color
Tool sub-panel swatch button opens a topmost ApplicationModal `QColorDialog` with **alpha**. During snip,
the overlay is temporarily **hidden** (override-redirect cannot sit under a normal dialog) and grabs are released; it is restored when the dialog closes. Esc does not abort the snip.

## CLI / single-instance

One live PinShot owns a `QLocalServer` named `pinshot-<uid>`. A second launch forwards the command and exits:

```bash
python -m pinshot                  # raise control window
python -m pinshot snip             # start snip (or --snip)
python -m pinshot snip --delay 3   # delayed snip
python -m pinshot pin              # pin clipboard
python -m pinshot pins             # hide/show all pins
python -m pinshot prefs            # preferences
./run.sh --snip                    # same via run.sh
```

## Element detection & AT-SPI (rootless)
`Tab` in a snip toggles **window ↔ element**. Element mode takes AT-SPI2 accessible bounds (role + name, SHOWING
objects only, time-budgeted walk) when the a11y bus answers, merged with X11 `XQueryTree` child windows as a
fallback; hover picks the smallest rect under the cursor. The toast names the backend.

No root needed: `python3-gi`, `python3-gi-cairo`, `gir1.2-glib-2.0`, `gir1.2-girepository-2.0`,
`gir1.2-freedesktop`, `gir1.2-atspi-2.0`, `libgirepository-1.0-1` and `at-spi2-core` were fetched with
`apt-get download` and extracted into `.syslibs/`. `run.sh` sources `a11y_env.sh`, which sets
`GI_TYPELIB_PATH`, `PYTHONPATH`, `XDG_DATA_DIRS`, reuses the desktop session bus, and starts
`at-spi-bus-launcher` if needed. (The launcher's hard-coded config path was patched, same length, to
`~/.local/a11y/atspi/accessibility.conf`, because `/usr/share/defaults` is not writable.)
`Atspi.init()` aborts the process when no bus is reachable, so availability is probed in a subprocess first.

**Apps only show up if they export accessibility.** Qt apps need `QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1`, GTK apps do
it by default once the bus exists, and **Chrome must be started after the bus exists (or with
`--force-renderer-accessibility`)**. A Chrome that was already running stays on X11-child fallback.

## Language & theme
Preferences → General → **Language** (English / 简体中文) and **Theme** (Follow system / Light / Dark). Both are saved
and applied **live** when you hit Apply/OK. `i18n.py` keeps an en→zh dictionary plus regex patterns for dynamic strings.
An app-wide translator rewrites widget text, tooltips, titles, menu actions and combo items and keeps the
English originals, so switching back works too. Painted strings (hints, toasts, magnifier, CLICK-THRU badge, colour
cards) call `tr()`. The theme uses Fusion with a light or dark palette, theme-aware toolbar/sub-panel/confirm CSS, and re-tinted toolbar icons.
Accent (selection blue) and annotation colours are never changed.

## Windows

PinShot runs on Windows 10/11 through the same code. OS-specific parts live behind
`pinshot/platform/` (`base.py` interface + portable defaults, `linux_x11.py`, `windows.py`):

| Feature | Linux / X11 | Windows (`windows.py`, pure ctypes) |
|---|---|---|
| Capture | Qt per-screen grab, mss fallback, X11 input hold | Qt per-screen grab composed in logical coords, mss fallback; per-monitor DPI v2 (`SetProcessDpiAwarenessContext`) |
| Global hotkeys | X11 `XGrabKey` (pynput if present) | `RegisterHotKey` + `QAbstractNativeEventFilter` (`WM_HOTKEY`, `MOD_NOREPEAT`) |
| Click-through | Qt flag + empty XShape input (client + frame) | Qt flag + `WS_EX_TRANSPARENT \| WS_EX_LAYERED` via `SetWindowLongPtrW` |
| Window detection | `_NET_CLIENT_LIST_STACKING` + frame extents | `EnumWindows` (Z-order) + `DWMWA_EXTENDED_FRAME_BOUNDS`; skips invisible/minimised/cloaked |
| Element detection (Tab) | AT-SPI2, else X11 child windows | UI Automation via `comtypes` (time-limited ControlView walk), else `EnumChildWindows` |
| Clipboard extras | direct X11 CLIPBOARD read (INCR) | Qt native clipboard (CF_DIB / PNG) |
| Autostart | `~/.config/autostart/pinshot.desktop` | `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` → `PinShot` |
| Single instance | `QLocalServer` unix socket `pinshot-<uid>` | `QLocalServer` named pipe `pinshot-<user>` |
| Exclude from capture | n/a | `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` on the delay countdown |

### Build the exe on your Windows PC
Prerequisite: Python 3.10+ from python.org (with the `py` launcher). No compiler is needed; all dependencies are wheels.

```bat
git clone <your repo> pinshot && cd pinshot
build_windows.bat
rem or:  powershell -ExecutionPolicy Bypass -File build_windows.ps1
```
Both scripts create `.venv-win`, install `requirements-windows.txt` (PySide6, mss, comtypes, PyInstaller),
generate `assets\pinshot.ico` from our own icon (`tools\make_icon.py`), run `tests\test_platform.py`, then
`PyInstaller pinshot.spec` (`--onedir --windowed`), and print `PinShot.exe --version`.
Output: `dist\PinShot\PinShot.exe`. Ship the whole `dist\PinShot` folder. Set `PYTHON=C:\path\python.exe` to pick an interpreter.

Run from source without building: `py -3 -m venv .venv-win && .venv-win\Scripts\pip install -r requirements-windows.txt && .venv-win\Scripts\pythonw -m pinshot`.
The CLI works the same: `PinShot.exe snip --delay 3`, `PinShot.exe pin`, `PinShot.exe prefs` (forwarded to the running instance).

### Build via GitHub Actions
Push this repo to GitHub. `.github/workflows/build-windows.yml` runs on `windows-latest` on every push/PR
(or **Actions → build-windows → Run workflow**). It compile-checks, runs the platform unit tests and a real-Win32
smoke test (`tests\win_smoke.py`, informational), builds with PyInstaller, smoke-runs `PinShot.exe --version`,
and uploads the **PinShot-windows** artifact (zip of `dist/PinShot/`). Download it from the run's Summary page.

### Windows known gaps (not tested on real Windows from this Linux box)
- All Windows backends have only been checked on Linux: `py_compile`, unit tests with a fake `user32`, and
  imports with `sys.platform` forced to `win32`. Real Win32 behaviour is first exercised by `tests\win_smoke.py` in CI or on your PC.
- Mixed-DPI multi-monitor: snips are composed at **logical** resolution, so 150% screens are downsampled. Same-DPI setups are fine.
- UI Automation element walk is time-limited (~0.6 s) and covers the 12 topmost windows. Without `comtypes` it falls back to child HWNDs (coarse for Chrome/Electron).
- `RegisterHotKey` cannot take a combination another app owns (e.g. PrintScreen when Snipping Tool has it). The status line shows the backend; failed keys are skipped.
- No code signing: SmartScreen may warn on first run. The `--windowed` exe has no console (use `PINSHOT_VERSION_OUT=file` with `--version`).
- Not done: installer (MSI/Inno), per-monitor overlays, Windows toast notifications (uses the Qt tray balloon).

## Limitations / gaps vs design doc
- Wayland: no capture of other apps (run under X11/XWayland); no xdg-desktop-portal backend. Primary screen only.
  Global hotkeys require X11/XWayland (GrabKey); unavailable on pure Wayland.
- Element detection via AT-SPI only covers apps that registered on the a11y bus (see above). Chrome started earlier → X11 fallback.
- i18n: zh_CN + en only; Qt standard dialogs (file/colour pickers) stay in Qt's own language (no qtbase .qm bundled).
- Stubs: captured cursor (`` ` ``).
- Missing: plugins, more languages,
  native macOS/Win click-through backends beyond Qt's flag, per-screen (non-stitched) overlays.
- On exit the process uses `os._exit` to avoid a PySide6 teardown segfault seen on this box.
