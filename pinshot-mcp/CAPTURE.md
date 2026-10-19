# PinShot 原生截图工具

`capture_screen` 无需参数，通过 PinShot 的 Rust 原生程序捕获全部显示器，返回 PNG 图片，不打开框选界面。

调用链：MCP → pinshot --capture-to <临时文件.png> → Capture::grab(false) → DXGI / GDI 回退（Windows）或原有 X11 捕获（Linux）→ 现有图片编码器 → MCP 图片结果。

先在 rust 目录执行 `cargo build --release --bin pinshot`。必须使用本分支新增 --capture-to 的版本，旧发行版不支持。

服务默认寻找仓库 rust/target/release/pinshot.exe，再寻找 debug 构建。可通过 MCP 进程的 PINSHOT_EXE 环境变量指定程序绝对路径（Linux 文件名为 pinshot）。不会自动寻找已安装的旧版本。

在 pinshot-mcp 目录运行 `npm run build`，重新加载 Codex MCP 服务后，调用 capture_screen。

验收：`npm run smoke` 检查工具发现与 ping；`npm run smoke -- --capture` 会真实截屏并校验返回 PNG。超时 30 秒，PNG 大小上限 24 MB，独立临时目录在调用结束后清理。
