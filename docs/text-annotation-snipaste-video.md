# SnipClone 文字标注演示视频 — 解析与 PinShot 对齐清单

**源文件：** `docs/media/文字标注展示.mp4`（约 31s，1278×798 选区，SnipClone / Snipaste 复刻 overlay）

云端或其它环境可直接打开上述 mp4 对照；本文档为逐帧抽帧（0.5 fps）后的行为摘要，便于实现与验收。

---

## 1. 视频里演示的主流程

| 阶段 | 画面表现 | 预期交互 |
|------|----------|----------|
| 选区 | 蓝色 8 向手柄框 + 左上角 `1278 × 798 px` | 与现有 snip 选区一致 |
| 选文字工具 | 主工具栏 **T** 高亮；下方浮出**文字样式子面板** | 仅 `text` 工具时显示子面板 |
| 首次输入 | 虚线/浅底输入框，光标闪烁，键入「你好」 | 单击画布进入输入，非多行富文本编辑器外观 |
| 字号变化 | 子面板字号从约 **10.5 → 42.7 → 26.8 → 17.8 → 13.8** 等 | **拖文字框角点缩放时，字号随框同步变化**（非仅改 spinbox） |
| 样式 | **B / I / A**（描边或阴影类）、字体 **Microsoft YaHei UI**、色板选红 | 与 Snipaste 文字条一致 |
| 选中态 | 文字外接矩形 + 四角/四边 resize 手柄；可再次编辑 | 双击或选中后进入 `begin_edit_text` |
| 完成态 | 大号红色「你好」，带白色描边/光晕（A 开启时） | `outline` 渲染与面板联动 |
| 快捷键 HUD | 左下：WASD 移光标 1px、方向键移选区、1/2 或滚轮调线宽、右键/Esc 取消、`,` `.` 显隐指针 | overlay 帮助层；文字模式下线宽项对文字可能映射字号 |

---

## 2. 文字子面板（视频中的 UI 规格）

子面板位于主工具栏**正下方**独立浮层（切换工具时不挤动主栏）：

1. **B** — 粗体  
2. **I** — 斜体  
3. **A** — 文字描边/阴影（视频中最终「你好」为红字白边）  
4. **字体下拉** — 默认 Microsoft YaHei UI  
5. **字号** — 小数步进显示（如 17.8、42.66），随缩放更新  
6. **颜色** — 左侧大色块 + 右侧 20 色预设格  

主工具栏顺序（视频）：形状/笔/箭头/… → **T** → 马赛克 → 撤销重做 → 关闭/贴图/保存/复制。

---

## 3. PinShot（本仓库 Python）现状对照

| 能力 | 视频 / Snipaste 目标 | PinShot 现状 | 对齐建议 |
|------|----------------------|--------------|----------|
| 文字子面板 | B/I/A + 字体 + 字号 + 色板 | `toolbar.py` `tool == "text"` 已实现同类控件 | 核对布局与 Snipaste 间距、图标 |
| 单击创建文字 | 虚线输入框 | `Annotator.start_text` + `QLineEdit` | 样式接近；确认半透明底与虚线边框 |
| 提交 / 取消 | Enter 提交、Esc 取消 | `commit_text` / Escape | 已具备 |
| 双击再编辑 | 选中后编辑 | `overlay.py` / `pin_window.py` `mouseDoubleClickEvent` → `begin_edit_text` | 已具备 |
| **框缩放改字号** | 拖手柄时 spin 字号跟着变 | `_resize_selected` 对 `text` 只改 `pts[0]` 为 topLeft，**未按框高比例改 `font_size`** | **优先补**：text resize 时按高度或宽度比例更新 `style.font_size` 并重绘 |
| 多行文字 | Snipaste 支持换行 | 渲染支持 `\n`，编辑器为单行 `QLineEdit` | 若要对齐 Snipaste，考虑 `QTextEdit` 或 Shift+Enter |
| 选中描边 | 白色 outline | `Style.outline` + `render` | 对照视频最终效果调 alpha/宽度 |
| 滚轮调字号 | 文字选中时滚轮可能改字号 | `annotate.py` 滚轮对 text 有 `font_size` 分支 | 与 SnipClone 实测是否一致 |

关键代码入口：

- 输入与提交：`pinshot/annotate.py` — `start_text` / `commit_text` / `_resize_selected`（`kind == "text"`）  
- 子面板：`pinshot/toolbar.py` — `elif tool == "text"`  
- 双击编辑：`pinshot/overlay.py`、`pinshot/pin_window.py`

---

## 4. 建议验收用例（对齐视频）

1. 选 T → 子面板出现 B/I/A、字体、字号、色板。  
2. 单击输入「你好」→ Enter → 红字显示；开 A 时有白描边。  
3. 选中文字，拖 corner handle 放大 → **字号数值与视觉同步变大**。  
4. 双击文字 → 回到输入态，内容保留。  
5. Esc 取消输入不产生空 shape；Delete 删除已提交文字。  

---

## 5. 维护说明

- 本地抽帧目录 `docs/media/frames/` 与 `tools/ffmpeg/` 已 gitignore，勿提交。  
- 更新演示请替换 `docs/media/文字标注展示.mp4` 并修订本节表格。
