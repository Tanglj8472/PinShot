# 第三方组件

Cargo.lock 固定依赖版本，许可证以上游源码包为准。

- UI/图形：winit、tiny-skia、softbuffer、egui/eframe、fontdue、ttf-parser 等。
- OCR：paddle-ocr-rs、ort、ONNX Runtime；Windows OCR 使用 WebGPU/Dawn 运行库。
- 模型来自 PaddleOCR/RapidOCR，按需下载，不包含在安装包中。
- 字体来自系统，安装包不分发用户字体。

打包脚本收集 Cargo 源码缓存及 ONNX Runtime 构建输出中的许可证/NOTICE 文件至 `licenses/`，分发时请保留。
