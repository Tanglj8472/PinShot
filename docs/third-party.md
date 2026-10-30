# 第三方组件与分发

依赖版本以锁文件为准，许可证以上游源码和随包声明为准。整理用户文档时，不删除分发所需的许可证或 NOTICE。

## 组件

| 类别 | 主要组件 | 范围 |
| --- | --- | --- |
| 窗口与界面 | winit、softbuffer、egui／eframe | 两版 |
| 图像与渲染 | tiny-skia、fontdue、image 等 | 两版 |
| OCR 推理 | paddle-ocr-rs、ort、ONNX Runtime | 完整版 OCR 构建 |
| GPU OCR | CUDA、DirectML 或 WebGPU 相关组件 | 仅对应 feature 构建 |
| MCP 协议 | Model Context Protocol SDK、Zod | 独立 Node 服务 |

默认 OCR 构建不等于启用所有 GPU 后端。Lite 不分发推理运行库或 MCP 服务。包内实际文件及依赖审计报告是判断运行库组成的依据。

## 模型与字体

OCR 模型来自 PaddleOCR／RapidOCR，按需下载，不包含在安装包中。字体使用系统字体，发布包不分发用户安装的字体。

## 许可证

Windows 打包脚本收集可用的依赖许可证和 NOTICE 到 `licenses/`。发布与再分发时，应保留对应声明，并核对新增依赖是否带来额外义务。

历史构建可能包含 Dawn 等 GPU 组件；保留仓库中的相关许可文件，不把它们当作可以删除的过时说明。
