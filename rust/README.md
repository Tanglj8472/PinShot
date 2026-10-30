# Windows 开发说明

工作区包含 `pinshot-core` 和 `pinshot`。功能入口见[项目 README](../README.md)，完整索引见[文档导航](../docs/README.md)。

## 环境

- Rust stable、Windows x64 MSVC 工具链、C++ 构建工具及 Windows SDK。
- 打包需要 Inno Setup 6。
- MCP 服务开发需要 Node.js 20 或更高版本。

OCR 构建可能联网下载 ONNX Runtime 依赖；OCR 模型在运行时单独下载。

## 构建与测试

在 `rust` 目录执行：

```powershell
# 完整版
cargo test --locked --workspace --features ocr
cargo build --locked --release --bin pinshot --features ocr

# Lite
cargo test --locked --workspace --features lite
cargo build --locked --release --bin pinshot --no-default-features --features lite

# 普通开发构建，不等同于 Lite
cargo test --locked --workspace
```

Lite 不能与 `ocr` 或 GPU OCR feature 同时启用。`pinshot.exe --edition` 输出 `full` 或 `lite`。

## 打包

在仓库根目录执行：

```powershell
./tools/package-windows.ps1
./tools/package-windows.ps1 -Lite
```

两个版本使用独立构建目录；打包脚本验证版本身份、依赖及文档内容。分发时不能只复制开发目录中的 EXE。

## 运行与诊断

常用参数为 `--snip`、`--pin-clipboard`、`--prefs`、`--debug`、`--version`、`--edition`。

完整版支持 `--automation` 和 `--capture-to <output.png>`；Lite 拒绝这两个参数。参见[MCP 接入](../pinshot-mcp/README.md)。

测试可用 `PINSHOT_CONFIG_DIR` 和 `PINSHOT_DATA_DIR` 指向独立目录，避免改动日常配置。既有 Unix/X11 源码保留，但当前只构建并发布 Windows。
