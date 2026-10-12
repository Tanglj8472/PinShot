# SnipClone

原生 Win32 + Direct2D 的 Snipaste 风格截图工具。

- **底层**（捕获 / DPI / Layered / 矢量绘制 / 编码）：参考 PowerToys ZoomIt（MIT，见 `THIRD_PARTY_NOTICES.md`）
- **批注组装**（Annotator 所有权、工具栏对话、copy/pin/save）：对齐本仓库 PinShot（自研 Snipaste 式 UX）

装配对照见 [`docs/ASSEMBLY.md`](docs/ASSEMBLY.md)。

## 当前进度

| 能力 | 状态 |
|------|------|
| 全局热键 `Ctrl+Shift+S` | ✅ |
| 冻结虚拟桌面 → 框选松手即批注 | ✅ |
| 选区放大镜 + HEX | ✅ |
| 批注宿主显示冻结底图（WIC Compose → ULW） | ✅ |
| Annotator：选中/笔/矩形/椭圆/箭头/文字/马赛克/橡皮 + undo/redo | ✅ |
| 工具栏图标 + 样式子面板（色板/线宽/填充/文字/自定义色） | ✅ |
| 复制剪贴板 / 保存 PNG / 贴图 Pin | ✅ |
| GraphicsCapture / 窗口探测 | ❌ 后续 |

## 使用

1. VS2022 打开 `SnipClone.sln`，`Release|x64` 生成（或 `.\build.ps1`）
2. 运行 EXE，按 `Ctrl+Shift+S`
3. 拖拽框选 → **松手即进入批注**（`Esc` 取消）
4. 工具栏：主栏图标（选/笔/形/…）+ 下方样式子面板（线宽/填充/色板/B·I·OL；可点当前色自定义）
5. **文字**：点击落点 → 浮动输入框（中文 IME）→ Enter 提交 / Esc 取消；双击重编
6. **选择工具**：点选图形后拖动；`Del` 删除
7. **橡皮**：沿路径恢复冻结底图像素
8. 贴图窗口可拖拽；双击关闭

文字交互细节见 [`docs/TEXT_ANNOTATION.md`](docs/TEXT_ANNOTATION.md)。

## 状态机

```
Idle → CaptureVirtualScreen(freeze)
     → SelectionOverlay（调整选区）
     → Crop → AnnotationOverlay(DrawStack + Toolbar)
     → Copy / Save / Pin / Esc → Idle
```

## 快捷键（批注）

| 键 | 作用 |
|----|------|
| S / P / R / E / A / X / M / K | 选择 / 笔 / 矩形 / 椭圆 / 箭头 / 文字 / 马赛克 / 橡皮 |
| F | 矩形/椭圆填充开关 |
| 1 / 2 | 线宽、字号或马赛克块减小/增大 |
| Del | 删除选中图形 |
| Ctrl+Z / Y | 撤销 / 重做 |
| Ctrl+C / S / T | 复制 / 保存 / 贴图 |

## 目录

```
SnipClone/
├─ Common/              DpiHelper · WinHelper · ImageUtil
├─ Capture/             ScreenCapture（GDI 冻结捕获）
├─ SelectionOverlay/    框选 + 手柄 + 放大镜
├─ AnnotationOverlay/   宿主 + DrawStack + D2D Renderer
├─ FloatingToolbar/     工具栏（对齐 PinShot 装配）
├─ PinWindow/           贴图
├─ Hotkey/
└─ docs/ASSEMBLY.md
```
