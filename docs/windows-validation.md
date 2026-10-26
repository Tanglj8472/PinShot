# Windows 验收清单

构建成功和依赖审计不等于实际桌面功能已通过。每次发布都应记录包名、源码提交、系统内部版本与测试结果。

## 验收环境

| 环境 | 适用版本 | 重点 |
| --- | --- | --- |
| 干净 Windows 10 2004+ x64 | 完整版、Lite | 无开发工具时的安装和启动 |
| Windows 11 x64 | 完整版、Lite | 常规功能和升级回归 |
| Windows 10 LTSC 2019 / 17763 | Lite | 旧系统实际运行 |
| 多显示器及不同 DPI | 两版 | 框选坐标、贴图缩放与拖动 |
| 远程或虚拟显示环境 | 两版 | DXGI 失败后的 GDI 回退 |

LTSC 1809 的实际验收尚未完成，不能根据 CI 通过宣称已验证。当前安装目录固定为当前用户的 Programs 目录，不再测试安装向导选择自定义目录的旧流程。

## 通用操作

1. 在未安装 Rust、Visual Studio 或额外 VC++ 运行库的环境运行安装包。
2. 打开设置，确认版本和语言；测试托盘截图、框选、标注、复制、保存及贴图。
3. 测试文字缩放、撤销重做、笔迹和重叠马赛克／模糊。
4. 保持应用运行进行覆盖安装，确认旧进程退出、文件替换和数据保留。
5. 卸载后确认配置与历史保留；需要清理数据时由用户自行确认。
6. 使用便携包时核对 `runtime-dependencies.txt` 及程序实际加载的 CRT。

## 版本专项

完整版：测试模型下载、离线 OCR、服务商切换和翻译；MCP 按[MCP 接入](../pinshot-mcp/README.md)验证，不能等同于通用桌面鼠标控制。

Lite：检查没有 OCR、翻译及 MCP 入口；不注册翻译快捷键，不下载模型，不从完整版通道自动更新；两个自动化启动参数须被拒绝。

## 辅助脚本

在便携包目录执行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File ./test-windows-clean-runtime.ps1 -PayloadDir .
```

该脚本使用受限 PATH 和独立配置检查设置窗口及 CRT，不能替代真实截图交互。升级回归脚本为 `tools/test-windows-upgrade.ps1`。

Windows Sandbox 可用于干净运行库测试；宿主生成脚本为 `tools/new-windows-sandbox-validation.ps1`。沙盒通常沿用较新的 Windows，不能当作 LTSC 1809 环境。需要启用功能或重启时必须由用户明确授权。
