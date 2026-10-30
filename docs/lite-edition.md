# Lite 精简版

Lite 面向只需要手动截图与标注的 Windows 用户，不包含 OCR、翻译和 MCP。

## 功能边界

| 保留 | 不包含 |
| --- | --- |
| 截图、标注、画笔、文字与序号 | OCR 推理、模型下载 |
| 马赛克、模糊与橡皮擦 | 截图翻译、划词翻译、服务商请求 |
| 贴图、剪贴板、图片保存与历史 | MCP 服务、自动化会话及工具 |

完整版不受此裁剪影响，仍可显式启用 MCP。Lite 不编译 MCP 服务、自动化事件和会话窗口；`--automation` 与 `--capture-to` 返回退出码 2，不会启动后台服务。

## 兼容范围

安装最低要求为 Windows 10 1809 / Build 17763，包含企业版 LTSC 2019，仅支持 x64 兼容架构。低于 17763 的系统未纳入支持范围。

移除推理库使安装包不再依赖其 DXCore 组件，但仍需要在 LTSC 1809 实机验证启动、截图、标注、贴图和保存。GitHub 新版 Windows 构建机不能替代这项验收。

## 构建与分发

在仓库根目录执行：

```powershell
cargo test --locked --manifest-path rust/Cargo.toml --workspace --features lite
./tools/package-windows.ps1 -Lite
```

Lite 使用独立构建目录，不得与 `ocr` 或 GPU OCR feature 同时启用。仅省略 `ocr` 不能生成 Lite。

打包脚本通过 `--edition` 校验版本，不复制 OCR/GPU DLL，并审计 EXE 与 CRT 的导入依赖。发现 ONNX、DXCore、DirectML、Dawn 或 CUDA 等依赖时，打包失败。

## 安装与更新

Lite 与完整版共享安装目录和配置，切换安装会互相替换，不支持同时运行。原有配置、历史和模型不会被删除。Lite 暂时仅支持手动下载对应包更新。

详见[安装与升级](install-upgrade.md)、[发布流程](releasing.md)和[验证记录](validation.md)。
