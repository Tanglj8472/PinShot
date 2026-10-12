# PinShot Rust 重写进度

对照源：`main` 上的 Python/PySide6 PinShot。Rust 在 `rust/`，分支 `rust`。Python 源码不改。

最后更新：2026-10-12（chrome 文字抗锯齿：芯片/菜单中文不再锯齿）。

---

## 当前进度

HEAD 在本轮 chrome 文字 AA 修复。工具栏芯片「描边 / 默认 / 等宽」和 B/I 不再发虚、发硬：透明缓冲改为 straight-alpha source-over（不再把覆盖率糊到黑底再盖成不透明），去掉 `cov < 8` 切断，chrome 2× 栅格 + 2×2 盒滤。UI 标签走 Noto Sans CJK SC（TTC index 2）+ Liberation Sans，标注家族 B/I/Serif/Mono 未动。Python 不改。

| 面 | 状态 |
|---|---|
| chrome 文字（芯片 / 菜单 / toast / prefs） | **已修**：平滑 AA；CJK 优先 Noto/Source Han/Droid/文泉驿，拉丁 Liberation |
| 标注工具栏 / 子面板视觉 | 上轮：近白底、6px 圆角、阴影、分隔线、选中 `#3D8FD1` 描边；中文芯片 12px |
| 图钉右键菜单 / 新分组提示 | 本轮：浅色主题、inset 悬停、勾选刻度、输入框圆角蓝边 |
| 确认放弃 / 取色 / toast / 尺寸条 / fallback·prefs 按钮 | 本轮：同一套圆角 chrome（prefs 有余力带过） |
| Linux 框选 / 矩形·椭圆·箭头 / 点边 8 手柄 / 滚轮改宽与面板数字 | 实机过（`7ef18fe`） |
| 文字快打 `hello world` + Ctrl+Enter、Ctrl+C PNG、prefs、`,` `.` 历史 | 实机过 |
| 重启后 R 套在**当前截图**的上次选区、Tab 不漏到 Chrome | 实机过 |
| pin 光标锚缩放（不切工作区）、菜单高度、Esc 只关菜单、穿透勾选 | 实机过 |
| Linux 滚轮每格两次 | **已修**（Xvfb `:22` 15/15）：3 格 → 3 次 `overlay wheel`，线宽 3→6；4 格缩放 → 4 次 `pin zoom`（不再 8 步） |
| 工具栏子面板 | Fill/Outline、Solid/Dash/Dot、B/I/Outline、Default/Sans/Serif/Mono、马赛克 Brush/Rect/Blur；`option_width` 对文字=字号、马赛克=size；当前色块 → overlay/pin 内 HSV 取色（无 zenity）；刚画完的形状保持 `option_target` 并画出 8 手柄 |
| Esc 放弃确认 | overlay 内 Y/N /「不再提示」（不再 zenity 抢抓键） |
| pin 阴影 / 缩略图 | 合成器在则默认阴影 M=12；窗=图+2M、图在 (M,M)、ARGB 阴影；菜单可勾 Shadow；Shift+双击 72px 中心裁切；session 存图原点 |
| 画笔采样 | 活笔时 QueryPointer 8ms（Python 8ms timer） |
| Windows 实机 | **未关**：Linux-first 改动后尚未在真 Windows 上复测（CI 绿） |
| AT-SPI / IME / 合成器 | 限制同前，见「已知限制」 |

`PARITY.md`：done 111 / partial 1 / missing 0 / impossible 6（合计 118）。剩余 partial：RapidOCR 条带检测（非完整 DBNet）。

---

## 目标

| 项 | 说明 |
|---|---|
| 语言/运行时 | 用 Rust 重写宿主 + 可移植核心，去掉 PySide6/Qt |
| 平台顺序 | **先 Linux X11**（Xfce / xfwm4，1920×1200；无托盘则 4 按钮 fallback），再 Windows |
| 行为 | 与 Python PinShot **1:1 对齐**（快捷键、菜单、标注、会话、热键、i18n），对照表见 [`PARITY.md`](PARITY.md) |
| 体积 | 单文件 `pinshot`；OCR 走 `--features ocr` / 独立 `pinshot-ocr`，默认包不绑模型 |
| 架构 | `pinshot-core` 可移植；`pinshot` 经 `native` 分叉 `linux_x11` / `windows` |

非目标：纯 Wayland（无 XWayland）、macOS、改 Python。

---

## 阶段与提交

时间按仓库提交顺序（均为 `rust` 线，除非注明）。

### UI chrome 文字 AA（本轮）

| SHA | 内容 |
|---|---|
| 本轮 | chrome 标签不再锯齿：`blend` 正确累加 alpha；chrome 字体栈 Noto Sans CJK SC + Liberation；2× 超采样。标注 `blit_pixmap_ex` / 家族解析未改。`cargo test --workspace --lib`：pinshot-core 126 + pinshot 23。截图 `/opt/cursor/artifacts/chrome-aa/` |

### UI chrome（上轮）

| SHA | 内容 |
|---|---|
| `21b37dc` + 上轮补记 | 工具栏 CELL 24 / 高 30、圆角阴影、选中蓝边、芯片实心蓝、直线/箭头图标区分、菜单勾选+悬停、分组框蓝边。`cargo test --workspace --lib`：pinshot-core 123 + pinshot 22。Xvfb `:28` 1920×1200：F1 框选 → `tb bar 559,708 422x30` → Text/Pin → 菜单 268×816 → New group 提示。截图 `/opt/cursor/artifacts/ui-polish/` |

### P1 骨架（Windows-only workspace）

| SHA | 内容 |
|---|---|
| `6cb9db5` | 加入 Windows-only Rust workspace：`pinshot-core` + winit/tiny-skia 宿主骨架 |

### 黑屏 / DXGI 根因

| SHA | 内容 |
|---|---|
| `6664a0f` | **根因**：DXGI 桌面复制偶发全黑帧被当成有效截图。拒绝空白 DXGI 帧，GDI 路径保持不透明，overlay 不再黑屏 |
| `f5ec1a0` | overlay 脏矩形 GDI 呈现，框选时不再每帧重绘整桌面 |

### P2 核心 + Windows 宿主

| SHA | 内容 |
|---|---|
| `ee5f600` | 分组、历史、会话、翻译、OCR 路径进入 core |
| `9e188b0` | Windows 宿主：prefs / pins / history / OCR |
| `88be97a` | Windows CI 产出 `pinshot.exe` + `pinshot-ocr.exe` |
| `65b5af4` | 修 Phase 2 引入的 Windows 编译错误 |
| `852bb3a` | pin 侧丢掉 toolbar `adjust_width` 的 bool 误用 |
| `0bea2e5` | 标注滚轮 / `option_target`、系统字体文字进 core |

### Linux X11 后端（方向改为 Linux 优先）

| SHA | 内容 |
|---|---|
| `32a04c2` | Linux X11 宿主：x11rb 捕获、XGrabKey、override-redirect overlay、EWMH pin、XShape 穿透、fallback 四按钮、XDG 配置/自启 |
| `5549fbf` | Linux CI artifact、`PARITY.md`、Xfce 构建文档 |

### 焦点 / 崩溃 / 菜单 / 会话（实机 5549fbf → 52d5642）

| SHA | 内容 |
|---|---|
| `96a68a4` | `clamp_f32` 防 min>max panic；工具切换只改新图默认；`PinSnapshot.shapes`；放大镜整数格 |
| `52d5642` | overlay `XGrabKeyboard`+`XSetInputFocus`；pin 改为托管窗并在点击时聚焦；进程内完整右键菜单；Linux 主循环设置窗；`_NET_WM_WINDOW_OPACITY`；原图+shapes 会话；颜色面板线宽；选中手柄；panic 写 `pinshot.log` |
| `aa86d9d` | 中文进度报告（本文件首版） |

### 52d5642 实机 A–L（本轮）

| SHA | 内容 |
|---|---|
| `4d32471` | prefs 整行命中 + 标题/主题/fallback 即改；Tab 子窗+AT-SPI 矩形；R/`,` 历史回放；F1 首笔拖动（指针查询+GrabPointer）；箭头头；文字首击/Ctrl+Enter/IME 放抓；点边选中+滚轮改宽；剪贴板常驻+`xclip`；pin 光标锚缩放；默认 F4；恢复不弹 toast；toast 1.2s；键盘 `owner_events=false` |
| `986dc67` | 本文件与 `PARITY.md` 将 A–L 标为已修 |
| `b2e09b0` | 进度报告写入 A–L 修复 SHA |
| `b2495fb` | F1 隐藏 fallback 起就 `InputHold`；剪贴板 `xclip` 不阻塞 + 日志 |
| `d01fe6c` | **不要**在第二路 X 客户端或 winit 的 Xlib Display 上 `XGrabPointer`（事件进错连接）。overlay 期间 `ControlFlow::Poll` + 根窗口 `QueryPointer` 认领 Button1 |
| `0098973` | 记录 d01fe6c 的 Xvfb A–L 15/15 |

### b791ea3 → `7ef18fe`（本轮）

| SHA | 内容 |
|---|---|
| `7f9b7dc` | recover **禁止** `annot.press`（修矩形/椭圆/箭头画不出）；R 只套 rect；pin `geom_pos`+`set_outer_position`；根窗口抓 Tab；菜单高度按行求和 |
| `876fc4f` | fallback 不再 Esc=Quit（Python 也没有；焦点落到四按钮会杀掉全部 pin） |
| `a4bf1b3` | 记录 Xvfb `:21` 实机节拍 18/18 |
| `7ef18fe` | PARITY：fallback Esc 不对齐 Quit |

### 0098973 实机 11 项（本轮）

实机 Xfce/xfwm4 上 A,B,D,E,G,H,K 已过；下列 11 项在真实鼠标/快打字下仍开。Xvfb 慢节拍曾误报通过。

| ID | 问题 | 状态 | 修法 |
|---|---|---|---|
| 1 | 文字快打 “hello”+Ctrl+Enter 只留下 “hel” | 已修 | 缓存 keymap；拉丁文保持抓键；`0xff00+` 不当成 Unicode（Control_L≠￣）；晚到的字母仍插入 |
| 2 | pin 滚轮缩在中心，指针离开后多余格切工作区 | 已修 | 每格 `QueryPointer` 为锚；跟踪 `geom_pos` + winit `set_outer_position`（第二路 x11rb ConfigureRequest 会被 xfwm4 丢掉） |
| 3 | `,` `.` 打出 toast 但画面不变；`.` 清选区 | 已修 | 按键立刻 `request_redraw`；回放 `replace_shot` 保持 `primed=false`；回到 live 时保留选区（Python 同） |
| 4 | 新进程 R 无上次选区 | 已修 | `data_dir/history/` 落盘；R 只套最后一条 **rect** 到当前截图（不再 `replace_shot` 旧图） |
| 5 | toast 淡出后中心残留残字 | 已修 | 用 `text::measure` 算盒宽，恢复矩形再胀 12px |
| 6 | 按键后要鼠标动才重绘 | 已修 | 每个 X11Key / handle 脏时 `window.request_redraw()` |
| 7 | Esc 后立刻 F1 无日志 | 已修 | `OverlayKeyGrab::release` join 后立刻 ungrab；overlay 内 F1 → `Resnip`；关闭时队列里的 F1 转 `Hotkey::Snip` |
| 8 | 点矩形后第一笔快拖仍是箭头，第二笔空 | 已修→回归 | 曾用 recover 上升沿 `annot.press`；b791ea3 实机因此矩形/椭圆/箭头画不出。现改为 **仅 MouseInput press**，recover 只 move/release 已有 extent 的 stroke |
| 9 | 滚轮改选中图形线宽后面板仍显示 3 | 已修 | 面板用 `option_width()`（选中/live），滚轮后 `relayout` |
| 10 | 穿透项无勾；页脚 Zoom/Opacity 中文未译 | 已修 | 可勾项始终画勾槽；`match_pattern` 只拿 `{0}` 后到下一占位的分隔符 |
| 11 | overlay 开着 Chrome 仍收到 Tab | 已修 | 先放 `InputHold` 再 `XGrabKeyboard`；根窗口被动抓 Tab/ISO_Left_Tab；默认关 IME；失败 1ms 重试 |

### b791ea3 实机（Xfce/xfwm4，2026-10-12 07:32 UTC+8；附件 `rust-test-b791ea3_a248.md` / `_54b5.md`）

当时 PASS：文字快打+Ctrl+Enter；`,` `.` 历史 + 重启后 R；toast 淡出；按键即重绘；Esc 后 F1；pin 菜单勾选+中文页脚；Ctrl+C；pin 按钮；prefs。

下列 FAIL / 未测已在 `7f9b7dc`…`876fc4f` 修好，并经 **`7ef18fe` 实机（08:33 UTC+8）确认全过**。

| ID | 问题 | 状态 | 修法 / 实机 |
|---|---|---|---|
| 1 | **回归**：矩形/椭圆/箭头画不出；日志只有 `recover annot press`，无 drag/release | 已修 | `7f9b7dc`：recover **禁止** `annot.press`。`7ef18fe` 实机：三工具能画出 |
| 2 | pin 滚轮缩小锚在左上：算出 nx,ny 但窗停在 148,100；多出的格滚到 Chrome / 切工作区 | 已修 | `geom_pos` + `set_outer_position`。`7ef18fe` 实机：锚在指针，不切工作区 |
| 3 | overlay 里 Tab 漏到 Chrome（Jump to content） | 已修 | 根窗口抓 Tab。`7ef18fe` 实机：Chrome 收不到 Tab |
| 4 | R 后背景是旧截图（应为当前屏幕 + 上次选区） | 已修 | `reuse_last` 只 `set_rect`。`7ef18fe` 实机：重启后 R 套在 live 截图 |
| 5 | Esc 关 pin 菜单也关掉 pin，其它已恢复的 pin 消失 | 已修 | 菜单 Esc 只关菜单；fallback 不再 Esc=Quit。`7ef18fe` 实机：只关菜单 |
| 6 | pin 右键菜单很高，末项下大块空白 | 已修 | 高度按行求和（Sep=8）。`7ef18fe` 实机：高度正常；穿透勾选可切换 |
| — | 选中图形滚轮改线宽（当时被 #1 挡住） | 已修 | `7ef18fe` 实机：点边出 8 手柄，滚轮改宽，面板数字跟着变。双倍滚轮见本轮 `WheelGate` |

### `ed59e0f` 之后（本轮）

| SHA | 内容 |
|---|---|
| `ed59e0f` | 用户：overlay 用共享 `toggle_tool` |
| `7c406a6` | 用户：单击再点同一工具取消；语言标签回归测试 |
| `a6d6765` | 用户：Windows 设置事件循环 + 截图焦点 |
| `7d5ebb7` / `c344764` / `6edae0a` | `WheelGate` 去双倍滚轮；Fill/Dash/Dot 子面板；overlay 内 ConfirmAbort；pin 阴影/缩略；zenity 自定义色（已被本轮内置取色替换）；画笔 8ms poll |
| `f32767c` / `da81652` / `0ff0169` | A–F：选区移动带标注；内置 HSV 取色；透明度写到 frame；Ctrl 滚轮读 QueryPointer；prefs ABOVE；矩形 Dash/Dot；双击复制同点才触发 |
| `23335a9` | `0ff0169` 实机两项：`add` 选中刚画的形状（手柄可见，Solid/色仍改它）；pin 按 Python 图+2M / ARGB / 发丝边，去掉顶左边暗条 |

### `7ef18fe` 实机（Xfce/xfwm4，2026-10-12 08:33 UTC+8）

**7 / 7 PASS。**

| # | 检查 | 结果 |
|---|---|---|
| 1 | 矩形 / 椭圆 / 箭头绘制（b791ea3 回归） | PASS（`7f9b7dc`） |
| 2 | 点边选中 → 8 手柄；滚轮改线宽；面板字段更新 | PASS（线宽数字会变；每格双倍见未关） |
| 3 | pin 滚轮缩放锚在指针，不切工作区 | PASS |
| 4 | overlay Tab 被吞，不漏到 Chrome | PASS |
| 5 | 重启后 R：上次选区套在当前截图 | PASS |
| 6 | pin 菜单 Esc 只关菜单；高度正常；穿透勾选可切换 | PASS |
| 7 | 快打 `hello world`+Ctrl+Enter；Ctrl+C PNG；prefs；`,` `.` 历史 | PASS |

---

## 已发现的全部问题（各轮测试）

状态：`已修` / `未关`。52d5642 实机 A–L、0098973 实机 11 项、b791ea3 实机 6 项 + 滚轮改宽已关。双倍滚轮已关。**仍开：Windows 实机复测。**

### Windows 早期轮次

| 问题 | 状态 | 说明 |
|---|---|---|
| Overlay 黑屏 | 已修 | DXGI 空白帧；`6664a0f` |
| 托盘每一项都开截图 | 已修 | 先排空 `MenuEvent`，仅左键 Trigger 截图 |
| 文字工具无效 | 已修 | 虚线编辑框 + IME `Commit` + 按键转发（Linux IME 仍见 F） |
| 滚轮不改选中图形线宽 | 已修 | `option_target` / `wheel(angle/120)` + 0.5s 撤销合并（Linux 再回归见 G） |
| Release 弹出控制台 | 已修 | `windows_subsystem = "windows"`；`--debug`/`--version` 再挂控制台 |

### Linux 5549fbf 实机（Xfce，DISPLAY :2，picom）

| 问题 | 状态 | 说明 |
|---|---|---|
| overlay/pin 键盘无焦点（焦点留在 Chrome） | 已修 | `XGrabKeyboard` + `XSetInputFocus`；pin 非 override-redirect |
| 缩小 3 格后右键 `f32::clamp` panic | 已修 | `clamp_f32`；panic+backtrace → `pinshot.log` |
| 右键弹出标注工具栏而非上下文菜单 | 已修 | `PinMenuWin`，不再 zenity 失败回退到工具栏 |
| fallback「首选项」无窗无日志 | 已修 | 主循环 `PrefsWin`（52d5642 仍有 A/B） |
| 换工具后第一笔仍是上一工具；选箭头改写上一笔直线 | 已修 | `set_style_default` + 绘图按下不抢上一笔（G 要求点边再选中，见下） |
| Ctrl+滚轮透明度无可见效果 | 已修 | `_NET_WM_WINDOW_OPACITY`（需合成器） |
| 重启后标注丢失 | 已修 | PNG 原图 + `session.toml` `shapes` |
| 颜色面板无线宽；选中无手柄 | 已修 | −/+ 与 8 手柄（G：点边仍选不中） |
| 放大镜局部空白 | 已修 | 整数格 + 逐像素填充 |

### Linux 52d5642 实机（Xfce/xfwm4 1920×1200，2026-10-11）

报告里 **PASS** 的不再单列。下列为 **FAIL A–L**。

| ID | 问题 | 状态 | 修法 |
|---|---|---|---|
| A | 语言/主题/登录等值按钮只响应字段左半边 | 已修 | 命中区覆盖整行（含右侧值盒） |
| B | 切英文不完整：标题仍「首选项」；「PinShot Preferences」字叠在语言行；fallback 要重启才换语言；Apply 在 Cancel 右侧多画 “Apply”；Light 主题无变化 | 已修 | 去掉叠字标题；`set_title`；Apply 后重绘 fallback；状态「已应用。」；`UiTheme` 浅/深 |
| C | Tab 元素模式只高亮整窗（AT-SPI probe false）；R 不能回上次选区；逗号出 toast 但不回放历史；toast 不消失 | 已修 | 按 client 走 viewable 子窗 + AT-SPI 矩形；R 与 Shift+R；空历史不 toast；`replace_shot` 全幅刷新；toast 1.2s |
| D | F1 后第一笔拖动有时丢失（第二次 F1） | 已修 | 隐藏 fallback 即 `InputHold`；映射后 **禁止** 第二连接/`Xlib` `XGrabPointer`；`Poll` + `QueryPointer` 认领仍按下的 Button1 |
| E | 箭头工具画出的是无箭头直线 | 已修 | Line 默认无箭头；Arrow 开箭头；更大箭头头 |
| F | 文字：第一击无框（键入丢失）；第二击才有；Ctrl+Enter 提交带「–」；中文被丢（无 IME） | 已修 | 空编辑器重定位不提交；Ctrl 不插入；滤 en/em dash；文字时 ungrab 键盘给 IME |
| G | 点图形边选不中（无 8 手柄）；文字工具点边上会新开文本；滚轮在图形上不改线宽 | 已修 | 与 Python 相同：非橡皮/马赛克点中即选；`wheel_at` |
| H | Ctrl+C 日志已复制，overlay 关掉后剪贴板空 | 已修 | 进程内常驻 `Clipboard` + `xclip -t image/png` 移交 |
| I | pin 滚轮缩小锚在左上，指针离开窗口，多余滚轮被桌面吃掉切工作区 | 已修 | `set_scale_at` 以光标为锚；`geom_pos` + winit 移窗 |
| J | 配置里 `hotkey_clickthrough=""`；热键页显示「—」；菜单穿透无勾 | 已修 | 默认/空值填 F4；菜单勾选改画色块（字体无 ✓） |
| K | 恢复的每张 pin 弹出「Opacity: 100%」且不消失 | 已修 | `set_opacity_ex(..., false)`；toast 1.2s 后清 |
| L | overlay 打开时 Chrome 偶尔仍收到键；overlay 重绘有时慢 | 已修 | `XGrabKeyboard(owner_events=false)` + 丢焦点重抓；无 dirty 不 present |

附：该轮「贴剪贴板按钮」和「8 手柄」随 G / 单元测试补验。

### Xvfb + xfwm4 + xdotool（`:10` 1920×1200，`d01fe6c`）

脚本 `/tmp/pinshot-al-verify.sh`，结果 `pass=15 fail=0`。

| 检查 | 结果 |
|---|---|
| A/B 点语言行右侧值盒 → 标题 `PinShot Preferences`，Apply 后 `language = "en"` | PASS |
| J 空 `hotkey_clickthrough` Apply 后写成 `F4`；pin 菜单绿块勾选 | PASS |
| L Chrome 聚焦时 `XGrabKeyboard ok` | PASS |
| D 首笔拖动 `recover Button1` → `overlay copy 360x280` | PASS |
| H overlay 关闭后 `xclip TARGETS` 含 `image/png`（360×280 PNG） | PASS |
| C `,` / `R` 打出 `toast: Snip history` / `Last snip area`，选区 360×280 回放 | PASS |
| I pin 滚轮缩小 300×220 → 140×103，位置随光标移动 | PASS |
| K 重启恢复 pin，无 `Opacity: 100%` toast | PASS |
| E/F/G | `cargo test --workspace` 101 通过（箭头头、文字首击/滤 dash、点边选中+`wheel_at`） |

工件：`/opt/cursor/artifacts/al-verify/`（截图 + `clipboard.png` + `report.txt`）。

---

## 当前 `PARITY.md` 计数

| 状态 | 行数 |
|---|---|
| done | 103 |
| partial | 10 |
| missing | 1 |
| impossible | 6 |
| **合计** | **120** |

仍偏 partial/missing：完整字体下拉、像素级 hit、无 a11y 时 AT-SPI、pin 分组改名、OCR 条带、`PynputHotkeys`。

---

## 仍开（本轮之后）

| 项 | 状态 | 说明 |
|---|---|---|
| Linux 滚轮每格两次 | 已修 | `WheelGate` 去重；Xvfb 连续滚轮应 3 格→+3 宽、4 格缩放 4 步 |
| Windows 实机 | 未关 | Linux-first 改动后只跑过 CI，尚未在真 Windows 上复测、未出新的 release exe |
| PARITY 缺口 | 部分关 | ConfirmAbort / 阴影缩略 / 画笔 poll / 子面板 / 内置取色已做；剩字体家族、像素 hit、Pynput（103/10/1/6） |
| 6edae0a 实机 A–F | 已修 | 见下表 |
| 0ff0169 实机两项 | 已修 | 刚画完的形状：`option_target` 仍改它，并画出 8 手柄；pin 顶/左边暗条：窗=图+2M + ARGB |

---

## 已知限制

| 项 | 说明 |
|---|---|
| AT-SPI | 子进程探测 + 矩形转储；无 a11y 总线时 `probe false`，走 X11 viewable 子窗 |
| IME / 中文输入 | 仅 IME preedit 时放抓给 XIM；拉丁文保持 `XGrabKeyboard`（快打不再丢字） |
| 合成器 | `_NET_WM_WINDOW_OPACITY`、阴影依赖 picom/compton；无合成器时属性在、观感不变 |
| 剪贴板 owner | 进程内 arboard + `xclip` 移交；无 xclip 时依赖进程不退出 |
| 纯 Wayland | 与 Python 相同，只走 X11/XWayland |
| 划词翻译 | Linux 上 Python 也拒绝，Rust 同样 |
| 排除截图 (WDA) | 仅 Windows 10 2004+ |

---

## 下一步（按优先级）

1. **Windows 实机 + 放出 exe**：在真机复测 overlay/pin/热键/剪贴板，CI 已绿的 `pinshot.exe` 再打一版。
2. **PARITY 剩余**：RapidOCR 完整 DBNet（条带检测已能出字）。本轮已关：3×3 hit、F6 toast、字体芯片、pin 分组 New/Rename、AT-SPI exclude/`role:name`、OCR `_NET_RESTACK_WINDOW`。

### 6edae0a 实机 OPEN A–F（本轮）

实机报告：滚轮已过。下列 A–F 在本轮对齐 Python（选区坐标 / 无 zenity）。

| ID | 问题 | 状态 | 修法 |
|---|---|---|---|
| A | 关掉工具后拖动选区，标注留在原屏幕位置（被裁切） | 已修 | `Annotator::translate_all`：Moving / 方向键 Move 时按选区位移×dpr 平移全部点与 undo。双击复制只认**同点**（12px），避免点工具再拖选区被当成双击 Copy |
| B | 当前色块调 zenity，无 zenity 则无反应 | 已修 | overlay/pin 内 HSV+调色板+OK/Cancel；不放抓、不依赖外部进程 |
| C | `_NET_WM_WINDOW_OPACITY` 已设但 picom 下仍不透明 | 已修 | 属性写到 client **和** WM frame；`apply_geom` 后重设 |
| D | 未聚焦 pin 上 Ctrl+滚轮不可靠 | 已修 | `QueryPointer` mask Control=4，与 winit mods 或运算 |
| E | 首选项开在置顶 pin 下面 | 已修 | prefs `AlwaysOnTop` + `raise_on_top` |
| F | 矩形面板无 Solid/Dash/Dot | 已修 | 与直线相同的线型芯片；`render` 已用 `line_style` |

验证约定：Linux 改动必须在 **Xvfb + xfwm4 + xdotool**（以及有桌面时的 `:1` Xfce）上做焦点/按键/菜单回归。本轮用 `type --delay 20`、两步快拖、连续滚轮格（间隔 80ms > 门控 40ms）。

### 0ff0169 实机两项（本轮，`23335a9`）

实机 A–F 全过（picom 下 pin 半透明）。两项对齐 Python：

| ID | 问题 | 状态 | 修法 |
|---|---|---|---|
| 1 | Solid/取色也改刚画的矩形，但看不见选中 | 已修 | Python `add` 只设 `width_target`，`option_target` 仍改该形状。保留该语义，并设 `selected`/`edit_target`，按 `option_target` 画 8 手柄 |
| 2 | pin 顶/左边一条暗带（阴影偏移 / 1px 边） | 已修 | 窗按 Python 图+2M、原点图−M、`with_transparent` + `present_argb`；先阴影再贴图；发丝边 `adjusted(-0.5,-0.5,-0.5,-0.5)`；圆角阴影 |

### Xvfb `23335a9` 两项（`:24` 1920×1200 + xfwm4 + picom）

脚本 `/tmp/pinshot-findings-verify.sh`。`pass=11 fail=0`。

| 检查 | 结果 |
|---|---|
| picom `_NET_WM_CM_S0` | PASS |
| Dot 画矩形后 `selected=Some(0)` | PASS（`annot Rect 260x220 selected=Some(0)`） |
| 8 手柄可见 | PASS（8/8 点到 `(30,144,255)`） |
| Solid 改刚画的矩形 | PASS（顶边由虚线变为两段连续红 116px；手柄仍 8/8） |
| pin 窗=图+2M | PASS（`pin create 584x484 at 348,228 margin=12 transparent=true`；选区 560×460 @360,240） |
| 照片顶/左无暗带 | PASS（图内 y/x≥12 为白；y=11 发丝蓝；y=0–10 为外边距阴影） |

工件：`/opt/cursor/artifacts/findings-verify/`。

### PARITY 剩余项（本轮，Linux-first）

对照 `2d2a328` 表里 8 个 partial + 1 个 missing（页脚 10/1 是旧计数）。

| 项 | 状态 | 修法 |
|---|---|---|
| 3×3 像素 hit | **done** | `shape_paints_at`：3×3 pixmap，平移使点落在 (1,1)，alpha>16。箭头 margin 仍先做 bbox。马赛克/橡皮保持 path-contains |
| `option_width` 文字/马赛克 | **done** | 文字显示 `font_size`，马赛克显示 `size`（Python 面板转盘） |
| 字体家族 + B/I 进渲染 | **done** | 只选可加载 TTF/OTF（Liberation/DejaVu 优先）；Sans 不匹配 DejaVuSansMono；B/I 走 Bold/Italic 文件，否则假粗/假斜 |
| F6 toast | **done** | `translate_on_select` 开 overlay 时 toast「框选文字后松开」 |
| AT-SPI / X11 子窗 | **done** | 行走深度 4；AT-SPI 标 `{role}:{name}` 并排除本进程 pid |
| `raise_on_top` / OCR 压过 pin | **done** | `_NET_RESTACK_WINDOW` Below `lower`，再 ABOVE+Above+ACTIVE。OCR 在 0/60/250ms 抬，不再每 tick 查 X |
| pin 分组 New/Rename | **done** | KeyboardInput 在非 preedit 时写入（与 overlay 文字相同）；`Ime::Enabled` 不挡键；空 Enter 不建空分组；Default 不能改名 |
| `PynputHotkeys` | **done** | 按 Python 在无 pynput/evdev 时的 XGrabKey 回退 |
| RapidOCR DBNet | **partial** | 仍用条带检测，完整 DB 太大 |

单元测试：`cargo test --workspace --offline --lib` → pinshot-core 122 + pinshot 20。Windows 交叉编译在本环境缺 SDK（`ring`/`assert.h`），以 CI 为准。

### `a08bf05` 实机两项（本轮）

| ID | 问题 | 状态 | 修法 |
|---|---|---|---|
| 1 | B/I/Serif/Mono 几乎看不出；B 像素相同；I 可能落到 DejaVuSansMono；Serif/Mono 被 PFB 抢走 | **已修** | 只接受 TTF/OTF/TTC；Liberation/DejaVu 优先并必须 `fontdue` 能加载；Sans 排除 `mono`/`serif`；无 Bold/Italic 文件则假粗/假斜 |
| 2 | 分组 New/Rename 不能打字：`Ime::Enabled` 把 KeyboardInput 丢掉 | **已修** | 与 overlay 文字相同：非 preedit 时收 `event.text`；只有非空 Preedit 才挡键；`Ime::Commit` 仍收 CJK；空/空白 Enter 忽略。提示期间失焦不关窗；无 preedit 时 Enter 才提交 |

### Xvfb `:26` 复测（1920×1200 + xfwm4）

脚本 `/tmp/pinshot-font-ime-verify.sh`。`pass=9 fail=0`。

| 检查 | 结果 |
|---|---|
| pin 创建 | PASS（`pin create 420x300`） |
| 右键菜单 | PASS（`pin menu new-group at …`） |
| New 提示框 | PASS（`pin menu prompt new group`） |
| 打 ASCII `hello` | PASS（`pin menu prompt text "hello"` + 画面 `hello|`） |
| Enter 提交 | PASS（`pin menu new group named`） |
| Text 工具 + B/I/Family | PASS（`tb Bold(true)` / `tb Italic(true)` / `tb Family`） |
| 字体像素 | PASS（`bold_and_italic_change_paint`：Default/Sans B/I 与 Serif/Mono 互异；Liberation TTF） |

### Xvfb `5264145` 实机两项（`:26` 1920×1200 + xfwm4）

脚本 `/tmp/pinshot-a08-verify.sh`。`pass=25 fail=0`。

| 检查 | 结果 |
|---|---|
| Text `HiBg` 提交 | PASS（`text commit "HiBg"`） |
| Bold 像素 | PASS（相对 Regular **666** px；LiberationSans-Bold.ttf） |
| Italic 像素 | PASS（**615** px；LiberationSans-Italic.ttf） |
| Serif 像素 | PASS（**636** px；LiberationSerif-Regular.ttf） |
| Mono 像素 | PASS（**743** px；LiberationMono-Regular.ttf） |
| Sans ≠ Mono / 无 PFB | PASS（LiberationSans-Regular.ttf） |
| New 空 Enter | PASS（`pin menu prompt empty ignored`，未 `submit ""`） |
| New 键入 Work | PASS（`pin menu prompt submit "Work"` + `new group named`） |
| Rename 键入 Jobs | PASS（`pin menu prompt submit "Jobs"`；菜单 838 含 Work 行） |

工件：`/opt/cursor/artifacts/a08-verify/`。overlay/pin 在编文字时也走 `blit_pixmap_ex`（B/I/家族即时可见）。Windows 短名 `segoeuib`/`segoeuii` 算 Bold/Italic，CI 不再要求 Liberation 文件名。

### Xvfb `6f19f21` PARITY 剩余项（`:25` 1920×1200 + xfwm4）

脚本 `/tmp/pinshot-parity-verify.sh`。`pass=11 fail=0`。

| 检查 | 结果 |
|---|---|
| `--translate` overlay | PASS |
| F6 toast「Select text, then release」 | PASS |
| Text 工具 | PASS（`tb Tool(Text)`） |
| 字体芯片 | PASS（`tb Family("")` Default） |
| pin 创建 | PASS |
| 菜单高度含 Rename | PASS（`pin menu 268x816`，比有 Shadow 的 794 多一行） |
| Rename 名称框 | PASS（`pin menu prompt rename group`） |
| Default 不能改名 | PASS（`pin menu rename skipped (Default)`，与 Python 相同） |

工件：`/opt/cursor/artifacts/parity-verify/`。3×3 hit / `option_width` 文字·马赛克由单元测试覆盖。OCR restack 与 AT-SPI 实总线仍需真机。

### Xvfb `da81652` A–F（`:23` 1920×1200 + xfwm4，实机节拍）

脚本 `/tmp/pinshot-af-verify.sh`。`pass=14 fail=0`。

| 检查 | 结果 |
|---|---|
| A 关工具后拖选区 | PASS（`annot translate 120,100`；标注留在洞内） |
| B 内置取色 | PASS（`color picker open #FF0000` → `custom color #0088FF`；无 zenity） |
| C 透明度属性 | PASS（`opacity set 0.90 client=… frame=…`；xprop 非 `0xffffffff`。Xvfb 无 picom，观感仍不透明） |
| D 未聚焦 Ctrl+滚轮 | PASS（第二次 `opacity set 0.80`） |
| E prefs 置顶 | PASS（`prefs raise above`；`_NET_WM_STATE_ABOVE`） |
| F 矩形 Dash | PASS（`tb LineStyle(Dash)`；画面虚线） |

工件：`/opt/cursor/artifacts/af-verify/`。

### Xvfb `c344764` 双倍滚轮 + PARITY（`:22` 1920×1200 + xfwm4，实机节拍）

脚本 `/tmp/pinshot-wheel-verify.sh`。`pass=15 fail=0`。

| 检查 | 结果 |
|---|---|
| 3 格滚轮改宽 | PASS（3 条 `overlay wheel +120`，width 3→4→5→6，不是 3→9） |
| 4 格 pin 缩放 | PASS（4 条 `pin zoom`，560×460→382×314，锚随指针） |
| Fill 芯片 | PASS（`tb Fill(true)`） |
| Esc ConfirmAbort | PASS（`confirm abort shown`；N 留下 overlay；Y 放弃） |
| 菜单含 Shadow | PASS（`pin menu 268x794`，比无 Shadow 的 772 多一行） |
| Esc 只关菜单 | PASS |

工件：`/opt/cursor/artifacts/wheel-verify/`。

### Xvfb `876fc4f` 六项（实机节拍，`:21` 1920×1200 + xfwm4）

脚本 `/tmp/pinshot-b791-verify.sh`，`type --delay 20`、快拖、连续滚轮。`pass=18 fail=0`。

| 检查 | 结果 |
|---|---|
| 1 矩形/椭圆/箭头 | PASS（`annot Rect 260x220` / `Ellipse 200x180` / `Line 270x180`；无 `recover annot press`） |
| 2 pin 连续缩小锚光标 | PASS（320×240+600+220 → 123×93+731+318；每格 `geom_pos` 不同） |
| 3 Tab | PASS（`overlay Tab swallowed`；xev 无 KeyPress） |
| 4 R 套在当前截图 | PASS（`R last rect 560x460 on live capture 1920x1200`） |
| 5 Esc 只关菜单 | PASS（菜单后仍 2 个 pin；fallback 不再 Esc=Quit） |
| 6 菜单高度 | PASS（`pin menu 268x772`，分隔行 8px；末项下无空洞） |

工件：`/opt/cursor/artifacts/b791-verify/`（`03-shapes.png` 可见三笔标注）。

### Xvfb 0098973 十一项（`:20` 1920×1200 + xfwm4）

| 项 | 结果 |
|---|---|
| 1 快打 hello + Ctrl+Enter | PASS（`text commit "hello"`，画面无 ￣） |
| 2 pin 连续缩小锚光标 | PASS（80×50→55×34，原点 40,40→44,44） |
| 3/6 `,` `.` 不移鼠标即重绘 | PASS（toast「截图历史」） |
| 4 重启后 R | PASS（`restored 1` + toast「上次选区」） |
| 5 toast 淡出无残字 | PASS（中心无残留） |
| 7 Esc 后立刻 F1 | PASS（ungrab + 重新 mapped） |
| 10 菜单勾槽 + 中文页脚 | PASS |
| 11 Tab 不到 xev | PASS |
| 8/9 工具栏命中 | 脚本点偏时会 miss；`tb Tool(Text)` / `recover annot press` 在对准后通过 |
