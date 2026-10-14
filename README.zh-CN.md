# PinShot Rust

使用 Rust 实现的桌面截图、标注与贴图工具。此分支只维护 Rust 版本；旧 Python/Qt 实现保留在 Git 历史中。

## 下载与安装

在 [Releases](https://github.com/Tanglj8472/pinshot/releases) 下载 Windows x64 安装包 `PinShot-*-windows-x64-setup.exe`，或解压便携 ZIP 后运行 `pinshot.exe`。便携包中的 DLL 必须与程序放在一起。

Windows 发布包包含 OCR 支持，首次识别需要联网下载模型，之后可离线识别。翻译会向设置中选择的服务发送文字。安装包未代码签名，Windows 可能显示 SmartScreen 提示，请核对发布页及 SHA-256。

## 功能与操作

- 区域截图、窗口识别、复制、保存与置顶贴图。
- 矩形、椭圆、直线、箭头、画笔、记号笔、文字、编号、模糊和马赛克。
- 直线/箭头：拖动绘制单段；单击起点、继续单击添加转折点，右键结束。
- 记号笔和模糊/马赛克提供笔涂抹与区域填充；左侧粗细控件兼作笔模式按钮。笔迹不可选中或拖动，支持撤销。
- 模糊/马赛克共用效果切换按钮；滑条控制效果强度，滚轮调整笔触粗细。
- 系统字体下拉选择，加粗、斜体、描边；文字框左上、左下、右下角等比例缩放，右上角 × 删除。
- PP-OCRv5 本地识别及可配置的在线翻译。

默认 `F1` 截图、`F3` 贴出剪贴板、`Esc` 退出当前操作；快捷键可在设置中修改，已有用户配置优先。标注完成后右键取消选中。

## 从源码运行

Windows 需要 Rust stable、Visual Studio C++ Build Tools 和 Windows SDK。

```powershell
cd rust
cargo run --release --bin pinshot --features ocr
cargo run --release --bin pinshot --features ocr -- --debug
cargo test --workspace --features ocr
```

Windows 不需要 OCR 时可去掉 `--features ocr`。Linux 当前支持 X11，依赖与命令见 [Rust 开发说明（中文）](rust/README.zh-CN.md)。

## 打包 Windows 安装程序

安装 Inno Setup 6 后，在仓库根目录执行：

```powershell
powershell -ExecutionPolicy Bypass -File tools/package-windows.ps1
```

脚本测试并构建 OCR 版本，收集运行库，生成安装包、便携 ZIP 和校验清单到 `dist/`。详见 [发布流程](docs/releasing.md)。

## 项目结构

```text
rust/crates/pinshot-core/  标注、渲染、OCR 数据与配置
rust/crates/pinshot/       窗口、输入、截图、托盘与平台实现
assets/                   应用图标
tools/                    打包脚本
packaging/                安装程序定义
.github/workflows/        Windows / Linux 构建验证
docs/                     维护与发布说明
```

Windows 配置、日志和数据位于 `%APPDATA%\PinShot`。Linux 遵循 XDG 配置与数据目录。卸载默认保留用户数据。

开发约定见 [CONTRIBUTING.md](CONTRIBUTING.md)。Cargo 元数据声明 MIT；第三方依赖遵循各自许可证，见 [第三方说明](docs/third-party.md)。
