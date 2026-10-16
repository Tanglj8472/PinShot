# 文字标注目标（对齐 参考工具 / PinShot）

> 说明：演示视频 `文字标注展示.mp4` 位于本机路径，云环境无法直接读取。
> 本规格以 PinShot 中已按 参考工具 公开行为实现的文字交互为准，并作为 SnipClone 目标。

## 交互

| 步骤 | 行为 |
|------|------|
| 选 Text 工具 | 工具栏 `text` 或快捷键 `X` |
| 单击空白 | 在落点打开**浮动输入框**（Win32 EDIT，支持中文 IME） |
| 打字 | 输入框随内容变宽；可改 B / I / OL / 字号 / 颜色 |
| Enter | 非空则提交为文字标注 |
| Esc | 取消本次输入 |
| 再点空白 | 先提交当前输入框（PinShot 同款），需再点一次才新建 |
| 单击已有文字 | 重开输入框编辑 |
| 双击已有文字 | 任意工具下均可重编 |
| 1 / 2 或 w- / w+ | 字号 ±2（8–120） |

## 视觉

- 输入框：白底 + 当前颜色虚线边框（近似 PinShot `QLineEdit`）
- 落盘渲染：可选 outline 光晕（浅色字黑晕 / 深色字白晕），支持粗体斜体

## 代码

- `AnnotationOverlay/TextEditor.*` — 浮动编辑器
- `DrawStack::AddText / UpdateText / HitTest`
- `AnnotationRenderer::DrawTextCmd`
