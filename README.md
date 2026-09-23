# 🌊 AsciiFlow v2 — Rust CPU + Vulkan pipeline

Input is detected automatically: H.264 8-bit 4:2:0, HEVC Main 8-bit 4:2:0,
and AV1 Main 8-bit 4:2:0. Output defaults to H.264; HEVC Main and AV1 Profile0
8-bit output are available through VAAPI. Production ten-bit HEVC/AV1
transcoding and HDR are not supported. Internal software 10-bit SDR decoding
to P010LE is under qualification; Intel P010 VAAPI interop is not yet qualified.
Software decode and Intel Arc Meteor Lake hardware decode/interop
are qualified for these 8-bit inputs. See [codec support](docs/codecs.md).

AsciiFlow v2 is the primary development path. Host NV12 remains the portable
production boundary. An internal P010LE processing foundation now exists and
can consume software-decoded 10-bit SDR HEVC/AV1; the CLI still rejects real
10-bit output because no 10-bit encoder path is available.
See [P010 processing contract](docs/p010.md). At startup, AsciiFlow probes the
input and the local runtime, then selects the fastest legal pipeline. On the
qualified Intel Linux path,
this can carry decoded VAAPI surfaces into Vulkan and return processed pixels
to encoder-owned VAAPI surfaces without Host pixel copies:

```text
FFmpeg software or VAAPI decode
  -> hardware download to Host NV12, or DRM PRIME / DMA-BUF import
  -> CPU ASCII, or Vulkan mapping pass + render pass
  -> Host NV12 + optional hardware upload, or writable encoder DMA-BUF import
  -> FFmpeg software H.264, or VAAPI H.264/HEVC Main/AV1 Profile0 MP4 encode

FFmpeg demux -> compatible compressed audio packets -> the same MP4 mux owner
```

The existing .NET implementation remains in `src/` and `tests/` as the behavior
and performance reference until the Rust baseline has broader regression
coverage. It has not been deleted or mechanically translated.

## Rust quick start

Fedora development dependencies include Rust, `clang`/`libclang`, FFmpeg
development headers, `libva`/`libdrm` headers, and an FFmpeg build containing
`libx264`, VAAPI, and libdrm. H.264 VAAPI additionally needs a driver build
that exposes the patented H.264 profiles; Fedora's free Intel driver omits
them.

```bash
cargo build --workspace
cargo test --workspace
cargo run --release --bin asciiflow -- input.mp4 output.mp4 \
  --width 160 --max-frames 100 --verbose

# Inspect the runtime capability snapshot and selected plan without creating output.
cargo run --release --bin asciiflow -- input.mp4 --capabilities
cargo run --release --bin asciiflow -- input.mp4 --explain-plan

# Explicit Stage 2 hardware media; never falls back to software.
cargo run --release --bin asciiflow -- input.mp4 output-vaapi.mp4 \
  --backend vulkan --decode vaapi --encode vaapi \
  --input-interop off --output-interop off \
  --hw-device /dev/dri/renderD128 --width 160 --verbose

# Explicit Stage 3A zero-host-copy input. This is strict and Intel-specific.
cargo run --release --bin asciiflow -- input.mp4 output-interop.mp4 \
  --backend vulkan --decode vaapi --encode software \
  --vaapi-vulkan-input-interop on --hw-device /dev/dri/renderD128 --width 80 --verbose

# Explicit Stage 3B GPU-resident pixel path into h264_vaapi. Both directions are strict.
cargo run --release --bin asciiflow -- input.mp4 output-full-interop.mp4 \
  --backend vulkan --decode vaapi --encode vaapi \
  --vaapi-vulkan-input-interop on --vaapi-vulkan-output-interop on \
  --hw-device /dev/dri/renderD128 --width 80 --verbose

# HEVC Main output; codec and encoder backend are independent choices.
# H.264 remains the default. HEVC output currently requires VAAPI.
cargo run --release --bin asciiflow -- input.mp4 output-hevc.mp4 \
  --output-codec hevc --encode auto --width 80 --verbose

# AV1 Profile0 output uses VAAPI hardware encode; H.264 remains the default.
cargo run --release --bin asciiflow -- input.mp4 output-av1.mp4 \
  --output-codec av1 --encode auto --width 80 --verbose
```

Supported options are positional `input` and optional `output` (output is not
needed for `--capabilities` or `--explain-plan`), plus
`--backend auto|cpu|vulkan`, `--vulkan-mapping auto|gpu|cpu`,
`--decode auto|software|vaapi`, `--encode auto|software|vaapi`,
`--output-codec h264|hevc|av1` (default `h264`),
`--audio auto|copy|none`, optional
`--hw-device`, `--vaapi-vulkan-input-interop auto|off|on` (also
`--input-interop`), `--vaapi-vulkan-output-interop auto|off|on` (also
`--output-interop`), `--capabilities`, `--explain-plan`, `--width`,
`--height`, `--charset`, `--font`, `--color`, `--max-frames`, `--no-progress`,
and `--verbose`. Output is MP4 with H.264, HEVC or AV1 video. HEVC Main and
AV1 Profile0 output are 8-bit 4:2:0 and VAAPI-only; neither silently changes
codec or falls back to software encoding.
Audio defaults to `auto`, which
copies every MP4-compatible compressed audio stream without decoding it and
warns when an incompatible stream is skipped. `--audio copy` is strict;
`--audio none` deliberately drops audio. Video `auto` selects from
the probed capability graph: it prefers qualified Vulkan processing and full
interop, then staged hardware encode, then software media, and finally CPU
processing. It does not prefer VAAPI decode followed by `hwdownload` when input
interop is unavailable. Explicit `--decode vaapi`, `--encode vaapi`,
`--backend vulkan`, or interop `on` requests fail instead of silently falling
back; `auto` is the only mode allowed to degrade. `--verbose` prints the
selected video and audio plans and startup probe/planning timings;
`--capabilities` prints the
facts and reasons; `--explain-plan` also lists rejected candidates and exits
before creating an output file. Audio transcoding, VP9/WebM,
cross-API asynchronous fences, and direct external-image shaders remain out of
scope. GPU mapping uses two bounded in-flight slots by default; CPU mapping is
an explicit hybrid diagnostic and remains single-slot.

Use a scalable monospaced font file with `--font` (optional collection index:
`--font-face-index 0`):

```bash
asciiflow input.mp4 output.mp4 --font /path/to/mono.ttf
```

Without `--font`, the portable built-in 8×8 font remains unchanged. Explicit
invalid or proportional fonts fail; there is no font fallback or shaping.
Builds require system FreeType development files via pkg-config. See
[fonts.md](docs/fonts.md) for geometry and supported-font details.

Audio passthrough currently requires video timestamps compatible with the
existing CFR output. `--max-frames` limits video only; copied audio retains its
full timeline. See [audio behavior and limitations](docs/audio.md).

For the pinned FFmpeg 9.0.1 source build and dynamic-link setup, see
[`docs/v2-architecture.md`](docs/v2-architecture.md) and
`third_party/ffmpeg/`. The Rust binding is raw `ffmpeg-sys-next`; native calls
and pointers do not enter Core or the CLI; the interop crate contains the only
narrow unsafe bridge from an opaque retained VAAPI frame to FFmpeg DRM PRIME.

## Rust workspace

```text
apps/asciiflow-cli/       CLI and composition root
crates/asciiflow-core/    frames, traits, planner, bounded pipeline, metrics
crates/asciiflow-media/   safe FFmpeg-facing API and internal unsafe wrappers
crates/asciiflow-cpu/     permanent CPU reference backend
crates/asciiflow-font/    cached R8 glyph atlas
crates/asciiflow-interop/ FFmpeg DRM PRIME + Vulkan DMA-BUF composition
crates/asciiflow-vulkan/  Vulkan 1.3 compute backend and resource ownership
shaders/src/              build-time GLSL compute shader sources
third_party/ffmpeg/       pinned native source/build recipe
docs/v2-architecture.md   v2 invariants, ownership, and limitations
docs/stage1-validation.md Stage 1 parity, media, and benchmark evidence
docs/stage2-validation.md Stage 2 VAAPI correctness and benchmark evidence
docs/stage3a-validation.md DMA-BUF input interop evidence and benchmark
docs/stage3b-validation.md DMA-BUF output interop evidence and benchmark
docs/auto-planner.md       Stage 4.0 capability graph and selection policy
docs/failure-semantics.md  Stage 4.1 failure, cancellation, and output contract
docs/audio.md              Stage 4.2 compressed-audio passthrough contract
docs/fonts.md              Stage 4.3 font/atlas contract
docs/codecs.md             Current input/output codec support and limits
docs/testing.md            Portable and opt-in hardware regression tests
docs/stage5.1b-av1-encode-validation.md  Latest Intel output qualification
docs/p010.md               Internal P010LE processing contract and validation
```

---

# AsciiFlow v1 — .NET reference implementation

<p align="center">
  <b>基于 .NET 10、FFmpeg 与 SkiaSharp 的全彩字符视频处理工具</b>
</p>

---

## ✨ 核心特性

- 🎨 **原视频全彩渲染（Color ASCII）**
  支持字符级 RGB 色彩采样与 Alpha 遮罩混合，在保留 ASCII 风格的同时还原主要色彩。
- 👁️ **人眼感知最佳灰度映射（BT.709 + S-Curve）**
  遵循 ITU-R BT.709 亮度标准，结合 S 曲线 (S-Curve) 伽马对比度增强，明暗细节清晰，高对比度不失真。
- 🎞️ **多媒体格式支持**
  输入由 FFmpeg 按内容自动探测，可读取 MP4、MKV、MOV、AVI、WebM、FLV、WMV、M4V、MPEG-TS 等包含视频流的媒体文件。
- 📦 **多容器输出**
  支持 MP4、M4V、MOV、MKV、AVI、MPEG-TS 和 WebM；WebM 自动使用 VP9，其他容器使用 H.264。
- 🎵 **原声音频无损透传（Audio Stream Copy）**
  音频编码与目标容器兼容时复制原始音频包；不兼容时安全忽略音轨并在详细日志中说明。
- ⚡ **三槽有界处理流水线**
  解码、映射/渲染与编码三阶段重叠执行；三个循环复用的帧槽控制内存上界。渲染器直接写入 YUV420P，不再生成整帧 RGB24 中间图像；单帧内融合灰度计算、字符映射和颜色采样。
- 🎞️ **经过样本验证的编码模式**
  默认 `speed` 使用 `libx264 ultrafast / CRF 20 / 0 B 帧`；也可选择较小文件的 `balanced` 或兼容旧参数的 `quality`。
- 🐧 **全平台字体兼容（Cross-Platform Font Fallback）**
  内置 Windows / Linux / macOS 自动字体选择降级机制（Consolas ➔ Cascadia Mono ➔ DejaVu Sans Mono ➔ Liberation Mono ➔ Monospace），防止跨平台全黑画面。
- ⏱️ **智能帧率匹配（Auto Frame Rate）**
  使用有理数保留 `30000/1001`、`24000/1001` 等小数帧率，避免整数截断造成的累计漂移。
- 📈 **可信的进度语义**
  优先使用媒体提供的总帧数；缺少该元数据时，以时长和平均帧率显示明确标注为“约”的估算进度，完成摘要始终报告实际处理帧数。
- 🛡️ **安全输出**
  先写入同目录临时文件，成功完成容器尾部后再替换目标；失败或取消不会破坏旧输出。

---

## 🛠️ 系统流水线

```mermaid
flowchart LR
    A[📹 输入视频] --> B[1. FFmpeg 解码 RGB24]
    B -->|复用帧槽| C[2. BT.709 / 颜色采样 / S-Curve ASCII 融合映射]
    C --> D[3. 字符多线程直接渲染 YUV420P]
    D -->|复用帧槽| E[4. H.264 / VP9 直接编码 + 兼容音轨透传]
    E --> F[🎬 最终 ASCII 视频]
```

流水线的职责边界、并发与内存上界、帧率/帧数语义以及安全输出流程，详见
[架构说明](docs/architecture.md)。

---

## 🚀 快速开始

### 依赖环境

- **[.NET 10.0 SDK](https://dotnet.microsoft.com/)**；项目目标框架为 `net10.0`
- 与 `FFmpeg.AutoGen 9.0.1` 匹配的 **FFmpeg 9 共享库**
- FFmpeg 构建必须包含 `libx264`；若要输出 WebM，还必须包含 `libvpx-vp9`

仓库中的 Linux x64 库可用于当前开发布局。Windows 和 macOS 运行时需要自行提供
对应平台、对应 ABI 且包含所需编码器的共享库。AsciiFlow 通过 FFmpeg.AutoGen
在进程内加载 `libav*`，不是调用系统中的 `ffmpeg` 命令行程序；只有安装
`ffmpeg` 可执行文件并不保证应用能够启动或编码。

### 构建项目

```bash
# 克隆或进入项目目录
cd AsciiFlow

# 编译项目
dotnet build AsciiFlow.slnx -c Release

# 运行单元测试
dotnet test AsciiFlow.slnx -c Release
```

### 配置 FFmpeg 共享库

应用按以下顺序查找原生库：

1. `FFMPEG_ROOT` 指向的目录；
2. 应用目录下的 `ffmpeg/{linux|windows|macos}/`；
3. 项目根目录下的同名平台目录；
4. 当前工作目录下的同名平台目录；
5. 最后由操作系统的动态链接器按其默认搜索规则解析。

`FFMPEG_ROOT` 必须直接指向包含 `libavcodec`、`libavformat`、`libavutil` 和
`libswscale` 等共享库的目录，而不是 `ffmpeg` 可执行文件。示例：

```bash
FFMPEG_ROOT=/opt/ffmpeg-9/lib \
  dotnet run --project src/AsciiFlow.App -- -i input.mp4 -o output.mp4
```

不要混用不同 FFmpeg 主版本的库，也不要只替换单个 `libav*` 文件。绑定、所有
相互依赖的共享库及编码器插件必须来自同一套构建；可用 `--verbose` 查看实际解析到
的库目录、输出容器、编码器与音轨处理结果。

### 基础使用

```bash
# 最简转换（自动匹配帧率与原视频颜色）
dotnet run --project src/AsciiFlow.App -- -i input.mp4 -o output.mp4

# MKV 输入并输出 MOV
dotnet run --project src/AsciiFlow.App -- -i input.mkv -o output.mov

# WebM / VP9 输出
dotnet run --project src/AsciiFlow.App -- -i input.avi -o output.webm

# 指定字符网格分辨率 (如 160×90 字符)
dotnet run --project src/AsciiFlow.App -- -i input.mp4 -o output.mp4 -w 160 -h 90

# 生成经典黑白 ASCII 视频
dotnet run --project src/AsciiFlow.App -- -i input.mp4 -o output.mp4 --color false

# 测试预览模式（仅转换前 100 帧）
dotnet run --project src/AsciiFlow.App -- -i input.mp4 -o output.mp4 --max-frames 100
```

---

## 📖 命令行参数说明

| 短参数 | 长参数 | 默认值 | 说明 |
| :--- | :--- | :--- | :--- |
| `-i` | `--input` | **必填** | 输入媒体文件，格式由 FFmpeg 自动探测 |
| `-o` | `--output` | `output/output_ascii.mp4` | 输出文件；支持 `mp4/m4v/mov/mkv/avi/ts/m2ts/webm` |
| `-w` | `--width` | `240` | ASCII 字符画宽度（默认 `240` 字符超高清） |
| `-h` | `--height` | `0` (自动) | ASCII 字符画高度（`0` 表示根据原视频比例自动计算，16:9 对应 `135`） |
| `-f` | `--framerate` | `0.0` (自动) | 输出视频帧率（`0` 表示自动与原视频一致） |
| `-C` | `--color` | `true` | 是否启用彩色模式 (`true` / `false`) |
| `-c` | `--charset` | `standard` | 字符集选用：`standard`（70 字符）或 `detailed`（16 字符） |
| | `--font-family` | `Consolas` | 渲染字体族名称（跨平台自动回退） |
| | `--font-size` | `12` | 渲染字体大小 (px) |
| | `--max-frames` | `0` | 最大转换帧数（`0` 表示转换全片） |
| | `--encoder-mode` | `speed` | 编码模式：`speed`、`balanced` 或 `quality` |
| | `--no-progress` | `false` | 禁用动态进度条（输出重定向时会自动禁用） |
| `-v` | `--verbose` | `false` | 显示编码、渲染、性能明细和完整错误诊断 |

参数边界：宽度为 `1..8192`，高度为 `0..8192`，帧率为 `0..1000`，字体大小
为 `(0, 512]`，`--max-frames` 不能为负数；ASCII 网格最多 400 万个单元格。
当请求的网格宽高超过源视频时会自动收缩，输出像素尺寸则保持源视频尺寸（奇数边长
会补到相邻偶数，以满足 YUV420P 编码要求）。

---

## 🧭 运行行为说明

- 输入与输出不能是同一路径。输出先写入目标目录中的临时文件，只有编码和容器收尾全部成功后才替换目标文件。
- 音频仅在源编码与目标容器确认兼容时原样透传；例如 WebM 不接受 AAC 时会保留视频转换结果并忽略不兼容音轨，`--verbose` 会显示判断结果。
- 源媒体没有 `nb_frames` 时，总帧数保持“未知”。界面只使用 `时长 × 平均帧率` 生成标有“约”的进度参考，并将其最高限制为 `99.9%`；完成摘要使用实际解码并编码的帧数。
- `--max-frames` 是处理上限，适合快速预览。使用 `--no-progress` 可关闭动态进度；标准输出被重定向时也会自动关闭。
- 按 `Ctrl+C` 会请求取消并返回退出码 `130`；参数解析失败返回 `2`，运行或编码失败返回 `1`，成功返回 `0`。
- 取消或任一阶段失败都只会删除临时输出；已有目标文件保持逐字节不变，不会提交半成品 MP4。
- 显式请求 VAAPI、Vulkan 或硬件互操作时，能力或初始化失败会直接报错；只有 `auto` 可在开始处理前降级，运行中绝不切换路径。

---

## 📊 性能测量

默认终端只显示输入、输出、核心配置、源视频信息、进度和完成摘要。使用 `--verbose` 时，额外显示容器、编码器、音轨状态以及各处理阶段的实际耗时。

编码模式参数：

| 模式 | H.264 / libx264 | WebM / VP9 | 适用场景 |
| :--- | :--- | :--- | :--- |
| `speed`（默认） | `ultrafast / CRF 20 / 0 B 帧` | `realtime / cpu-used 8 / CRF 20` | 最高吞吐与低编码延迟，可接受更大文件 |
| `balanced` | `superfast / CRF 20` | `good / cpu-used 6 / CRF 20` | 平衡速度与文件体积 |
| `quality` | `fast / CRF 23 / tune fastdecode` | `good / cpu-used 4 / CRF 23` | 更注重压缩效率 |

WebM/VP9 会根据输出分辨率和可用 CPU 自动设置编码线程与 tile 分块；`speed` 模式同时关闭 lookahead，减少短视频和预览转换的收尾等待。

在 120 帧、1920×1080 彩色 ASCII 样本上，默认模式的流水线由旧参数的 72.6 FPS 提升至 92.4 FPS；H.264 子阶段由约 1.99 ms/帧降至 0.96 ms/帧。相同样本的隔离编码测试中，默认模式相对旧参数的综合 SSIM 为 0.9911（旧参数对照为 0.9919），PSNR 为 41.76 dB（旧参数对照为 40.20 dB）。默认模式样本文件为 10.62 MB，`balanced` 为 7.99 MB，`quality` 为 4.66 MB。测试结果取决于源编码、分辨率、ASCII 网格、字体、CPU 和 FFmpeg 构建，请在目标机器上使用同一输入进行比较，不把单台机器的结果视为通用保证。

---

## 📁 项目结构

```text
AsciiFlow/
├── src/
│   ├── AsciiFlow.App/             # 命令行应用层 (CLI & Pipeline Orchestration)
│   └── AsciiFlow.Core/            # 核心领域逻辑库
│       ├── Video/                 # FFmpeg 解码器与音轨处理
│       ├── Processing/            # 可独立使用的灰度转换组件
│       ├── AsciiMapping/          # 灰度到 ASCII 字符与颜色映射器
│       ├── Rendering/             # SkiaSharp 字符位图渲染引擎
│       └── Encoding/              # FFmpeg 容器选择与 H.264/VP9 编码
├── tests/
│   └── AsciiFlow.Core.Tests/      # 核心逻辑、CLI、终端输出与并发流水线测试
├── ffmpeg/                        # FFmpeg 原生动态库
├── output/                        # 默认输出目录
└── README.md                      # 项目说明文档
```

提交改动前可运行与 CI 一致的检查：

```bash
dotnet format AsciiFlow.slnx --verify-no-changes --no-restore
dotnet build AsciiFlow.slnx -c Release --no-restore
dotnet test AsciiFlow.slnx -c Release --no-build --no-restore
```

其中 CI 会执行还原、Release 构建和测试；`dotnet format` 是提交前的本地附加检查。

---

## 📄 开源协议

AsciiFlow 自有源码基于 [MIT License](LICENSE) 开源。NuGet 依赖和 FFmpeg 原生库
保留各自的许可证；仓库的 MIT 许可证不会覆盖这些第三方组件。当前 H.264 输出依赖
`libx264`，分发应用或原生库前，请根据实际 FFmpeg 构建配置核对并随发行物提供相应
许可证、声明及源码提供方式。本文不是法律意见。
