# PinShot Lite 精简版

## 需求与边界

增加独立构建的精简版，保留截图、画笔及其他标注、模糊/马赛克、贴图、保存和现有 MCP；不包含文字识别、模型下载、截图翻译、划词翻译或翻译服务商请求。

完整版不变。Lite 使用 Cargo feature `lite`，禁止与 `ocr` 及派生 GPU OCR feature 同时启用。OCR 和翻译执行模块替换为无网络、无推理的禁用模块；界面隐藏对应菜单、工具栏、设置项和全局快捷键。

## 构建与打包

```powershell
cd rust
cargo test --locked --workspace --features lite
cargo build --locked --release --bin pinshot --no-default-features --features lite
cd ..
./tools/package-windows.ps1 -Lite
```

`pinshot.exe --edition` 输出 `lite` 或 `full`，打包时验证，防止误把完整版放入 Lite 包。Lite 使用独立构建目录，包名带 `-lite`，不复制 OCR/GPU DLL，检查可执行文件和附带 CRT 的依赖，发现 ONNX、DXCore、DirectML、Dawn、CUDA 等依赖即失败。

## Windows 兼容与安装

Lite 安装门槛为 Windows 10 1809 / Build 17763（包括企业版 LTSC 2019），仅 x64 兼容架构。完整版仍要求 Build 19041。移除 OCR 依赖不等于已完成旧系统真机验收：GitHub Windows 构建机不是 LTSC 1809，需在目标系统验证启动、截图、标注、贴图和 MCP。

两版共享原有安装目录和用户配置，安装时互相替换，不支持同时运行；切换到 Lite 不删除原有翻译配置、历史或模型。旧目录即使留有 OCR DLL，Lite 也不会加载它们。Lite 不通过完整版自动更新通道安装更新，暂由用户手动下载安装对应 Lite 包。

## 验证清单

- 完整版、普通无 OCR 构建和 Lite 构建分别验证，原有无 OCR 构建不等同 Lite（仍可有翻译）。
- Lite 工具栏及贴图右键菜单不出现 OCR/翻译；设置隐藏 OCR 与翻译配置；不注册两个翻译快捷键。
- 通过 CLI、IPC 或旧会话触发禁用功能时，不发起模型下载、推理或翻译请求。
- 包内无推理运行库，依赖审计通过；安装门槛匹配 17763。
- 在 LTSC 1809 实机验证前，不宣称该系统已完成验收。
