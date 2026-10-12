# SnipClone

原生 Win32 + Direct2D 的 Snipaste 风格截图工具。

- **底层**（捕获 / DPI / Layered / 矢量绘制 / 编码）：参考 PowerToys ZoomIt（MIT，见 `THIRD_PARTY_NOTICES.md`）
- **批注组装**（Annotator 所有权、工具栏对话、copy/pin/save）：对齐本仓库 PinShot（自研 Snipaste 式 UX）

装配对照见 [`docs/ASSEMBLY.md`](docs/ASSEMBLY.md)。

## 当前进度（V0–V4 切片）

| 能力 | 状态 |
|------|------|
| 全局热键 `Ctrl+Shift+S` | ✅ |
| 框选 Overlay + 尺寸 | ✅ |
| 选区 GDI 捕获（ZoomIt 风格 BitBlt） | ✅ |
| Annotator：笔 / 矩形 / 箭头 + undo/redo | ✅ |
| 悬浮工具栏（PinShot ToolDock 顺序子集） | ✅ |
| 复制剪贴板 / 保存 PNG / 贴图 Pin | ✅ |
| GraphicsCapture / 文字 / 马赛克 / 完整图标 | ❌ 后续 |

## 使用

1. VS2022 打开 `SnipClone.sln`，`Release|x64` 生成（或 `.\build.ps1`）
2. 运行 EXE，按 `Ctrl+Shift+S`
3. 拖拽框选 → 松手进入批注
4. 工具栏：`pen|rect|arrow | undo redo | close pin save copy`
5. 贴图窗口可拖拽；双击关闭

## 状态机

```
Idle → SelectionOverlay → ScreenCapture(region)
     → AnnotationOverlay(DrawStack + Toolbar)
     → Copy / Save / Pin / Esc → Idle
```

## 目录

```
SnipClone/
├─ Common/              DpiHelper · WinHelper · ImageUtil
├─ Capture/             ScreenCapture（GDI 区域捕获）
├─ SelectionOverlay/    框选
├─ AnnotationOverlay/   宿主 + DrawStack + D2D Renderer
├─ FloatingToolbar/     工具栏（对齐 PinShot 装配）
├─ PinWindow/           贴图
├─ Hotkey/
└─ docs/ASSEMBLY.md
```
