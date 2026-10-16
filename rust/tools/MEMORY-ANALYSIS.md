# PinShot 运行时内存分析：实测结论

采样对象：`target\release\pinshot.exe --debug`（build with `--features ocr`）
机器：LENOVO 83AH / 16 GB / Intel Iris Xe + NVIDIA RTX 3050 + AskLink 虚拟显示器 + VMware
工具：`tools\mem-probe.ps1`、`tools\mem-regions.ps1`、`tools\mem-modules.ps1`、`tools\mem-compare.ps1`

## 0. 结论速览

| 场景 | 内存 | 成因 |
|---|---|---|
| 启动后待机 | **16–32 MB** | 正常（OCR 后会自动回落） |
| 常驻 Private（任务管理器） | **可能显示 3000 MB** | **假象**，GPU 驱动显存预留，见 §1 |
| OCR 峰值（server + WebGPU，原配置） | 3020 MB Private / 15.9 s | 见 §2 |
| **OCR 峰值（mobile + CPU，推荐）** | **678 MB Private / 6.6 s** | **降 78%，快 2.4 倍** |

**一句话：任务管理器里那个 3000 MB 是假的；真正要治的是 OCR——
把 `ocr_model` 改成 `"mobile"` 即可，WebGPU 在这套模型上反而是负优化。**

各阶段实测明细（同一张 1686×729 截图，`max_side=2560`）：

| 阶段 | WS | Private |
|---|---|---|
| startup | 32.1 MB | 10.6 MB |
| ocr:before-recognize | 302.5 MB | 288.5 MB |
| ocr:pre-load-models | 314.7 MB | 300.6 MB |
| **ocr:post-load-models（mobile+CPU）** | **417.7 MB** | **419.9 MB** |
| **ocr:post-infer（mobile+CPU）** | **647.5 MB** | **678.0 MB** |
| OCR 结束（引擎释放后） | 16–32 MB | — |

**内存峰值在 detection 阶段就到了**：`ocr:post-detect` 已经是 657.3 MB，
到 `ocr:post-infer` 只涨到 647.5 MB，识别阶段没再增加。

## 1. 关键结论：那 3 GB 不是 PinShot 的

| 进程 | Private | WorkingSet |
|---|---|---|
| **pinshot** | **2,987 MB** | **9.2 MB** |
| devenv | 1,315 MB | 75 MB |
| WorkBuddy | 884 MB | 796 MB |
| explorer | 495 MB | 100 MB |
| dwm | 307 MB | 31 MB |

pinshot 的 Private 是全机第一，是 `dwm` 的近 10 倍，但**驻留只有 9 MB**。
`Private` 高而 `WorkingSet` 极低 = 提交量大但几乎没碰过 = 不是堆，是**预留/映射的显存镜像**。

按模块归因（`mem-modules.ps1`，≥4 MB 的提交区）：

| 大小 | 归属 |
|---|---|
| **2,648.8 MB** | `<image, no module match>` — 无模块归属的 IMAGE 区 |
| 79.7 MB | igc64.dll（Intel 核显驱动） |
| 77.2 MB | nvwgf2umx.dll（NVIDIA 用户态驱动） |
| 75.3 MB | nvgpucomp64.dll（NVIDIA 着色器编译器） |
| 57.2 MB | `<mapped file>` |
| **21.9 MB** | **pinshot.exe ← 真正的程序自身占用** |
| 13.3 MB | dxcompiler.dll（HLSL 编译） |
| 4.6 MB | d3d10warp.dll（WARP 软件光栅） |
| 4.3 MB | webgpu_dawn.dll |

栈是完整的三套图形栈同时在场：Intel 核显 + NVIDIA + D3D12 WARP + Dawn(WebGPU)。
这台机器还挂着 **VMware 虚拟显卡 + AskLink 虚拟显示器 + Todesk 虚拟显示器**，
虚拟显示驱动会在每个创建 D3D 设备 / 桌面复制的进程里预留大段显存虚拟地址。

因此 **2,987 MB 里，PinShot 自己的代码和数据只占 21.9 MB（0.7%）**。

## 2. OCR 的内存：mobile 档实测对比（最终结论）

同一张 1686×729 截图，`max_side=2560`，OCR 成功完成。

| 指标 | server + WebGPU | mobile + WebGPU（崩） | **mobile + CPU** |
|---|---|---|---|
| post-load-models WS | 629.3 MB | 457.3 MB | **417.7 MB** |
| post-infer WS | 1822.5 MB | 733.2 MB（失败值） | **647.5 MB** |
| post-infer Private | 3018.2 MB | 850.5 MB（失败值） | **678.0 MB** |
| OCR 耗时 | 15920 ms | 失败 | **6590 ms** |
| blocks | 73 | — | **76** |
| chars | 848 | — | **846** |

### 最终建议：mobile 档 + CPU EP

**Private 从 3018 MB 降到 678 MB，降了 78%。**
**耗时从 15.9 秒降到 6.6 秒，快了 2.4 倍。**
识别质量没有下降（846 vs 848 字符，blocks 76 vs 73）。

WebGPU 在这套模型上**没有带来任何收益**：

- server 档在 GPU 上要 15.9 秒，mobile 档在 CPU 上只要 6.6 秒；
- mobile 模型在 WebGPU 上直接崩（WGSL 不兼容）；
- GPU 堆还是内存大头——server + GPU 的 Private 是 mobile + CPU 的 4.5 倍。

模型大小对比：

| 档位 | det | rec | 合计 |
|---|---|---|---|
| server | 88.1 MB | 84.6 MB | 172.7 MB |
| mobile | 4.8 MB | 16.6 MB | **21.4 MB** |

### 三个阶段的三种配法

```toml
# config.toml
ocr_model = "mobile"
```

```powershell
# 启动时强制 CPU EP（mobile + WebGPU 会崩）
$env:PINSHOT_OCR_GPU = "0"
target\release\pinshot.exe --debug
```

或者用新增的 `CPU_FALLBACK` 机制：**首次失败后自动切 CPU**，第二次起无需设置环境变量。

### 陷阱：mobile 档曾经静默失效


改成 mobile 后日志显示的仍是：

```
ocr using fallback server det=...\ch_PP-OCRv5_server_det.onnx
ocr done 823 chars ... kind=server ...
```

**配置改了完全没生效，也没有任何提示。**

根因在 `ocr_run.rs` 的 `ensure_models_with_progress()`：mobile `ready()` 为 false →
检查 server `ready()` 为 true → **直接 return fallback**，
只有两档都缺失才走下载分支。本地只有 server 权重时，mobile 配置永远被忽略。

已修：Mobile → Server 的 fallback 现在打印明确警告。
Server → Mobile 保持静默（mobile 代价低，静默默认合理）。

手动补下载（代码不会自动下）：

```bash
cd ~/AppData/Roaming/PinShot/ocr-models
curl -sL -o ch_PP-OCRv5_mobile_det.onnx "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.9.2/onnx/PP-OCRv5/det/ch_PP-OCRv5_det_mobile.onnx"
curl -sL -o ch_PP-OCRv5_mobile_rec.onnx "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.9.2/onnx/PP-OCRv5/rec/ch_PP-OCRv5_rec_mobile.onnx"
```

实测 **det 4.8 MB + rec 16.6 MB = 21.4 MB**，对上注释的 "~20 MB"。
两档文件名不同，共存不冲突。

### 更直接的杠杆：`DET_MAX_SIDE`

**已改为环境变量可调**，无需重编译：

```rust
// pinshot-core/src/ocr.rs
pub fn det_max_side() -> u32 {
    if let Ok(v) = std::env::var("PINSHOT_OCR_MAX_SIDE") {
        if let Ok(n) = v.trim().parse::<u32>() {
            return n.clamp(640, 4096);
        }
    }
    DET_MAX_SIDE
}
```

clamp 到 [640, 4096]：低于 640 小字检测会崩，高于 4096 只涨成本没有收益。
非法值回退到默认，不会静默禁用检测。已加单元测试
`det_max_side_defaults_and_clamps_env_override` 覆盖这四种情况。

同时在 `infer()` 里加了：

- `drop(rgb)` + `ocr:post-detect` 埋点——检测激活是峰值，识别阶段不应再背着
  补边后的 RGB 缓冲；
- `ocr done` 行补上 `max_side` 与 `blocks`，便于横向对比；
- `ocr:done` 埋点记录推理结束时的驻留。

一键扫描：

```powershell
.\tools\ocr-sweep.ps1 -Side 1280 -Trim
.\tools\ocr-sweep.ps1 -Side 1920
.\tools\ocr-sweep.ps1 -Side 2560
```

### 实测扫描结果（server 档，ep=gpu，同一张 1686×729 截图）

| max_side | 检测块数 | 识别字符 | 模型加载后 WS | 推理后 WS | 推理后 Private | 耗时 |
|---|---|---|---|---|---|---|
| 2560 | 73 | 848 | 643.0 MB | **1801.6 MB** | 3026.1 MB | 15920 ms |
| 1920 | 74 | 858 | 631.8 MB | **1810.5 MB** | 3018.0 MB | 17604 ms |
| 1280 | 26 | 843 | 657.3 MB | **1309.7 MB** | 1984.7 MB | **9880 ms** |

**两个反直觉的发现：**

**1. 1920 与 2560 几乎没差别**（1801 vs 1810 MB，耗时还更长）。
说明内存并非由 det 输入尺寸主导——`max_side` 只是**上限**，
而这张截图是 1686×729，2560 这个上限**根本没被触发**，det 实际按原尺寸跑。
所以降 `max_side` 到 1920 是无效操作。

**2. 1280 生效了，但代价是漏检。**
块数从 73 掉到 **26**（掉了 64%），耗时减半（15.9s → 9.9s），
内存降到 1309.7 MB（只降 27%）。
字符数 843 看着没少，是因为剩下 26 个大块里塞了同样的文字量，
但**版面结构丢了**——73 行变 26 行，选中文字的对应关系会错乱。

## 2. OCR 的内存：mobile 档实测对比

`ocr_model = "server"` → `"mobile"`，模型 172.7 MB → **21.4 MB**
（det 4.8 MB + rec 16.6 MB，实测）。

| 阶段 | server 档 | mobile 档 | 变化 |
|---|---|---|---|
| post-load-models ws | 629.3 MB | **457.3 MB** | −27% |
| post-infer ws | 1822.5 MB | **733.2 MB** | **−60%** |
| post-infer Private | 3018.2 MB | **850.5 MB** | **−72%** |
| 模型加载增量 | +306 MB | +144 MB | −53% |

**换 mobile 档是有效的**——推理后 Private 直接降了 72%。

### 但 WebGPU 与 mobile 模型不兼容

```
ocr fail: ocr detect: Ort(Error { code: GenericFailure, msg:
"WebGPU validation failed. Error while parsing WGSL: :91:13 error:
no matching call to 'clamp(vec4<f32>, abstract-float, abstract-float)'
    value = clamp(...) * value + ..., 0.0, 1.0);
" })
- While calling [Device].CreateShaderModule([ShaderModuleDescriptor]).
```

mobile 模型的 clip 算子生成了 Dawn 的 WGSL 解析器不接受的写法，
所以 **mobile 档必须配 CPU EP**。

### 缺陷：WebGPU 惰性编译让回退失效

`init_models_custom()` 返回成功，shader 却在首次 `detect()` 时才编译失败，
所以 `with_engine()` 里 `Err => 回退 CPU` 的分支**从未被触发**。

已修：新增 `CPU_FALLBACK` 标志；推理失败且消息含 `WebGPU`/`Shader` 时，
丢弃坏 session 并置位，`want_ocr_gpu()` 的 Auto 分支随后返回 false。

两个实现约束（重构时务必注意）：

1. `with_engine` 的回调是 `F: FnOnce(...)`——`f(ocr)` 之后 `f` 已被消费，
   **无法递归重试**。所以失败当次照实返回错误，只丢 session + 标记，
   让下一次 OCR 自然走 CPU。
2. `guard` 已持有 ENGINE 的 `Mutex` 锁，**不能再 `cell.lock()`**（会死锁）。
   必须先 `drop(ocr)` 再 `*guard = None`。

`ForceGpu` 仍然可以覆盖这个回退——用户可能已经修好驱动或换了模型。

把 `ocr_model` 改成 `"mobile"` 后，日志显示的仍是：

```
ocr using fallback server det=...\ch_PP-OCRv5_server_det.onnx
ocr done 823 chars ... kind=server ...
```

**配置改了完全没生效。**

根因在 `ocr_run.rs` 的 `ensure_models_with_progress()`：mobile `ready()` 为 false →
检查 server `ready()` 为 true → **直接 return fallback**，
只有两档都缺失才走下载分支。本地只有 server 权重时，mobile 配置永远被忽略。

已修：Mobile → Server 的 fallback 现在会打印明确警告。
Server → Mobile 保持静默（mobile 代价低，静默默认合理）。

手动补下载（代码不会自动下）：

```bash
cd ~/AppData/Roaming/PinShot/ocr-models
curl -sL -o ch_PP-OCRv5_mobile_det.onnx "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.9.2/onnx/PP-OCRv5/det/ch_PP-OCRv5_det_mobile.onnx"
curl -sL -o ch_PP-OCRv5_mobile_rec.onnx "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.9.2/onnx/PP-OCRv5/rec/ch_PP-OCRv5_rec_mobile.onnx"
```

实测 **det 4.8 MB + rec 16.6 MB = 21.4 MB**，对上 `ocr.rs:10` 注释的 "~20 MB"。
对比 server 的 172.7 MB，**小 8 倍**。两档文件名不同，共存不冲突。

## 3. CPU 40% 的来源

`ocr_run.rs:557`：

```rust
fn ocr_threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
        .clamp(2, 8)
}
```

这个值同时喂给 `with_intra_threads` 和 `with_inter_threads`（`ocr_run.rs:589-590`），
核多时两个池都开到 8，双份开销。若只想压 CPU，把 `.clamp(2, 8)` 改成 `.clamp(2, 4)`。

实测 server 档识别 1055 字耗时 **19.3 秒**（`ocr done ... 19329ms`），
这个耗时与档位和 EP 选择强相关，是比内存更值得优化的指标。

## 3.5 内存为何会在 OCR 后回落

`ocr_run.rs` 有 `release_engine()`，把 `ENGINE` 里的 session `take()` 掉。
实测 OCR 结束后进程回落到 **16.5 MB**。

所以 OCR 的 2771 MB 是**瞬时峰值**，不是常驻。这也解释了为什么平时监控只看到十几 MB。
排查时必须在 OCR 进行中采样，OCR 一结束就什么都看不到了。

## 3.6 判定指标

| 场景 | 看什么 | 原因 |
|---|---|---|
| 判断有没有泄漏 | **WorkingSet** | 真实物理占用，不受驱动预留干扰 |
| 判断程序逻辑内存 | Private − 驱动预留基线 | 基线在启动瞬间就确定 |
| 关注显存/图形栈 | Virtual | 只是地址空间，无成本 |
| 观察 OCR 开销 | OCR 期间采样 | OCR 结束即回落，事后无从查证 |

启动基线 16–32 MB（`startup ws=32.1MB`，含字体预取）。

## 4. 真正的程序内热点

本次采样时未触发截图/钉图/OCR，所以这些是代码勘查结论，不是实测：

- `crates/pinshot/src/capture.rs:664` — `vec![0u8; src.len()]` BGRA→RGBA 全量拷贝。
  本机双屏合计 4160×1400，一份 ≈ 22 MB，拷贝瞬间双份 ≈ 44 MB。
  已在通道转换前后加 `capture:pre-channel-swap` / `post-channel-swap` 埋点。
- `crates/pinshot/src/ocr_run.rs:577` — `static ENGINE: OnceLock<Mutex<Option<(String, OcrLite)>>>`。
  见 §2，是 OCR 1000 MB 的主因。
- `crates/pinshot/src/pin.rs:1695` — 每钉图窗口一份全尺寸 `Pixmap`，随窗口数线性增长。
- `crates/pinshot-core/src/history.rs` — `HistoryEntry.image: Raster` 内存里存整图，
  受 `history_max = 10` 约束。

## 5. 内建内存探测

`crates/pinshot/src/logutil.rs` 新增 `MemSnapshot`，走 Win32
`GetProcessMemoryInfo` + `PROCESS_MEMORY_COUNTERS_EX`，读取
`WorkingSetSize` / `PrivateUsage` / `PeakWorkingSetSize`。

`logutil::mem(label)` 仅在 `--debug` 时输出一行：

```
[pinshot] mem startup ws=14.1MB peak_ws=14.1MB priv=3.4MB
```

**WorkingSet 排在最前**，因为 Private 会被显存预留污染。实测启动基线：`ws=14.1MB`。

埋点位置：

| 标签 | 位置 |
|---|---|
| `startup` | `logutil::init()` |
| `ocr:before-recognize` / `ocr:after-recognize` | `recognize_with_progress()` |
| `ocr:pre-load-models` / `ocr:post-load-models` | 引擎初始化前后 |
| `ocr:pre-infer` / `ocr:post-infer` | 每次推理前后 |
| `capture:pre-channel-swap` / `post-channel-swap` | DXGI 通道转换 |

非 Windows 平台返回默认值（0），保证可编译。

## 6. 复现步骤

```powershell
# 1. 启动（务必带 --debug，否则没有内存埋点）
target\release\pinshot.exe --debug

# 2. 另开终端：基线 + 采样（看 WorkingSet）
powershell -ExecutionPolicy Bypass -File tools\mem-probe.ps1 -Seconds 30

# 3. 想知道任务管理器里那 3 GB 是什么
powershell -ExecutionPolicy Bypass -File tools\mem-modules.ps1

# 4. 和全机其他进程比，判断是个案还是普遍现象
powershell -ExecutionPolicy Bypass -File tools\mem-compare.ps1
```

### 验证 OCR 开销（手动步骤）

1. 用 F7 截图，存成钉图；
2. 右键钉图 → 选择 OCR；
3. 看日志 `%APPDATA%\PinShot\pinshot.log`，找 `ocr:post-load-models` 那行，
   `ws=` 即 OCR 后的真实驻留。

对比 server / mobile 两档：改 `config.toml` 的 `ocr_model`，**重启 PinShot**，
重复上述步骤，读 `ocr:post-load-models` 的 `ws=`。

## 7. 踩过的坑

- **PS 5.1 按 ANSI/GBK 读无 BOM 的 .ps1**，中文字符串会破坏解析器并报
  `Unexpected token`。所有 PowerShell 脚本必须**纯 ASCII**。
- `VirtualQueryEx` 返回 `SIZE_T`（UIntPtr），用 `IntPtr` 声明会让 `Add-Type` 编译失败。
- `Get-Process` 读不出 Private/WorkingSet 之外的分项，需要自己遍历 `VirtualQueryEx`。
- 用户环境 PowerShell 工具 stdout 捕获失效、node spawn powershell 报 EBUSY；
  落盘的文件要用 node 读。
- **正在运行的 pinshot.exe 会锁住 `target\release\deps\pinshot.exe`，链接报 LNK1104。**
  重新编译前先关掉旧进程。
- windows 0.58 的 `GetProcessMemoryInfo` 返回 `Result`（不是 `BOOL`），
  用 `.is_ok()`；`PROCESS_MEMORY_COUNTERS_EX` 字段是
  `WorkingSetSize` / `PrivateUsage` / `PeakWorkingSetSize`，**没有 `dwFlags`**；
  第三个参数是 `u32`，`size_of::<T>()` 要 `as u32`。
- 本机沙箱会拦截 `Start-Process` 和 `cmd /c start` 启动 GUI 进程，
  需要人工手动启动被测程序。
