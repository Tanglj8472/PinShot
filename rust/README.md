# PinShot（Rust）

Python/PySide6 的跨平台重写：`pinshot-core` 可移植，宿主在 **Linux X11** 与 **Windows** 上通过 `native` 平台层分叉。

行为对照表见 [`PARITY.md`](PARITY.md)。Python 源码未改。

## 技术栈

| 层 | 选nihn
|---|---|
| 窗口 | `winit` 0.30 |
| 自绘 | `tiny-skia` + `softbuffer`（Linux）；Windows 遮罩另用脏矩形 GDI） |
| Linux | `x11rb`：`XGetImage` / MIT-SHM、`XGrabKey`、override-redirect、EWMH ABOVE + skip taskbar、XShape 穿透 |
| Windows | 官方 `windows` crate：DXGI + GDI、`RegisterHotKey`、分层窗口 |
| 托盘 | `tray-icon`（Linux 无托盘时用小控制窗，与 Python `_build_fallback` 相同） |
| 剪贴板 | `arboard`（含 image/png） |
| 配置 | Linux：`~/.config/PinShot/config.toml`；数据：`~/.local/share/PinShot`。Windows：`%APPDATA%\PinShot` |

## Linux 本地构建（Xfce / xfwm4，x86_64）

```sh
sudo apt-get update
sudo apt-get install -y \
  build-essential pkg-config \
  libx11-dev libx11-xcb-dev libxkbcommon-dev libxkbcommon-x11-dev libxcb-xkb-dev \
  libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
  libxcb-randr0-dev libxcb-shm0-dev libxrandr-dev libxi-dev \
  libxext-dev libxcursor-dev libxinerama-dev libxdo-dev \
  libgtk-3-dev libayatana-appindicator3-dev \
  libclang-dev fonts-wqy-microhei

cd rust
cargo test --workspace
cargo build --release --target x86_64-unknown-linux-gnu
./target/x86_64-unknown-linux-gnu/release/pinshot --version
# PinShot 0.1.0 (linux-rust)

# 无 DISPLAY 的机器只编不跑。有 X11 桌面时：
./target/x86_64-unknown-linux-gnu/release/pinshot --debug
# 或
PINSHOT_DEBUG=1 ./target/release/pinshot --snip
```

没有系统托盘时会出现 **PinShot** 小窗（截图 / 贴剪贴板 / 首选项 / 退出），快捷键 F1 / F3 也绑在这个窗上；全局热键走 `XGrabKey`。

日志：`~/.local/share/PinShot/pinshot.log`。`--debug` 同时打到 stderr。

开机自启：`~/.config/autostart/pinshot.desktop`（首选项里「登录时启动」）。

## Windows 构建

要求：Windows 10/11、Rust stable、MSVC（`x86_64-pc-windows-msvc`）。

```powershell
cd rust
cargo test --workspace
cargo build --release
.\target\release\pinshot.exe --version   # PinShot 0.1.0 (windows-rust)
```

Release exe 使用 `windows_subsystem = "windows"`（无控制台）。`--version` / `--debug` / `PINSHOT_DEBUG=1` 会 Attach/Alloc 控制台。日志在 `%APPDATA%\PinShot\pinshot.log`。

## 常用参数

```
--snip / --pin-clipboard / --prefs / --translate
--debug
--dump-capture          # 把捕获帧写成临时目录 pinshot-capture.png
--version
```

## 仓库结构

```
rust/
  PARITY.md              # Python 函数/菜单/快捷键/i18n → Rust 对照
  crates/pinshot-core/   # 几何、标注、撤销、i18n、配置、字体、OCR 路径
  crates/pinshot/        # 宿主：native + windows / linux_x11
```

## CI

- `.github/workflows/build-rust-windows.yml` → `pinshot.exe`、`pinshot-ocr.exe`
- `.github/workflows/build-rust-linux.yml` → `pinshot-linux`

## 许可证

与仓库其余部分相同。
