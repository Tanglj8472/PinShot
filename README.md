# PinShot

面向 Windows 的开源截图与屏幕标注工具，使用 Rust 构建。支持区域截图、多显示器、即时标注、桌面贴图及图片保存；完整版另提供 OCR、翻译和 MCP 自动化。

[下载版本](https://github.com/Tanglj8472/pinshot/releases) · [文档导航](docs/README.md) · [报告问题](https://github.com/Tanglj8472/pinshot/issues)

## 选择版本

| 功能与要求 | 完整版 | Lite 精简版 |
| --- | --- | --- |
| 截图、标注、马赛克与模糊 | 支持 | 支持 |
| 贴图、剪贴板、图片保存 | 支持 | 支持 |
| OCR 与模型下载 | 支持 | 不包含 |
| 截图翻译、划词翻译 | 支持 | 不包含 |
| MCP 自动化 | 支持，需显式启用 | 不包含 |
| 最低 Windows 内部版本 | 19041（Windows 10 2004） | 17763（Windows 10 1809） |
| 更新方式 | 应用内更新或手动安装 | 手动安装对应 Lite 包 |

发布目标为 Windows x64 兼容架构。Lite 的安装门槛包含企业版 LTSC 2019；实际运行仍需按[验收清单](docs/windows-validation.md)测试。低于 17763 的系统不在当前支持范围。既有 Linux 源码保留，但不再自动构建或发布 Linux 版。

## 主要功能

- 多显示器与高 DPI 截图，支持窗口区域识别。
- 矩形、椭圆、直线、箭头、画笔、记号笔、序号、文字及橡皮擦。
- 马赛克与模糊的笔刷、区域模式，以及常用工具参数记忆。
- 剪贴板复制、图片保存、截图历史和可拖动、缩放的贴图。
- DXGI 屏幕捕获，在不兼容环境回退到 GDI。
- 可配置快捷键、保存文件名、语言、主题与分级日志。

部分标注仅在刚创建后的编辑阶段可调整；画笔等笔迹不可选中，并非所有标注都能一直编辑。

## 安装与使用

在 Releases 中按需选择版本；名称含 `-lite` 的文件属于精简版。

| 文件 | 用途 |
| --- | --- |
| `*-setup.exe` | 当前用户安装包 |
| 桌面应用 `*.zip` | 便携包，解压后运行并保留随包 DLL |
| `pinshot-mcp-*.zip` | 独立 MCP 服务包，仅供完整版使用 |

左键点击托盘图标开始截图，右键打开菜单。快捷键可在设置中调整。完整版首次 OCR 需要联网下载模型，模型不随安装包分发。

两版共享安装目录和配置，安装时互相替换，不支持同时运行。安装、升级和卸载保留用户数据。详见[安装与升级](docs/install-upgrade.md)。

## 从源码构建

需要 Rust stable、MSVC C++ 构建工具及 Windows SDK。以下命令在仓库根目录执行：

```powershell
# 完整版
cargo run --manifest-path rust/Cargo.toml --release --bin pinshot --features ocr -- --debug

# Lite
cargo run --manifest-path rust/Cargo.toml --release --bin pinshot --no-default-features --features lite -- --debug
```

仅省略 `ocr` 不等同于 Lite：普通开发构建仍保留翻译和 MCP。详见[开发说明](rust/README.md)。

## 配置与诊断

默认配置和数据目录为 `%APPDATA%\PinShot`。日志按 UTC 日期写入 `pinshot-YYYY-MM-DD.txt`；保留规则和已知限制见[日志与诊断](rust/docs/logging.md)。

报告问题时请提供应用版本、`--edition` 输出、Windows 内部版本、操作步骤及脱敏日志，不要附带 API 密钥。

## 项目与许可

- `main`：Rust 桌面应用、Lite 和 MCP 服务。
- `cpp`：独立保留的 C++ 实现。
- `rust/crates/pinshot-core`：配置、标注、渲染和通用逻辑。
- `rust/crates/pinshot`：平台集成与桌面交互。
- `pinshot-mcp`：完整版的 Node.js MCP 协议服务。

Rust 工作区声明使用 MIT 许可证。第三方声明见[第三方组件](docs/third-party.md)，发布包中的许可证文件必须保留。参与开发前请阅读[贡献指南](CONTRIBUTING.md)。
