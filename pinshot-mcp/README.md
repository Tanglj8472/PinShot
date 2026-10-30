# PinShot MCP

通过 MCP 调用完整版 PinShot 的原生截图、标注和会话功能。Node.js 服务处理协议，Rust 应用执行捕获与渲染。Lite 不包含 MCP。

## 工作方式

```text
MCP 客户端
    │ stdio
Node.js MCP 服务
    │ Windows 本机命名管道
Rust PinShot：捕获、标注、预览与保存
```

接口仅在完整版使用 `--automation` 启动后开放，不需要公开网络端口。MCP 自动化不等于通用桌面控制：本组工具不提供任意鼠标点击、键盘输入、设置操作、OCR 或翻译调用。

## 准备与连接

需要 Windows 交互桌面、Node.js 20+ 和本项目完整版。先退出旧实例，再在应用目录执行：

```powershell
./pinshot.exe --automation
```

使用预构建 MCP 服务包时，解压后保留 `dist` 和运行依赖。客户端启动命令为 `node`，参数为 `dist/index.js` 的绝对路径。

从源码运行时，在 `pinshot-mcp` 目录执行：

```powershell
npm ci
npm run build
npm run smoke
```

将客户端配置指向构建后的入口，例如 `C:/Apps/PinShotMCP/dist/index.js`。客户端配置格式各不相同，应填写对应的 command 与 args 字段。重连后先调用 `ping`，再测试截图；仅更新文件而不重载工具列表，可能仍看到旧工具。

不要通过 `npm start` 作为已部署客户端入口。stdio 的 stdout 只传协议消息，日志应写 stderr。

## 工具

| 工具 | 用途 |
| --- | --- |
| `ping` | 检查连接 |
| `get_state` | 查询进程、会话、图片尺寸、桌面原点和标注 ID |
| `list_windows` | 枚举窗口标题及物理像素位置 |
| `capture_screen` | 捕获全部显示器 |
| `capture_window` | 按标题关键字捕获窗口 |
| `add_annotation` | 添加矩形、椭圆、直线、箭头、文字、模糊或马赛克 |
| `remove_annotation` | 按标注 ID 删除 |
| `preview_session` | 返回带标注的 PNG 预览 |
| `save_session` | 保存完整尺寸 PNG，拒绝覆盖已有文件 |
| `show_session` | 显示原生会话窗口 |
| `hide_session` | 隐藏窗口，保留会话与编辑历史 |
| `edit_session` | 打开原生标注工具栏 |
| `undo_session` | 撤销一次编辑 |
| `redo_session` | 重做一次编辑 |
| `close_session` | 关闭会话并释放资源 |

## 截图与坐标

截图和预览返回真正的 PNG 图片内容，不仅是文件路径。预览长边不超过 1600 像素，响应包含 `preview.width`、`preview.height` 和 `preview.scale`。

标注始终使用原图坐标：

```text
原图坐标 = 预览坐标 ÷ preview.scale
```

图片内坐标从图片左上角开始。多显示器的桌面 `origin` 可以为负数，不要把桌面绝对坐标直接当作标注坐标。

捕获指定窗口前先调用 `list_windows`。窗口标题按不区分大小写的关键字匹配，取层叠靠前的匹配窗口；默认尝试激活窗口，结果中的 `window.activated` 表示是否成功。窗口被遮挡时，捕获结果可能包含上层内容，需读取图片确认。

## 标注与会话

- 最多同时保留 4 个会话，每个最多 1000 个标注。
- 文字使用一个点，其他标注使用两个点；`width` 表示线宽或字号，`strength` 表示效果程度。
- `show: false` 只建立后台会话，不弹出预览窗口。
- 原生编辑与 AI 编辑使用稳定标注 ID 和共享撤销历史。
- 未完成的手工操作通过 busy 状态保护；并发冲突应重新读取状态，不盲目重试覆盖。
- 后台添加的已有标注是首次打开编辑器时的初始内容，不一定有可撤销历史。
- 保存路径必须为绝对路径，父目录须存在；已有文件不会被覆盖。
- 只关闭自己建立的测试会话，不清理用户已有会话。

## 验证与限制

`npm run smoke` 检查 15 个工具发现与 ping，不证明真实截图可用。`npm run test:workflow` 会调用运行中的完整版，测试捕获、标注、预览、导出、窗口和撤销重做。

已有本机证据与未覆盖项目见[验证记录](../docs/validation.md)。手工绘制与 AI 双向同步仍需专门交互验收，不能仅凭工具返回成功认定完成。

若需要操控其他应用的鼠标键盘，应另行配置并授权桌面控制工具。第三方桌面控制服务不包含在 PinShot 发布包中。
