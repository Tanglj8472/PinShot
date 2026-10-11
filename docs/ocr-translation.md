# OCR 与划词翻译

## 使用

- 原截图快捷键保持不变，选区工具栏增加 **OCR**（仅本地识别）与 **译**（识别后在线翻译）。
- **F6** 打开截图翻译，框选松开后识别并翻译。
- **F5** 翻译当前前台软件中的选中文字，不需要常驻鼠标钩子。
- 设置的 Hotkeys 页可以修改 F5/F6。
- 结果窗提供可编辑原文、目标语言、译文及复制按钮。翻译失败时保留原文，可再次点击翻译。
- OCR 结果窗显示选区原图预览，方便核对。

## 运行

安装 requirements.txt 中的 RapidOCR 和 ONNX Runtime，执行 `python tools/prepare_ocr_models.py` 下载并校验中英 PP-OCRv5 mobile 三个模型，再执行 PyInstaller 打包。模型不进入 Git，打包程序携带模型，可离线识别。源码运行未准备模型时，由 RapidOCR 首次下载到 Qt AppLocalDataLocation 下的 ocr-models。首次使用懒加载引擎；识别裁剪冻结原图的物理像素，不将标注或选区控制点送入引擎。

翻译沿用 SuperTranslate 的 Edge 在线接口，仅在用户请求翻译时发送原文，网络请求超时为 8 秒。Edge 网页接口可能变动，本版暂未接入其他翻译源和密钥设置。

OCR 与网络请求在后台线程运行，界面不等待计算。关闭结果窗后忽略迟到结果。

划词通过模拟 Ctrl+C 获取文字并恢复剪贴板；不可复制的控件或更高权限的窗口可能无法取词。本版不自动提权。

## 参考与许可

交互和服务流程参考 [SuperTranslate](https://github.com/Tanglj8472/SuperTranslate)，MIT，Copyright (c) 2026 唐凌君；Python/Qt 适配独立实现。OCR 依赖 [RapidOCR](https://github.com/RapidAI/RapidOCR) 和 [PaddleOCR](https://github.com/PaddlePaddle/PaddleOCR)，Apache-2.0。

## 本版验证

- 标注交互 10 项、服务与剪贴板回归 8 项、真实 OCR 模型测试 2 项通过。平台回归 9 项通过、2 项跳过。
- 实际鼠标选中文字，F5 显示原文并翻译成中文；点击复制译文并粘贴到测试文本框，结果正确。
- F6 实际框选中英两行，松开后自动识别并打开结果窗，原文完整。
- F6 单独框选英文行，正确识别 Hello world 并自动翻译为“你好，世界”。
- Windows 打包程序实际框选 Hello，随包模型识别成功，在线翻译为“你好”。
- 283×93 小区域原先漏字，独立模型测试复现；限制检测缩放为最长边 960、不放大小区域后，大小两种区域测试均通过。
- 选区在鼠标松开时使用释放坐标收尾，避免漏掉最后一次移动事件时裁剪边界不完整。

原版 Snipaste 同时运行时可能占用 F1/F3。控制窗会显示未注册的热键，设置中可调整。
