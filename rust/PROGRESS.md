# PinShot Rust 重写进度

对照源：`main` 上的 Python/PySide6 PinShot。Rust 在 `rust/`，分支 `rust`。Python 源码不改。

最后更新：2026-10-12（b791ea3 实机回归：recover annot.press 导致矩形/椭圆/箭头画不出；本轮改回事件路径 press）。

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

### b791ea3 实机（Xfce/xfwm4，2026-10-12）

PASS：文字快打+Ctrl+Enter；`,` `.` 历史 + 重启后 R；toast 淡出；按键即重绘；Esc 后 F1；pin 菜单勾选+中文页脚；Ctrl+C；pin 按钮；prefs。

| ID | 问题 | 状态 | 修法 |
|---|---|---|---|
| 1 | **回归**：矩形/椭圆/箭头画不出；日志只有 `recover annot press`，无 drag/release | 已修 | recover **禁止** `annot.press`；只在已有 extent 时 move/release。事件路径 `press→move_to→release` 单测 Rect/Ellipse/Arrow |
| 2 | pin 滚轮缩小锚在左上：算出 nx,ny 但窗停在 148,100；多出的格滚到 Chrome | 已修 | 跟踪 `geom_pos`（不重读过期 `outer_position`）；始终 `set_outer_position` + `request_inner_size` + EWMH；滚轮时 raise/focus |
| 3 | overlay 里 Tab 漏到 Chrome（Jump to content） | 已修 | 根窗口 `XGrabKey` Tab/ISO_Left_Tab（`owner_events=false`）；默认 `set_ime_allowed(false)`；抓键 1ms 重试 |
| 4 | R 后背景是旧截图（应为当前屏幕 + 上次选区） | 已修 | `reuse_last` 只 `set_rect`，不 `replace_shot` |
| 5 | Esc 关 pin 菜单也关掉 pin，其它已恢复的 pin 消失 | 已修 | 菜单开着或刚关 450ms 内 pin 不处理 Esc；菜单 Esc 只关菜单 |
| 6 | pin 右键菜单很高，末项下大块空白 | 已修 | 高度按行真实高度求和（Sep=8，其它=22），不再 `len*ROW_H` |

未测：选中图形滚轮改线宽（被 #1 挡住；#1 修后事件路径单测覆盖绘制）。

---

## 已发现的全部问题（各轮测试）

状态：`已修` / `未关`。52d5642 实机 A–L、0098973 实机 11 项、b791ea3 实机 6 项本轮已关。

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
| done | 99 |
| partial | 11 |
| missing | 4 |
| impossible | 6 |
| **合计** | **120** |

仍偏 partial/missing 的大块：属性面板细节、自定义取色、pin 阴影/缩略图、无 a11y 总线时的 AT-SPI、逐工具子面板、画笔 8ms 采样、像素级 hit。

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

1. **P1 — 对齐缺口**：无 a11y 时更细的控件框、属性面板（填充/虚线/模糊/字体）、自定义取色、pin 阴影。
2. **P2 — Windows 再对齐**：在 Linux 稳定后，用同一套 core 把 Windows 宿主接到同等行为；CI 两边保持绿。
3. **P3 — 体积与 OCR**：默认 exe 保持无模型；`--features ocr` / `pinshot-ocr` 文档化。

验证约定：Linux 改动必须在 **Xvfb + xfwm4 + xdotool**（以及有桌面时的 `:1` Xfce）上做焦点/按键/菜单回归。本轮用 `type --delay 20`、两步快拖、连续滚轮格。

### Xvfb b791ea3 六项（实机节拍，`:21` 1920×1200 + xfwm4）

见本轮提交后的 `/tmp/pinshot-b791-verify.sh` 结果（绘制走事件路径、pin 缩放位移、Tab 不漏、R 活截图、Esc 只关菜单、菜单高度）。

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
