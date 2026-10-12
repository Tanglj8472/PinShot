# SnipClone（V0）

原生 Win32 + Direct2D 的 Snipaste 风格截图原型。参考 PowerToys ZoomIt 的
Layered Overlay / DPI / 选区交互思路，按「先框选、再捕获」重写（见
`THIRD_PARTY_NOTICES.md`）。

## 当前进度：V0 最小原型

| 能力 | 状态 |
|------|------|
| 全局热键 `Ctrl+Shift+S` | ✅ |
| 虚拟桌面全屏选区 Overlay | ✅ |
| 拖拽画框 + 实时 `宽 × 高` | ✅ |
| ESC / 右键取消 | ✅ |
| 区域捕获 / 批注 / 工具栏 / 贴图 | ❌ 计划 V1–V4 |

## 环境

- Visual Studio 2022（v143），Windows 10 1903+
- 工作负载：使用 C++ 的桌面开发
- 打开 `SnipClone.sln` → 配置 `Release | x64` → 生成

或在「x64 Native Tools Command Prompt」中：

```bat
msbuild SnipClone.sln /p:Configuration=Release /p:Platform=x64
```

也可运行 `.\build.ps1`（需已安装 VS2022 + MSBuild）。

## 使用（V0）

1. 运行 `bin\x64\Release\SnipClone.exe`
2. 按 `Ctrl+Shift+S` 进入选区
3. 拖拽鼠标画矩形，左上角附近显示像素尺寸
4. 松开左键确认（矩形写入 `SelectionOverlay::LastSelection`，Debug 输出坐标）
5. `Esc` 或右键取消

V0 **不抓屏、不批注**；确认后仅回到 Idle，为 V1 `ScreenCapture` 留好选区数据。

## 目录

```
SnipClone/
├─ Common/              DpiHelper · WinHelper
├─ SelectionOverlay/    框选 Overlay（V0 核心）
├─ Hotkey/              RegisterHotKey
├─ Capture/             （V1 占位）
├─ AnnotationOverlay/   （V2 占位）
├─ FloatingToolbar/     （V3 占位）
├─ PinWindow/           （V4 占位）
├─ main.cpp             状态机入口
├─ app.manifest         Per-Monitor V2 DPI
└─ SnipClone.sln
```

## 状态机（已实现）

```
Idle --Ctrl+Shift+S--> SelectionOverlay --confirm/ESC--> Idle
```

## 相对方案骨架的修正

| 骨架问题 | V0 处理 |
|----------|---------|
| 选区阶段使用 `WS_EX_TRANSPARENT` | 去掉；选区必须收鼠标 |
| `LWA_COLORKEY` + HwndRT | 改为 `UpdateLayeredWindow` + D2D DC RT，真·挖空 |
| `SM_CXSCREEN` 仅主屏 | 使用虚拟桌面 `SM_*VIRTUALSCREEN` |
| 热键 `Ctrl+S` | 改为 `Ctrl+Shift+S`，少与其它软件冲突 |

## 路线图

- **V1** GraphicsCapture / GDI 区域捕获 → 剪贴板
- **V2** D2D 批注（笔/矩形/撤销栈）
- **V3** 悬浮工具栏 + 文字 + 马赛克
- **V4** PinWindow 贴图
- **V5** 局部刷新与资源生命周期打磨
