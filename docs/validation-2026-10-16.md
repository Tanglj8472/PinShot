# 本地发布候选验证记录（2026-10-16）

本次候选构建版本为 0.2.6，包含当前工作区改动，不等同于已经公开发布的同版本安装包。本次没有修改 GitHub Release。

## 已验证

- `cargo test --locked --workspace`：应用 89 项、核心库 247 项通过，核心库另有 2 项忽略。
- `cargo test --locked --workspace --features ocr`：应用 89 项、核心库 247 项通过，核心库另有 2 项忽略。
- 独立目录 `rust/target/package-windows` 的 release/OCR 构建成功；随后重新执行构建，无需重新编译，确认覆盖最后的源码改动。
- 发布目录 12 个 EXE/DLL 均为 x64，导入依赖均来自包内或明确列出的 Windows 系统组件。
- 从测试载荷移除 VC++ 运行库后，依赖门禁正确拒绝，报缺少 `msvcp140_1.dll`，没有借用开发机已安装的运行库。
- 自定义含空格目录首次安装、运行中覆盖升级、重复安装沿用原目录、关闭辅助程序失败时中止安装、静默更新后重启、自动更新传递当前目录与重启参数均通过。
- 安装和卸载后，测试配置、历史图像和模型文件的哈希保持不变；测试前已运行的其他 PinShot 实例保持运行。
- 当前构建的辅助程序另行通过覆盖升级和自动更新参数测试。
- 打包扫描拒绝了本机加密软件造成的密文 README。修复后仅对 Git 中未修改的文档提取原文，重新打包通过密文扫描。
- 最终 ZIP 解压后，用仅含 Windows 系统目录的 PATH 和独立配置启动设置窗口；四个 MSVC 运行库均实际从解压目录加载。
- 实际运行产生了带 UTC 毫秒时间、等级、进程号、源文件和行号的日志。
- 安装包与 ZIP 的 SHA-256 均与清单一致。

## 候选文件

目录：`dist/`。

```text
486642a5d7c1cc992b42cbf265cce1f788100a8032fcbf3a4d60b599945d1542  PinShot-0.2.6-windows-x64-setup.exe
9ee21f7964ca2425138aefc71481a88df1cc7003271949e9afd63a15ee030284  PinShot-0.2.6-windows-x64.zip
```

ZIP 启动报告：`C:/Users/Administrator/AppData/Local/Temp/pinshot-runtime-9e6457cad8084964a96b934402e25d37/runtime-smoke.txt`。

## 尚未验证

本机为 Windows 11（内部版本 26200），带有开发工具和显卡驱动。限制 PATH 及核对运行库加载位置不等同于全新系统验证。

尚需在全新 Windows 上完成安装包实际安装、截图/标注/贴图交互、首次 OCR 模型下载、离线识别与升级验收。步骤见 `docs/windows-validation.md`。当前包使用 DXCore，安装最低版本已设为 Windows 10 2004（19041）。

纯净系统验收及新版本发布仍未完成；不得将本记录解读为已经证明所有全新 Windows 机器均可运行。

## 沙盒验收准备

准备阶段查询到本机 `Containers-DisposableClientVM` 和 `Microsoft-Hyper-V-All` 均为 `Disabled`。

新增沙盒生成脚本与来宾验收脚本，均通过 Windows PowerShell 5.1 语法检查。已生成 `dist/sandbox-validation-20261016/PinShot-validation.wsb`，输入包只读映射，报告使用独立的可写目录。实际沙盒尚未运行，不能视为来宾安装测试通过。启用沙盒或提供可用的干净虚拟机后，才能继续此项验收。

用户随后授权启用 Windows Sandbox，并明确要求不自动重启。已执行 `Enable-WindowsOptionalFeature -Online -FeatureName Containers-DisposableClientVM -All -NoRestart`，进程退出码为 0；系统返回 `RestartNeeded: true`，再次查询功能状态为 `Enabled`。未触发重启，尚未启动沙盒。剩余验收需等待用户手动重启电脑后继续。
