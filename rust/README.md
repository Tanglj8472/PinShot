# Rust 开发说明

功能及下载入口见 [项目 README](../README.md)。工作区含 `pinshot-core`（逻辑与软件渲染）和 `pinshot`（桌面应用）。

## Windows

安装 Rust stable、MSVC C++ 工具链及 Windows SDK，在本目录运行：

```powershell
cargo test --workspace
cargo test --workspace --features ocr
cargo build --release --bin pinshot --features ocr
cargo run --release --bin pinshot --features ocr -- --debug
```

OCR 构建需要联网获取 ONNX Runtime 构建依赖。分发请使用 `../tools/package-windows.ps1` 收集 EXE 与动态库，不要只复制 EXE。

## Linux（X11）

Ubuntu 24.04 构建依赖：

```sh
sudo apt-get install build-essential pkg-config libx11-dev libx11-xcb-dev \
  libxkbcommon-dev libxkbcommon-x11-dev libxcb-xkb-dev libxcb1-dev \
  libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxcb-randr0-dev \
  libxcb-shm0-dev libxrandr-dev libxi-dev libxext-dev libxcursor-dev \
  libxinerama-dev libxdo-dev libgtk-3-dev libayatana-appindicator3-dev \
  libclang-dev libssl-dev fonts-wqy-microhei
cargo test --workspace
cargo build --release --bin pinshot
./target/release/pinshot --version
```

Linux 平台默认包含 OCR；目前不提供原生 Wayland 截图。CI 的依赖列表见 `.github/workflows/build-rust-linux.yml`。

## 命令行与数据

常用参数：`--snip`、`--pin-clipboard`、`--prefs`、`--translate`、`--debug`、`--version`。

Windows 使用 `%APPDATA%\PinShot`；Linux 使用 XDG 配置/数据目录，默认分别为 `~/.config/PinShot` 与 `~/.local/share/PinShot`。模型位于数据目录的 `ocr-models` 中，可用 `PINSHOT_OCR_MODELS` 指定已有模型来源目录。

`PARITY.md` 与 `PROGRESS.md` 为迁移历史，不作为当前功能清单或发布检查依据。
