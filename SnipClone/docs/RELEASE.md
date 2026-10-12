# SnipClone 发布说明

## 版本

- 当前：**0.2.0**（`app.manifest` / 关于对话框）
- 产物：`bin\x64\Release\SnipClone.exe`（单文件，无安装器）

## 最低系统

| 项 | 要求 |
|----|------|
| OS | Windows 10 1809+（x64），PerMonitorV2 DPI |
| 运行库 | **当前 Release 使用动态 CRT**：目标机需安装对应 [VC++ Redistributable](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist)。静态 CRT / 随包部署待批次三收口。 |
| 系统组件 | Direct2D、DirectWrite、WIC（Win10 自带） |

## 解压即用（在满足运行库前提时）

1. 复制 `SnipClone.exe` 到任意目录  
2. 双击运行 → 托盘图标出现  
3. `Ctrl+Shift+S` 截图（可在 `%APPDATA%\SnipClone\settings.ini` 修改）  
4. 托盘右键 → 退出  

## 单实例

第二次启动会激活已有实例（托盘气泡提示），不会开第二个进程。

## 设置（`%APPDATA%\SnipClone\settings.ini`）

当前无独立设置页；通过托盘「打开设置目录」编辑 ini。`StartWithWindows` 字段未实现，请勿依赖。

```ini
[Hotkey]
SnipVk=83
SnipMods=6
PinsVk=80
PinsMods=6

[General]
OutputDir=C:\Users\...\Documents\SnipClone
```

- `SnipMods` / `PinsMods`：`MOD_ALT=1`、`MOD_CONTROL=2`、`MOD_SHIFT=4`、`MOD_WIN=8`（可组合相加）  
- 改热键后需重启 SnipClone  

## CI 检查

- MSBuild Debug/Release x64  
- `SnipClone.Tests.exe`（裁切 / 撤销 / 合成）  
- `SnipClone.exe --smoke`（初始化托盘/热键/单实例后立即退出）  

## 已知未包含

- 安装器 / 自动更新  
- 开机启动（`StartWithWindows` 字段预留，尚未写注册表）  
- 窗口/控件识别、GraphicsCapture（见 `GRAPHICS_CAPTURE.md`）  
