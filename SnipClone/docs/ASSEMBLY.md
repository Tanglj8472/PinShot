# SnipClone 批注装配说明

## 分工

| 层 | 参考 | SnipClone 落点 |
|----|------|----------------|
| 屏幕捕获 / DPI / Layered / 矢量绘制底层 | PowerToys ZoomIt（MIT） | `Capture/` · `Common/DpiHelper` · D2D `AnnotationRenderer` · `ImageUtil` |
| 批注功能组装（谁拥有什么、工具栏怎么对话、导出） | PinShot（自研 Snipaste 式） | `AnnotationOverlay` + `DrawStack` + `Toolbar` + `PinManager` |

## PinShot 对位

| PinShot | SnipClone |
|---------|-----------|
| `App.snip` → capture → `SnipOverlay` | `main` → `SelectionOverlay` → `ScreenCapture` → `AnnotationOverlay` |
| `Annotator`（shapes/undo/tool） | `DrawStack` |
| `ToolDock.action` / `set_tool` | `Toolbar` → `OnToolbarAction` / `DrawStack::SetTool` |
| `ann.composed()` + crop | `AnnotationRenderer::Compose`（区域捕获后已是选区大小，无需再裁） |
| `copied` / `pinned` / `saved` | `DoCopy` / `DoPin` / `DoSave` |
| `PinWindow` + `ALL_PINS` | `PinWindow` + `PinManager` |

## 与 ZoomIt 的流程差

ZoomIt：全屏抓图 → 全屏批注。  
SnipClone / PinShot 式：先框选 →（本工程）只捕获选区 → 选区尺寸 Overlay + 底部工具栏批注 → 复制/保存/贴图。

## 数据流

```
Ctrl+Shift+S
  → SelectionOverlay（只画选框）
  → ScreenCapture::CaptureRegion(screenRect)   // GDI BitBlt，ZoomIt 风格
  → AnnotationOverlay 拥有 base 位图
       ├─ DrawStack（笔/矩形/箭头 + undo）
       ├─ Toolbar（工具切换 + undo/redo/close/pin/save/copy）
       └─ Compose → ImageUtil 剪贴板/PNG 或 PinManager::CreatePin
```
