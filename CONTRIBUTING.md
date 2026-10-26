# 贡献指南

当前维护 Windows Rust 应用及 MCP 服务，集成分支为 `main`。`cpp` 保存独立 C++ 实现。Linux 源码暂时保留，但不属于当前自动构建与发布目标。

## 开发原则

- 通用数据、配置和渲染放在 `pinshot-core`。
- 平台 API、窗口和交互放在 `pinshot`。
- MCP 协议放在 `pinshot-mcp`，捕获与标注继续复用 Rust。
- 交互改动同时检查截图浮层和贴图窗口，补充回归测试。
- 功能裁剪同时验证完整版与 Lite，不以禁用测试或全局忽略警告掩盖问题。
- 不提交密钥、用户截图、配置、日志、模型、编译产物或本地工具缓存。

## 提交前验证

在仓库根目录执行：

```powershell
cargo test --locked --manifest-path rust/Cargo.toml --workspace
cargo test --locked --manifest-path rust/Cargo.toml --workspace --features ocr
cargo test --locked --manifest-path rust/Cargo.toml --workspace --features lite
```

修改 MCP 时，在 `pinshot-mcp` 目录执行 `npm ci`、`npm run build` 和 `npm run smoke`。真实截图流程测试需要 Windows 交互桌面及启用自动化的完整版，不能针对 Lite 运行。

## 合并要求

1. 说明问题、改动范围、兼容性影响和验证结果。
2. 更新关联文档，并检查相对链接。
3. 等待 Windows 构建、测试和安装检查通过后合并。
4. 明确记录未完成的实机验收。
5. 清理已合并临时分支前，核对是否存在未合并的独有提交。

相关说明：[开发](rust/README.md)、[发布](docs/releasing.md)、[验收](docs/windows-validation.md)。
