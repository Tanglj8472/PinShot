# ZoomIt 复用边界与对照台账

上游：[PowerToys ZoomIt](https://github.com/microsoft/PowerToys)（MIT）。本仓库已在 `THIRD_PARTY_NOTICES.md` 声明。

## 允许参考（适配进 SnipClone）

| 主题 | ZoomIt 侧概念 | SnipClone 落点 | 状态 |
|------|---------------|----------------|------|
| 虚拟桌面 GDI 截屏 | `BitBlt` + `CAPTUREBLT`、强制不透明 alpha | `ScreenCapture.cpp` | ✅ 已适配 |
| DPI / 虚拟屏矩形 | `GetSystemMetrics(SM_X/Y/CX/CYVIRTUALSCREEN)` | `DpiHelper.cpp` | ✅ |
| Layered 更新 | `UpdateLayeredWindow` + 预乘 alpha | Selection / Annotation / Pin / Toolbar | ✅ |
| 笔迹采样密度 | 距离阈值加点 | `DrawStack::Move`（笔/橡皮） | ✅ 基础 |
| 抗锯齿形状 | GDI+/自绘 → **映射为 D2D** | `AnnotationRenderer` | ✅ D2D 路径 |
| 鼠标捕获 / Esc 取消 / 释放 | overlay 生命周期 | Selection / Annotation `Stop` | ✅ |
| 位图快照撤销 | ZoomIt 全图快照 | **对象文档快照** `DrawStack` undo | ✅ 已对象化（非位图栈） |

## 明确不照搬

| 主题 | 原因 |
|------|------|
| ZoomIt 自有 UI / 热键模型 | 体验对齐 Snipaste |
| 以位图为唯一文档 | 需要可编辑对象（选中/移动/缩放/重编文字） |
| 直接替换为 ZoomIt 绘制后端 | 第一阶段固定 D2D；避免双后端 |

## 对照记录模板（新增适配时填写）

```
日期:
上游版本/提交:
函数或文件:
修改摘要:
验证:
```

新增 ZoomIt 对照实现时，同步更新本表与 `THIRD_PARTY_NOTICES.md`。
