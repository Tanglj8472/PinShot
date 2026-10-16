# Windows 安装与运行验证

当前 x64 发布版本最低要求 Windows 10 2004（内部版本 19041），也支持 Windows 11。程序导入的 DXCore 从该版本起由系统提供，安装程序会拒绝更旧的系统，避免安装后出现缺少系统 DLL。依据：[Microsoft DXCore 系统要求](https://learn.microsoft.com/en-us/windows/win32/dxcore/dxcore)。

安装默认使用当前用户目录，不需要管理员权限。首次安装和交互式升级均显示目录选择页，可修改路径。后续安装默认读取上次目录；应用内静默更新显式传递当前程序目录。卸载保留配置、历史与 OCR 模型，发布包不附带模型。

打包脚本调用 `tools/test-windows-runtime.ps1` 检查程序和每个 DLL 的 x64 架构、导入依赖。非 Windows 系统依赖必须随包提供，不能借用开发机的 PATH 或已安装的 Visual C++ 运行库。检查清单随包保存为 `runtime-dependencies.txt`。此检查覆盖静态和延迟导入；动态加载、显卡驱动和界面行为仍须在干净 Windows 中验证。

## 全新系统验收

使用全新 Windows 10/11 x64 虚拟机，不安装 Rust、Python、Visual Studio、VC++ 运行库或 WebView2：

1. 复制安装包，选择含空格的自定义目录。
2. 启动设置、截图、标注、保存、复制和贴图。
3. 联网下载 OCR 模型并识别，随后断网重测 OCR。
4. 保持程序运行执行升级，确认关闭旧进程、替换文件和配置保留。
5. 再次安装时确认默认路径沿用自定义目录。
6. 卸载并检查配置、模型和历史仍保留。

未完成该实测时，发布记录必须明确说明；开发机运行成功和依赖检查不能代替全新系统验收。

发布包内附带 `test-windows-clean-runtime.ps1`。在全新 Windows 的 PowerShell 中进入解压目录后运行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\test-windows-clean-runtime.ps1 -PayloadDir .
```

此脚本仅依赖 Windows 自带 PowerShell，使用仅含系统目录的 PATH 和独立配置目录启动设置窗口，并核对四个 MSVC DLL 实际从发布目录加载。报告路径显示在终端。它不替代上面的截图交互和 OCR 验收。

## Windows Sandbox 验收

宿主机启用 Windows Sandbox 后，可从仓库根目录生成隔离验收配置：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/new-windows-sandbox-validation.ps1 -Installer dist/PinShot-0.2.6-windows-x64-setup.exe -PortableZip dist/PinShot-0.2.6-windows-x64.zip -Checksums dist/SHA256SUMS.txt
```

打开输出的 `.wsb` 文件后，沙盒自动校验哈希、验证便携包和安装版的设置窗口及运行库加载、自定义目录安装、运行中覆盖升级。输入文件只读映射，只有独立的结果目录允许写回宿主机；不会映射用户配置或开发工具。此生成脚本不会启用系统功能或重启电脑。

结果中的 `PENDING` 项需要继续在沙盒内操作并验收，不能将自动检查通过当作截图及 OCR 已通过。沙盒配置依据：[Microsoft Windows Sandbox 配置说明](https://learn.microsoft.com/windows/security/application-security/application-isolation/windows-sandbox/windows-sandbox-configure-using-wsb-file)。
