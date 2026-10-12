# Python → Rust parity map

Source of truth: `pinshot/*.py` on `main`. Rust lives under `rust/crates/`.
Statuses: **done** / **partial** / **missing** / **impossible**.

Counts at the bottom. Python sources are not modified.

## Platform layer

| Python | Rust | Status | Notes |
|---|---|---|---|
| `platform/base.py` `BasePlatform` | `pinshot::native` | done | Process / capture / hotkeys / click-through / detect / autostart / clipboard |
| `platform/__init__.py` `current()` | `cfg(windows)` / `cfg(unix)` | done | Compile-time, not runtime swap |
| `platform/linux_x11.py` | `x11util.rs` + `capture_x11.rs` + `detect_x11.rs` + `autostart_xdg.rs` | done | X11/XWayland |
| `platform/windows.py` | `win32.rs` + `capture.rs` + `detect.rs` + `autostart.rs` | done | Behind `cfg(windows)` |
| `LinuxX11Platform._xcb` | winit `x11` feature | done | No Wayland-native path (same as Python) |
| `input_hold` / `capture.InputHold` | `x11util::InputHold` | done | Grab pointer+keyboard on root during capture |
| `exclude_from_capture` | — | impossible | Windows 10 2004+ WDA only |
| `install_hotkeys` | `native::install_hotkeys` | done | Win32 `RegisterHotKey` / X11 `XGrabKey` |
| `raise_on_top` / `x11raise.py` | `x11util::raise_xid_above` | done | `_NET_RESTACK_WINDOW` Below on `lower`, then ABOVE + restack Above + `_NET_ACTIVE_WINDOW`. OCR raises above `PinShot Pin` at 0/60/250 ms (not every app tick) |
| `set_skip_taskbar` | `apply_pin_style` `_NET_WM_STATE_SKIP_TASKBAR` | done | |
| `x11pass.set_input_passthrough` | `x11util::set_click_through` XShape Input | done | |
| `winlist.window_rects` | `detect_x11::window_rects` | done | `_NET_CLIENT_LIST_STACKING` + frame extents |
| `winlist.element_rects` | `detect_x11::collect_elements` | done | X11 walk depth 4; AT-SPI `{role}:{name}` + `exclude_pids` (self); X11 children still fill apps without a11y |
| `winlist.atspi_available` | `detect_x11::atspi_available` | done | Same subprocess safety rule |
| `x11clip.read` | `arboard` + process-lifetime owner + `xclip -t image/png` handoff | done | Overlay close leaves CLIPBOARD; Xvfb `xclip TARGETS` shows `image/png` |
| `autostart.py` XDG desktop | `autostart_xdg.rs` | done | `~/.config/autostart/pinshot.desktop` |
| Windows HKCU Run | `autostart.rs` | done | |
| `settings.data_dir` `~/.local/share/PinShot` | `config::data_dir` | done | Split from `config_dir` (`~/.config/PinShot`) |
| QSettings org/app | `config.toml` | done | Intentional schema (TOML, not QSettings) |

## App / tray / fallback / IPC

| Python | Rust | Status |
|---|---|---|
| `main.App.__init__` | `app::run` | done |
| `_build_fallback` 4 buttons, min width 220 | `fallback.rs` | done | Always shown on Linux; Windows if tray fails. Esc does not quit (Python same) |
| `_rebuild_tray` menu order | `tray.rs` | done | Snip, Pin clipboard, Snip and translate, Hide/Show all, Restore, Groups, Prefs, Quit |
| `QSystemTrayIcon.Trigger` only snips | `tray::poll` MenuEvent first, left-click only | done | Bugfix: every item used to snip |
| `_bind_local_shortcuts` F1/F3/Shift+F3/Ctrl+Shift+P/F5/F6 | fallback keys + XGrabKey + pin/overlay | done |
| `reinstall_hotkeys` | `native::install_hotkeys` on apply | done |
| `_on_ipc` snip/pin/pins/prefs/quit/translate/selection/raise | `ipc.rs` / `ipc_unix.rs` | done |
| `snip` + `snip_delay` countdown | `request_snip` + `countdown.rs` | done | Fallback hidden 150 ms before grab (Python same) |
| `_countdown_snip` QLabel | `countdown.rs` | done | 140×140 always-on-top numeral |
| `capture_translate` / `selection_translate` | overlay translate flag / F5 | done | F6 toast “Select text, then release”; F5 Linux: message (Python also Windows-only) |
| `pin_clipboard` image / color / text card | `clipboard` + `font::color_card` / `text_card` | done |
| `toggle_pins` / `toggle_clickthrough` / `restore_closed` | `app.rs` | done |
| `open_prefs` / `quit` / `_notify` | prefs thread / exit / log | done |
| `ipc.IpcServer` / `try_send` | named pipe / unix socket + flock | done |
| `session.persist_session` | `session.rs` + PNG dir + `PinSnapshot.shapes` | done | Original pixels + shapes (not baked PNG) |
| CLI `--snip --pin-clipboard --prefs --translate --version` | `main.rs` + `parse_cli_cmd` | done |
| `--debug` / file log | `logutil.rs` → `data_dir/pinshot.log` | done |
| `windows_subsystem = windows` | `main.rs` cfg_attr | done |

## Overlay (`overlay.py`)

| Python | Rust | Status |
|---|---|---|
| `SnipOverlay` fullscreen freeze | `overlay.rs` + override-redirect | done |
| Dim + hole + handles + size label | `draw::OverlayFront` | done |
| Window hover / Tab element | `selection` + `detect` | done |
| Magnifier 15×15 @ 8×, C / Shift | `magnifier.rs` + overlay keys | done |
| Drag / move / 8 handles | `selection.rs` | done | First F1 *selection* drag recovered via `QueryPointer`. Annotation press is winit-only — recover must not `annot.press` (that emptied the live shape) |
| Toolbar + OCR/Translate | `toolbar.rs` | done |
| `,` `.` history / Shift+R last rect | `history.rs` + overlay + `data_dir/history` | done | `,`/`.` replay image+rect; **R applies last rect on the live capture** (not the old PNG); persist so R works after restart |
| Esc confirm abort | in-overlay ConfirmAbort | done | Y/N / Esc / “Do not show again”; skip via prefs |
| Ctrl+C / Enter copy, Ctrl+S save, Ctrl+T / F3 pin | overlay keys | done |
| Ctrl+A full screen | done |
| Ctrl+Z/Y undo/redo | done |
| WASD move cursor | `overlay::move_cursor` | done |
| Arrows move / Shift shrink / Ctrl grow | `nudge_key` + `Annotator::translate_all` | done | Moving the hole (drag or arrows) keeps annotations in selection space |
| `` ` `` toggle cursor | toast + `paint_pointer` | done | System cursor hidden while painted |
| V select, N number, 1/2 width | done |
| Double-click copy / edit text | done |
| `wheelEvent` → `ann.wheel` | `annot.wheel(angleDelta)` + `WheelGate` | done | One notch = ±120; X11 double-delivery dropped within 40ms |
| `ConfirmAbort` in-overlay Y/N | overlay Y/N + “Do not show again” | done | No zenity/MessageBox (keeps the X11 grab) |
| `show_cursor` paint | `draw::paint_pointer` | done | Also stamped into copy/pin/save crop |
| Color copy then cancel | done | |

## Annotator (`annotate.py`)

| Python | Rust | Status |
|---|---|---|
| `Style` / `default_styles` / `Shape` | `annotation::{Style,Shape,Tool}` | done |
| `option_target` live\|selected\|edit\|width | `Annotator::option_target` + live branch | done | `add` still sets `width_target` (style/color edit the just-drawn shape). Also sets `selected`/`edit_target` so the 8 handles show; Python paints handles only for `selected` (None after draw) |
| `option_tool` / `option_style` | `option_tool` | done |
| `adjust_width` + 0.5s undo merge | done |
| `wheel` trunc(angle/120) | `Annotator::wheel` | done |
| `start_text` / `commit_text` / `begin_edit_text` | done + painted dashed editor | done |
| `key` Esc/Enter/IME | overlay + pin + `Ime::Commit` + Linux `XGrabKeyboard` | done | Cached keymap; Latin stays grabbed; IME preedit pauses; Esc join-ungrabs so F1 is not eaten |
| `press` select-on-click, mosaic no-steal | done |
| `_resize_selected` number/polyline/rect | done |
| `_push_edit` / undo 50 / redo | `UndoStack` | done |
| `_renumber` | done |
| `render` mosaic/blur/eraser | `render.rs` | done |
| Text glyphs / number digits | `font.rs` fontdue | done |
| Dash/dot line, fill, arrow, marker alpha | `render.rs` | done |
| Property panel (bold/italic/family/mode/effect) | B/I/Outline + family chips + mosaic mode/effect | done | Family is Default/Sans/Serif/Mono (not QFontComboBox). Prefer loadable Liberation/DejaVu TTF; Sans ≠ Mono; B/I load Bold/Italic or fake-bold/fake-italic |
| Custom QColorDialog | in-overlay / in-pin HSV + palette picker | done | No zenity; grab stays on the freeze |
| Brush cursor poll timer | QueryPointer 8ms while brush live | done |
| Pixel-perfect hit via 3×3 paint | `shape_paints_at` 3×3 paint | done | Translate so `pt` is pixel (1,1); alpha>16. Mosaic/eraser stay path-contains |

## Toolbar (`toolbar.py`)

| Python | Rust | Status |
|---|---|---|
| Tool order Rect Ellipse Line Arrow Pen Marker Mosaic Text Number | `toolbar.rs` | done |
| OCR / Translate / Undo Redo Close Pin Save Copy | done |
| Pin Check (done) | `TbAction::Check` | done |
| Palette 20 + width −/+ | done | Panel shows `option_width` (selected/live), not the tool default |
| Per-tool panel (filled, dash, blur, font…) | Fill/Dash/Dot; mode/effect; B/I/Outline; family chips | done | Mosaic/text −/+ show `size` / `font_size` via `option_width` |
| Tips / i18n keys | `Tool::tip` + `tr` | done |

## Pin window (`pin_window.py`)

| Python | Rust | Status |
|---|---|---|
| Frameless always-on-top + skip taskbar | winit + EWMH / WS_EX_TOOLWINDOW | done |
| Drag move, wheel zoom (cursor-anchored), Ctrl+wheel opacity | done | `WheelGate` shared with overlay; Ctrl from winit mods **or** `QueryPointer` mask so unfocused pins still change opacity; `_NET_WM_WINDOW_OPACITY` on client + WM frame, re-set after geom |
| Annotating: wheel → `ann.wheel` not zoom | done |
| Space / right-click Show toolbar | Space = toolbar; right-click = full menu | done | Never falls back to toolbar |
| TOOL_KEYS R E P T N … | R E P T N + 1/2/3/4 | done |
| Esc close / Ctrl+W | done |
| Double-click close/hide/none + tip | `PinWin::on_double_click` | done | First hit tips; `dblclick_tip_shown` persisted |
| Context menu full (`build_menu`) | Win32 popup / in-process `PinMenuWin` | done | Height sums real row sizes (Sep=8); Esc closes menu only (pin ignores Esc ~450ms) |
| Rotate / flip / paste / replace file | done |
| Shadow / thumb | `shadow` + Shift+dblclick 72px thumb | done | Default on when `_NET_WM_CM_Sn` is owned; persisted. Widget is image+2M, photo at (M,M), `WA_TranslucentBackground` + premultiplied ARGB present; hairline `QRectF(r).adjusted(-0.5,-0.5,-0.5,-0.5)`; session stores image origin |
| Click-through XShape / WS_EX_TRANSPARENT | done |
| Groups move / new / rename | tray groups; pin menu New/Rename prompt | done | KeyboardInput when not IME preedit (overlay text same); Ime::Commit for CJK; empty Enter ignored; Default cannot be renamed |
| Glow fade-in | `tick_glow` | done |

## Prefs / settings / theme / i18n

| Python | Rust | Status |
|---|---|---|
| `prefs.PrefsDialog` General/Hotkeys/Output + Advanced | `prefs.rs` egui (Win) / `prefs_win.rs` main-loop (Linux) | done |
| zh/en only, light/dark/system | done | Linux prefs/fallback repaint on Apply; system ≈ GTK_THEME |
| Hotkey conflict detection | `hotkey::find_conflicts` | done |
| Groups add/rename/remove | done |
| `theme.py` apply + watch | `UiTheme` + egui visuals | done | Prefs/fallback/pin-menu follow light/dark; no live gtk-settings watch |
| `i18n.STRINGS` every key | `i18n.rs` PAIRS | done | Two-placeholder `Zoom {0}%  ·  Opacity {1}%` matches; pin-menu footer uses `tr` |
| `group_label` | `groups::label` | done |

## Capture / clipboard / cards

| Python | Rust | Status |
|---|---|---|
| `capture.grab_virtual` Qt / mss | DXGI+GDI (Win) / XShm+XGetImage (X11) | done |
| `clipboard.copy_pixmap` / `clipboard_pixmap` | `arboard` | done |
| `cards.color_card` / `text_card` / `parse_color` | `font.rs` + `color::parse_color_text` | done |

## OCR / translate / selection text

| Python | Rust | Status |
|---|---|---|
| `translation.recognize` RapidOCR | `--features ocr` ort + PP-OCRv5 | partial | Band detector, not full DB |
| Edge `translatetext` | `translate.rs` | done |
| `ResultWindow` above pins | Win32 EDIT / egui | done |
| `selection_text.send_copy` | `result_win::send_ctrl_c` | done (Windows) / impossible (Linux; Python same) |

## Hotkeys (`hotkeys.py`)

| Python | Rust | Status |
|---|---|---|
| `parse_seq` | `parse_hotkey` | done |
| `X11Hotkeys` GrabKey + Lock/NumLock variants | `x11util::HotkeyHost` | done |
| `PynputHotkeys` | `x11util::HotkeyHost` (XGrabKey) | done | Python tries pynput then X11Hotkeys; Rust ships the XGrabKey fallback (typical when pynput/evdev is absent) |
| `WinHotkeys` | `win32::HotkeyHost` | done |

## Icons / session / groups / history

| Python | Rust | Status |
|---|---|---|
| `icons.py` app + toolbar | `icons.rs` | done |
| `session.py` groups + persist | `session.rs` + `groups.rs` | done |
| Overlay `HISTORY` | `history.rs` + PNG sidecar under `data_dir/history` | done | Process-global in Python; Rust persists last N. R = last rect on current shot; `,`/`.` replay stored image |

## Defaults / shortcuts / menu strings

Mirrored: F1 snip, F3 pin, Shift+F3 hide, F5/F6 translate, Ctrl+Shift+Z restore, tray order, zh/en strings.
Rust config default `language = "zh_CN"` (Phase 1) vs Python QSettings default `"en"` — **intentional** (already shipped).

## Impossible or out of scope

| Item | Reason |
|---|---|
| Qt widget i18n event filter | No Qt; `tr()` at paint/menu time |
| WDA exclude-from-capture | Windows-only API |
| Select-text translate on Linux | Python explicitly refuses |
| Pure Wayland (no XWayland) | Python `linux_x11` same limit |
| macOS backend | `BasePlatform` no-op; not requested |

## Counts

| Status | Rows |
|---|---|
| done | 111 |
| partial | 1 |
| missing | 0 |
| impossible | 6 |
| **total** | **118** |

Remaining partial: RapidOCR band detector (full DBNet is large). Hands-on still wanted: F6 toast, font chips + B/I paint, pin-menu rename/IME, OCR-above-pins restack, AT-SPI `{role}:{name}` on a live a11y bus, 3×3 hit feel vs geometry slop. Custom color is in-process (no zenity).
