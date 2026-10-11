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
| `raise_on_top` / `x11raise.py` | `x11util::raise_on_top` | partial | EWMH ABOVE + restack; OCR window uses egui always-on-top |
| `set_skip_taskbar` | `apply_pin_style` `_NET_WM_STATE_SKIP_TASKBAR` | done | |
| `x11pass.set_input_passthrough` | `x11util::set_click_through` XShape Input | done | |
| `winlist.window_rects` | `detect_x11::window_rects` | done | `_NET_CLIENT_LIST_STACKING` + frame extents |
| `winlist.element_rects` | `detect_x11::collect_elements` | partial | X11 children; AT-SPI probed in subprocess, rects not yet merged |
| `winlist.atspi_available` | `detect_x11::atspi_available` | done | Same subprocess safety rule |
| `x11clip.read` | `arboard` | partial | image/png + text; no raw target prefer |
| `autostart.py` XDG desktop | `autostart_xdg.rs` | done | `~/.config/autostart/pinshot.desktop` |
| Windows HKCU Run | `autostart.rs` | done | |
| `settings.data_dir` `~/.local/share/PinShot` | `config::data_dir` | done | Split from `config_dir` (`~/.config/PinShot`) |
| QSettings org/app | `config.toml` | done | Intentional schema (TOML, not QSettings) |

## App / tray / fallback / IPC

| Python | Rust | Status |
|---|---|---|
| `main.App.__init__` | `app::run` | done |
| `_build_fallback` 4 buttons, min width 220 | `fallback.rs` | done | Always shown on Linux; Windows if tray fails |
| `_rebuild_tray` menu order | `tray.rs` | done | Snip, Pin clipboard, Snip and translate, Hide/Show all, Restore, Groups, Prefs, Quit |
| `QSystemTrayIcon.Trigger` only snips | `tray::poll` MenuEvent first, left-click only | done | Bugfix: every item used to snip |
| `_bind_local_shortcuts` F1/F3/Shift+F3/Ctrl+Shift+P/F5/F6 | fallback keys + XGrabKey + pin/overlay | done |
| `reinstall_hotkeys` | `native::install_hotkeys` on apply | done |
| `_on_ipc` snip/pin/pins/prefs/quit/translate/selection/raise | `ipc.rs` / `ipc_unix.rs` | done |
| `snip` + `snip_delay` countdown | `request_snip` + `countdown.rs` | done | Fallback hidden 150 ms before grab (Python same) |
| `_countdown_snip` QLabel | `countdown.rs` | done | 140×140 always-on-top numeral |
| `capture_translate` / `selection_translate` | overlay translate flag / F5 | partial | F5 Linux: message (Python also Windows-only) |
| `pin_clipboard` image / color / text card | `clipboard` + `font::color_card` / `text_card` | done |
| `toggle_pins` / `toggle_clickthrough` / `restore_closed` | `app.rs` | done |
| `open_prefs` / `quit` / `_notify` | prefs thread / exit / log | done |
| `ipc.IpcServer` / `try_send` | named pipe / unix socket + flock | done |
| `session.persist_session` | `session.rs` + PNG dir | done |
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
| Drag / move / 8 handles | `selection.rs` | done |
| Toolbar + OCR/Translate | `toolbar.rs` | done |
| `,` `.` history / Shift+R last rect | `history.rs` + overlay | done |
| Esc confirm abort | MessageBox / zenity | done |
| Ctrl+C / Enter copy, Ctrl+S save, Ctrl+T / F3 pin | overlay keys | done |
| Ctrl+A full screen | done |
| Ctrl+Z/Y undo/redo | done |
| WASD move cursor | `overlay::move_cursor` | done |
| Arrows move / Shift shrink / Ctrl grow | `nudge_key` | done |
| `` ` `` toggle cursor | toast + `paint_pointer` | done | System cursor hidden while painted |
| V select, N number, 1/2 width | done |
| Double-click copy / edit text | done |
| `wheelEvent` → `ann.wheel` | `annot.wheel(angleDelta)` | done |
| `ConfirmAbort` in-overlay Y/N | native dialog | partial |
| `show_cursor` paint | `draw::paint_pointer` | done | Also stamped into copy/pin/save crop |
| Color copy then cancel | done | |

## Annotator (`annotate.py`)

| Python | Rust | Status |
|---|---|---|
| `Style` / `default_styles` / `Shape` | `annotation::{Style,Shape,Tool}` | done |
| `option_target` live\|selected\|edit\|width | `Annotator::option_target` + live branch | done |
| `option_tool` / `option_style` | `option_tool` | done |
| `adjust_width` + 0.5s undo merge | done |
| `wheel` trunc(angle/120) | `Annotator::wheel` | done |
| `start_text` / `commit_text` / `begin_edit_text` | done + painted dashed editor | done |
| `key` Esc/Enter/IME | overlay + pin + `Ime::Commit` | done |
| `press` select-on-click, mosaic no-steal | done |
| `_resize_selected` number/polyline/rect | done |
| `_push_edit` / undo 50 / redo | `UndoStack` | done |
| `_renumber` | done |
| `render` mosaic/blur/eraser | `render.rs` | done |
| Text glyphs / number digits | `font.rs` fontdue | done |
| Dash/dot line, fill, arrow, marker alpha | `render.rs` | done |
| Property panel (bold/italic/family/mode/effect) | toolbar swatches + width only | partial |
| Custom QColorDialog | missing | |
| Brush cursor poll timer | move events only | partial |
| Pixel-perfect hit via 3×3 paint | geometry hit | partial |

## Toolbar (`toolbar.py`)

| Python | Rust | Status |
|---|---|---|
| Tool order Rect Ellipse Line Arrow Pen Marker Mosaic Text Number | `toolbar.rs` | done |
| OCR / Translate / Undo Redo Close Pin Save Copy | done |
| Pin Check (done) | `TbAction::Check` | done |
| Palette 20 + width −/+ | done |
| Per-tool panel (filled, dash, blur, font…) | missing | Colors + width only |
| Tips / i18n keys | `Tool::tip` + `tr` | done |

## Pin window (`pin_window.py`)

| Python | Rust | Status |
|---|---|---|
| Frameless always-on-top + skip taskbar | winit + EWMH / WS_EX_TOOLWINDOW | done |
| Drag move, wheel zoom (top-left), Ctrl+wheel opacity | done |
| Annotating: wheel → `ann.wheel` not zoom | done |
| Space / right-click Show toolbar | done |
| TOOL_KEYS R E P T N … | R E P T N + 1/2/3/4 | done |
| Esc close / Ctrl+W | done |
| Double-click close/hide/none + tip | `PinWin::on_double_click` | done | First hit tips; `dblclick_tip_shown` persisted |
| Context menu full (`build_menu`) | Win32 popup / zenity list | partial | Groups/shadow/view-in-folder/destroy incomplete |
| Rotate / flip / paste / replace file | rotate/flip/paste done; replace file missing | partial |
| Shadow / thumb | missing | |
| Click-through XShape / WS_EX_TRANSPARENT | done |
| Groups move / new / rename | tray groups; pin menu partial | partial |
| Glow fade-in | `tick_glow` | done |

## Prefs / settings / theme / i18n

| Python | Rust | Status |
|---|---|---|
| `prefs.PrefsDialog` General/Hotkeys/Output + Advanced | `prefs.rs` egui | done |
| zh/en only, light/dark/system | done | Linux system theme = light |
| Hotkey conflict detection | `hotkey::find_conflicts` | done |
| Groups add/rename/remove | done |
| `theme.py` apply + watch | egui visuals | partial | No live Qt palette watch |
| `i18n.STRINGS` every key | `i18n.rs` PAIRS | done | Extra Phase-1 strings kept |
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
| `PynputHotkeys` | — | missing | XGrabKey is the Linux path |
| `WinHotkeys` | `win32::HotkeyHost` | done |

## Icons / session / groups / history

| Python | Rust | Status |
|---|---|---|
| `icons.py` app + toolbar | `icons.rs` | done |
| `session.py` groups + persist | `session.rs` + `groups.rs` | done |
| Overlay `HISTORY` | `history.rs` | done |

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
| done | 97 |
| partial | 13 |
| missing | 4 |
| impossible | 6 |
| **total** | **120** |

Partial/missing that still need Windows or Xfce hands-on: property panel, pin shadow/thumb, AT-SPI rect merge, zenity-less pin menu, custom color dialog, per-tool toolbar panel.
