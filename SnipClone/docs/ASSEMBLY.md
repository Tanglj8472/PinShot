# SnipClone 批注装配说明

## 分工

| 层 | 参考 | SnipClone 落点 |
|----|------|----------------|
| 屏幕捕获 / DPI / Layered / 矢量绘制底层 | PowerToys ZoomIt（MIT，模式级参考，未整棵 vendoring） | `Capture/` · `Common/DpiHelper` · D2D `AnnotationRenderer` · `ImageUtil` |
| 批注功能组装（谁拥有什么、工具栏怎么对话、导出） | PinShot（自研 Snipaste 式） | `AnnotationOverlay` + `DrawStack` + `Toolbar` + `PinManager` |

## PinShot 对位

| PinShot | SnipClone |
|---------|-----------|
| `App.snip` → `capture_virtual()` → `SnipOverlay(shot)` | `main` → `CaptureVirtualScreen` → `SelectionOverlay(freeze)` → crop → `AnnotationOverlay` |
| `Annotator`（shapes/undo/tool） | `DrawStack` |
| `ToolDock.action` / `set_tool` | `Toolbar` → `OnToolbarAction` / `DrawStack::SetTool` |
| `ann.composed()` + crop | `LastCrop()` 已是选区；`AnnotationRenderer::Compose` 叠批注 |
| `copied` / `pinned` / `saved` | `DoCopy` / `DoPin` / `DoSave` |
| `PinWindow` + `ALL_PINS` | `PinWindow` + `PinManager` |

## 与 ZoomIt 的关系（诚实说明）

- **有参考**：ZoomIt `Utility`/`Zoomit.cpp` 的 GDI `BitBlt(... SRCCOPY\|CAPTUREBLT)`、32bpp DIB、DPI 辅助、以及 `SelectRectangle` 的拖拽/ESC 交互思路（MIT，见 `THIRD_PARTY_NOTICES.md`）。
- **没有**：把 ZoomIt 源码整目录拷进仓库；也没有复用 ZoomIt 的全屏画布批注状态机。
- **流程差**：ZoomIt Draw 多为全屏缓冲上批注；SnipClone / PinShot 式是 **先冻结虚拟桌面 → 在冻结图上框选 → 裁切 → 选区 Overlay + 底部工具栏**。

## 数据流

```
Ctrl+Shift+S
  → ScreenCapture::CaptureVirtualScreen()     // 先冻结（PinShot）
  → SelectionOverlay 冻结图 + 区外变暗 + 放大镜
       松手 → crop 进入批注；Esc → 取消
  → freeze.CropToScreenRect(selection)
  → AnnotationOverlay 拥有 crop 位图
       ├─ DrawStack（选中/笔/形/箭头/文字/马赛克/橡皮 + undo）
       ├─ Toolbar（工具 + 线宽/填充/色板 + undo + close/pin/save/copy）
       └─ Compose → ImageUtil 剪贴板/PNG 或 PinManager::CreatePin
```

## 批注快捷键（对齐 PinShot 习惯）

| 键 | 作用 |
|----|------|
| S / P / R / E / A / X / M / K | 选择 / 笔 / 矩形 / 椭圆 / 箭头 / 文字 / 马赛克 / 橡皮 |
| F | 填充开关 |
| 1 / 2 | 线宽、字号或马赛克块减小/增大 |
| Del | 删除选中图形 |
| 文字工具单击落点 | 浮动 EDIT 输入（中文 IME）；Enter 提交 / Esc 取消；双击重编 |
| Ctrl+Z / Y | 撤销 / 重做 |
| Ctrl+C / S / T | 复制 / 保存 / 贴图 |
