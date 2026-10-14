# Windows 发布流程

1. 合并到 `rust`，统一 `rust/Cargo.toml` 和 Cargo.lock 的版本。
2. 安装 Rust/MSVC 与 Inno Setup 6。
3. 在仓库根目录执行 `powershell -ExecutionPolicy Bypass -File tools/package-windows.ps1`。
4. 检查 `dist/` 的安装包、ZIP、SHA256SUMS.txt。ZIP 解压至干净目录，验证 `pinshot.exe --version` 与 GUI 启动。安装验证不得覆盖正在使用的安装目录。
5. 手动检查截图、笔迹、文字缩放/删除、重叠模糊及 OCR。首次 OCR 需下载模型。
6. 检查 Windows / Linux CI，为已验证提交创建 `v<版本>-windows-rust` 标签。
7. 创建 GitHub Release，上传安装包、便携包、校验清单，记录实际测试范围与未签名状态。

安装采用当前用户目录，不需要管理员权限。卸载保留配置、历史与 OCR 模型。打包脚本不附带模型。
