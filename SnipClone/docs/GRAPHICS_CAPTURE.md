# GraphicsCapture 评估（暂不引入）

## 现状

SnipClone 使用 **GDI `BitBlt` + `CAPTUREBLT`** 冻结虚拟桌面（ZoomIt 路径），再在冻结图上框选。

## GraphicsCapture（WinRT）可能收益

- 更好处理部分 DirectFlip / 全屏游戏 / 受保护内容边界情况  
- 可按窗口/显示器捕获，利于「窗口识别」产品化  

## 成本与风险

- 依赖 WinRT / Windows 10 1903+ API 表面更大  
- 异步帧池与现有同步状态机（热键 → 冻结 → overlay）集成复杂  
- 多显示器负坐标、DPI、排除自身 overlay（`WDA_EXCLUDEFROMCAPTURE`）需重测  
- CI / 最低系统说明要同步上调  

## 结论（0.2.0）

**暂不引入。** 优先巩固 GDI 冻结路径与托盘/设置/发布；待窗口识别需求明确且 GDI 兼容性成为实测瓶颈后再开专项。
