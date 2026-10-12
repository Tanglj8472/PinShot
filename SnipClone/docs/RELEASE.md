# SnipClone 发布说明

## 版本

- 当前：**0.2.0**（`app.manifest` / 关于对话框）
- 产物：`bin\x64\Release\SnipClone.exe`（单文件，无安装器）

## 最低系统

| 项 | 要求 |
|----|------|
| OS | Windows 10 1809+（x64），PerMonitorV2 DPI |
| 运行库 | **Release 使用静态 CRT（`/MT`）**：无需另装 VC++ Redistributable |
| 系统组件 | Direct2D、DirectWrite、WIC（Win10 自带） |

## 解压即用

1. 复制 `SnipClone.exe` 到任意目录  
2. 双击运行 → 托盘图标出现  
3. `Ctrl+Shift+S` 截图（可在 `%APPDATA%\SnipClone\settings.ini` 修改）  
4. 托盘右键 → 退出  

## 单实例

第二次启动会激活已有实例（托盘气泡提示），不会开第二个进程。`--smoke` 在已有实例时退出码 **2**。

## 设置（`%APPDATA%\SnipClone\settings.ini`）

托盘「设置…」可改热键、输出目录、开机启动、标注完成后是否切到选择工具；「应用 / 确定」后立即生效（热键重新注册，无需重启）。高级用户仍可直接编辑 ini。

```ini
[Hotkey]
SnipVk=83
SnipMods=6
PinsVk=80
PinsMods=6

[General]
OutputDir=C:\Users\...\Documents\SnipClone
StartWithWindows=0
ShowWelcomeTip=1

[Annotate]
SelectAfterDraw=1
DefaultTool=8
CloseAfterExport=1
```

- `SnipMods` / `PinsMods`：`MOD_ALT=1`、`MOD_CONTROL=2`、`MOD_SHIFT=4`、`MOD_WIN=8`（可组合相加）  
- `SelectAfterDraw=1`：画完矩形/箭头等后自动切到选择工具（默认开）  
- `DefaultTool`：`ToolKind` 枚举值（`8`=选择）  
- `CloseAfterExport=1`：复制/保存成功后关闭标注窗口  
- 设置页提供「恢复默认热键」；工具栏齿轮也可打开设置

## CI 检查

- MSBuild Debug/Release x64  
- `SnipClone.Tests.exe`（文档状态 / Compose / 坐标）  
- `SnipClone.exe --smoke`（必需：主窗口+设置；可选托盘/热键）  
- 可选本地：`SnipClone.exe --smoke-strict`（另要求截图热键注册成功）  

功能状态见 `VERIFICATION.md`；桌面勾选见 `DESKTOP_CHECKLIST.md`。

## 已知未包含

- 安装器 / 自动更新  
- 窗口/控件识别、GraphicsCapture（见 `GRAPHICS_CAPTURE.md`）  
- 预览帧时间桌面基准（A11 资源已复用，p95 目标待实测）  
