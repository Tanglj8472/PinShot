# PinShot（Rust / Windows）

这是 PinShot 的 **Windows-only Rust 重写**（Phase 1）。行为以仓库里现有的 Python/PySide6 实现为参考，尤其是：

- `pinshot/overlay.py` 截图遮罩
- `pinshot/annotate.py` 标注引擎
- `pinshot/pin_window.py` 贴图窗口
- `pinshot/toolbar.py` 工具栏
- `pinshot/platform/windows.py` Win32 后端
- `pinshot/i18n.py` 中英文本

Python 代码**未改动**。本目录是独立的 Cargo workspace。

## 技术栈

| 层 | 选择 |
|---|---|
| 窗口 | `winit` |
| 自绘 | `tiny-skia` + `softbuffer` |
| Win32 | 官方 `windows` crate：DXGI Desktop Duplication（GDI `BitBlt` 兜底）、Per-Monitor DPI v2、`RegisterHotKey`、`WS_EX_LAYERED / TOPMOST / TOOLWINDOW / TRANSPARENT` |
| 托盘 | `tray-icon` |
| 剪贴板 | `arboard` |
| 配置 | `%APPDATA%\PinShot\config.toml` |

图标全部由 `pinshot-core` 按原版 `icons.py` 自绘，没有第三方素材。

## 构建

要求：Windows 10/11、[Rust](https://rustup.rs/) stable、MSVC 工具链（`x86_64-pc-windows-msvc`）。

```powershell
cd rust
cargo test --workspace
cargo build --release
.\target\release\pinshot.exe
```

常用参数：

```powershell
.\target\release\pinshot.exe --version   # PinShot 0.1.0 (windows-rust)
.\target\release\pinshot.exe --snip      # 启动后立刻截图
.\target\release\pinshot.exe --pin       # 启动后立刻贴剪贴板
.\target\release\pinshot.exe --dump-capture --snip
# 把合成后的捕获帧写成 %TEMP%\pinshot-capture.png（也可用环境变量 PINSHOT_DUMP_CAPTURE）
```

Linux 上可以跑纯逻辑测试（窗口层被 `cfg(windows)` 裁掉）：

```sh
cd rust
cargo test -p pinshot-core
```

交叉检查（本机无 MSVC 链接器时用 check，不生成 exe）：

```sh
rustup target add x86_64-pc-windows-msvc
cargo check --target x86_64-pc-windows-msvc
```

CI：推送到 `rust` 分支会跑 `.github/workflows/build-rust-windows.yml`（`windows-latest`：`cargo test` + `cargo build --release`，上传 `pinshot.exe` 产物）。

## 使用（Phase 1）

常驻托盘。默认语言 **简体中文**（`config.toml` 里 `language = "zh_CN"`，可改 `en`）。

| 操作 | 行为 |
|---|---|
| 托盘左键 / **F1** | 全屏冻结截图 |
| **F3** | 无截图时：贴剪贴板（图 / 颜色值 / 文本卡片）；截图中：把选区贴到屏幕 |
| 拖拽 | 框选；点一下采用窗口自动检测高亮 |
| 选区内拖 / 八个手柄 | 移动 / 缩放选区 |
| 工具栏 | 矩形、椭圆、直线、箭头、铅笔、记号笔、马赛克、文字、序号、撤销/重做、颜色/粗细 |
| **Ctrl+C** / 双击选区 | 复制 |
| **Ctrl+S** | 另存为 PNG/JPEG |
| **Ctrl+T** / F3 | 贴图 |
| **Esc** | 取消（有选区时确认） |
| 贴图：拖动 / 滚轮 / **Ctrl+滚轮** | 移动 / 缩放（左上锚点） / 透明度 |
| 贴图右键 | 复制、另存为、缩放、关闭、鼠标穿透 |
| 贴图 **Esc** | 关闭（可恢复栈） |
| **Shift+F3** | 隐藏/显示全部贴图 |

放大镜：15×15 @ 8×，坐标 + RGB/HEX（Shift 切换），C 复制颜色。

## 配置

`%APPDATA%\PinShot\config.toml`（首次运行按默认值创建）。主要字段：

```toml
schema_version = 1
language = "zh_CN"          # 或 en
hotkey_snip = "F1"
hotkey_pin = "F3"
hotkey_hide_all = "Shift+F3"
image_quality = 92
skip_abort_confirm = false
```

## 仓库结构

```
rust/
  Cargo.toml                 # workspace
  crates/pinshot-core/       # 几何、标注、撤销、i18n、配置、图标、hooks（可单测）
  crates/pinshot/            # Windows 应用：托盘 / 捕获 / 遮罩 / 贴图
```

单元测试覆盖：几何、选区状态机、标注命中/拖动/撤销、序号重排、i18n、配置 TOML、热键解析、马赛克合成、自绘图标。

## 阶段计划

| 阶段 | 内容 | 状态 |
|---|---|---|
| **P1** | 托盘 + F1/F3、全屏遮罩（变暗/框选/手柄/窗口高亮/尺寸/放大镜）、标注工具栏、复制/保存/贴图、贴图拖动缩放透明度穿透、中英 i18n、TOML 配置、Windows CI | **本目录** |
| P2 | egui 设置窗、完整子面板（填充/虚线/字体）、贴图上再标注、延时截图、历史选区 | hook：`SettingsUi` |
| P3 | UI Automation 控件检测（Tab 切窗口/元素）、混合 DPI 每屏独立 overlay | hook：`ElementDetector` |
| P4 | `ort` + PP-OCRv5 本地 OCR | hook：`OcrEngine` |
| P5 | 翻译、分组/会话恢复、开机自启、安装包 | hook：`Translator` |

## 已实现 vs 占位

**已实现（P1）**

- 托盘菜单 + `RegisterHotKey`（F1 / F3 / Shift+F3）
- DXGI 整屏捕获（按适配器建 D3D 设备、拒绝全黑首帧）+ GDI `CreateDIBSection`/`BitBlt(CAPTUREBLT)` 兜底 + 每监视器 `CreateDC` + `PrintWindow`；多显示器拼成虚拟桌面；GDI 32 位 DIB 的 alpha=0 按不透明处理，避免预乘变黑
- Per-Monitor DPI Aware v2
- 遮罩：变暗挖洞、拖拽、移动/缩放手柄、`EnumWindows` + DWM 边框高亮、尺寸标签、放大镜 RGB/HEX
- 自绘工具栏（原版图标）+ 20 色盘 + 线宽
- 复制 / 保存 / 贴图 / Esc
- 贴图：拖动、滚轮缩放、Ctrl+滚轮透明度、右键菜单、`WS_EX_TRANSPARENT` 穿透、Esc 关闭
- 中英 `tr()`，默认中文
- `%APPDATA%\PinShot\config.toml`

**占位 / 未做（有 trait/模块，不在 P1 验收）**

- `SettingsUi`：egui 首选项窗口
- `ElementDetector`：UI Automation 元素级检测（Tab 目前只切换模式提示）
- `OcrEngine`：`ort` + PP-OCRv5
- `Translator`：划词/截图翻译
- 选择工具 / 橡皮擦完整编辑（核心模型已有，工具栏未放）
- 截图历史 `,` `.`、贴图分组、会话备份、开机自启、自定义颜色对话框

## 许可证

与仓库其余部分相同。
