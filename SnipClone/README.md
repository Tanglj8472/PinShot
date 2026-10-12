# SnipClone

原生 Win32 + Direct2D 的 Snipaste 风格截图工具。

- **底层**（捕获 / DPI / Layered / 矢量绘制 / 编码）：参考 PowerToys ZoomIt（MIT，见 `THIRD_PARTY_NOTICES.md`）
- **批注组装**（Annotator 所有权、工具栏对话、copy/pin/save）：对齐本仓库 PinShot（自研 Snipaste 式 UX）

装配对照见 [`docs/ASSEMBLY.md`](docs/ASSEMBLY.md)。分阶段计划见 [`docs/ROADMAP.md`](docs/ROADMAP.md)；桌面验收见 [`docs/DESKTOP_CHECKLIST.md`](docs/DESKTOP_CHECKLIST.md)；发布见 [`docs/RELEASE.md`](docs/RELEASE.md)。

**版本 0.2.0** · Windows 10 x64 · 解压运行 `SnipClone.exe`

## 当前进度

| 能力 | 状态 |
|------|------|
| 托盘常驻 / 单实例 / 退出 | ✅ |
| 全局热键（可改 `settings.ini`）+ 冲突提示 | ✅ |
| 冻结虚拟桌面 → 框选松手即批注 | ✅ |
| 选区放大镜 + HEX | ✅ |
| 批注宿主显示冻结底图（WIC Compose → ULW） | ✅ |
| 坐标映射 `CoordMap`（含负虚拟原点） | ✅ 代码；多屏 待验证 |
| 导出不含选中框 / 失败可重试 / 输出目录记忆 | ✅ |
| Annotator：选中/笔/矩形/椭圆/箭头/文字/马赛克/橡皮 + undo/redo | ✅ |
| 工具栏图标 + 样式子面板 | ✅ |
| 复制 / PNG / 贴图 | ✅ |
| LogicTests + `--smoke` CI | ✅ |
| 批注后调选区 / 命中缩放 / 文字 IME | ✅ |
| 贴图缩放/透明度/隐藏/穿透 | ✅ |
| 贴图再批注 / 窗口识别 / GraphicsCapture | ❌ 见评估文档 |

## 使用

1. VS2022 打开 `SnipClone.sln`，`Release|x64` 生成（或 `.\build.ps1`，含测试与 `--smoke`）
2. 运行 EXE → **托盘图标**；`Ctrl+Shift+S` 截图（冲突时看提示，可改 `%APPDATA%\SnipClone\settings.ini`）
3. 拖拽框选 → **松手即进入批注**（`Esc` 取消）
4. 工具栏：主栏图标 + 样式子面板
5. **文字**：落点输入（中文 IME）→ Enter 提交 / Esc 取消；双击重编
6. **选择工具**：点选后拖动/拖端点缩放；`Del` 删除
7. **贴图**：拖拽、滚轮缩放、右键菜单；穿透用橙框，`Ctrl+Shift+P` 退出
8. 托盘右键可退出；重复启动会提示已在运行

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
