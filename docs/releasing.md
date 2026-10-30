# Windows 发布流程

当前仅发布 Windows。集成分支为 `main`，C++ 实现保留在 `cpp`。

## 发布前检查

1. 确认源代码、安装脚本及文档一致，没有用户数据、密钥或本地缓存。
2. 对齐工作区版本和锁文件；正式标签采用 `v<版本>-windows-rust`。
3. 等待 `build-rust-windows` 的完整版与 Lite 检查通过。
4. 完成目标系统的必要人工验收，明确尚未覆盖的场景。
5. 确认产物来自同一待发布提交，不混用历史附件。

## 自动构建

GitHub Actions 执行普通构建、OCR 构建、Lite 测试、运行库审计、安装回归及打包。Lite 另检查禁用的 MCP 启动参数。

正式标签触发发布任务，收集两版安装包、便携包及校验清单。MCP Node 服务独立构建与分发，不应放入 Lite 安装包。

## 本地复现

仅在确实需要复现打包问题时使用。在仓库根目录执行：

```powershell
./tools/package-windows.ps1
./tools/package-windows.ps1 -Lite
```

需要 MSVC、Windows SDK 和 Inno Setup 6。不要覆盖正在使用的安装目录；测试使用独立配置。

## 产物核对

| 产物 | 内容 |
| --- | --- |
| `PinShot-<版本>-windows-x64-setup.exe` | 完整版安装包 |
| `PinShot-<版本>-windows-x64.zip` | 完整版便携包 |
| `PinShot-<版本>-windows-x64-lite-setup.exe` | Lite 安装包 |
| `PinShot-<版本>-windows-x64-lite.zip` | Lite 便携包 |
| `SHA256SUMS*.txt` | 对应文件的 SHA-256 校验 |

核对 `--version`、`--edition`、安装门槛、包内依赖及文件哈希。校验和不能替代代码签名；没有配置签名证书时，发布说明必须标明未签名，不保证系统不出现安全提示。

## 发布说明

说明版本差异、最低系统、配置保留、更新方式、已验证场景和已知限制。预发布版与正式版应明确区分，不把旧系统“允许安装”写成“已经实机验证”。

分支清理仅针对已合并或已备份的工作。完成后保留 `main` 和 `cpp`；发布标签仍保留作为可追溯记录。
