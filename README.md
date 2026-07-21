# 🌊 AsciiFlow - 高性能全彩 ASCII 视频转换器

<p align="center">
  <b>基于 .NET 10 & FFmpeg & SkiaSharp 的工业级全彩字符视频处理引擎</b>
</p>

---

## ✨ 核心特性

- 🎨 **原视频全彩渲染（Color ASCII）**
  支持字符级 RGB 色彩采样与透明度 Alpha 遮罩混合，完美还原原视频的丰富色彩。
- 👁️ **人眼感知最佳灰度映射（BT.709 + S-Curve）**
  遵循 ITU-R BT.709 亮度标准，结合 S 曲线 (S-Curve) 伽马对比度增强，明暗细节清晰，高对比度不失真。
- 🎵 **原声音频无损透传（Audio Stream Copy）**
  基于 FFmpeg 流复用（Remuxing）技术，将原视频音轨无损复制并自动挂载至输出 MP4 容器，零音质损耗，音画完全同步。
- ⚡ **极致处理性能（80+ FPS @ 1080p）**
  采用底层 C# `unsafe` 内存指针、行级 `Parallel.For` 多线程并行计算与预渲染字符位图缓存，无高频 GC 内存分配。
- 🐧 **全平台字体兼容（Cross-Platform Font Fallback）**
  内置 Windows / Linux / macOS 自动字体选择降级机制（Consolas ➔ Cascadia Mono ➔ DejaVu Sans Mono ➔ Liberation Mono ➔ Monospace），防止跨平台全黑画面。
- ⏱️ **智能帧率匹配（Auto Frame Rate）**
  默认自动检测并继承原视频的实际帧率（如 24 / 25 / 30 / 60 FPS），确保输出视频播放流畅。

---

## 🛠️ 系统流水线

```mermaid
flowchart LR
    A[📹 输入视频] --> B[1. FFmpeg 解码 RGB24]
    B --> C[2. SIMD/并行 灰度转换]
    C --> D[3. S-Curve ASCII & 颜色映射]
    D --> E[4. SkiaSharp 字符多线程渲染]
    E --> F[5. H.264 编码 + 音轨透传]
    F --> G[🎬 最终 ASCII 视频]
```

---

## 🚀 快速开始

### 依赖环境
- **[.NET 10.0 SDK](https://dotnet.microsoft.com/)** 或更高版本
- **FFmpeg 8.x** 动态链接库（已内置 Linux 64位库）

### 构建项目

```bash
# 克隆或进入项目目录
cd AsciiFlow

# 编译项目
dotnet build AsciiFlow.slnx -c Release
```

### 基础使用

```bash
# 最简转换（自动匹配帧率与原视频颜色）
dotnet run --project src/AsciiFlow.App -- -i input.mp4 -o output.mp4

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
| `-i` | `--input` | **必填** | 输入视频文件路径 |
| `-o` | `--output` | `output/output_ascii.mp4` | 输出视频文件路径 |
| `-w` | `--width` | `160` | ASCII 字符画宽度（字符数） |
| `-h` | `--height` | `90` | ASCII 字符画高度（字符数） |
| `-f` | `--framerate` | `0.0` (自动) | 输出视频帧率（`0` 表示自动与原视频一致） |
| `-C` | `--color` | `true` | 是否启用彩色模式 (`true` / `false`) |
| `-c` | `--charset` | `standard` | 字符集选用：`standard`(69字符) 或 `detailed`(25字符) |
| `-f` | `--font-family` | `Consolas` | 渲染字体族名称（跨平台自动回退） |
| `-s` | `--font-size` | `12` | 渲染字体大小 (px) |
| `-m` | `--max-frames` | `0` | 最大转换帧数（`0` 表示转换全片） |
| | `--no-progress` | `false` | 禁用控制台进度条显示 |

---

## 📊 性能表现

在 Linux 1080p 视频转 ASCII 视频的测试报告：

| 处理阶段 | 单帧平均耗时 | 耗时占比 | 优化技术 |
| :--- | :--- | :--- | :--- |
| **视频解码 (FFmpeg)** | 3.22 ms | 26.1% | 循环帧缓冲 + 原生硬件 API |
| **灰度转换 (BT.709)** | 1.24 ms | 10.1% | `unsafe` 指针 + `Parallel.For` 并行 |
| **ASCII/颜色映射** | 1.68 ms | 13.6% | $O(1)$ 查找表 + S-Curve 梯度 |
| **字符渲染 (SkiaSharp)** | 1.54 ms | 12.5% | 预渲染字符遮罩 + 多线程 Alpha 混合 |
| **视频编码 (H.264)** | 4.64 ms | 37.7% | libx264 快速容器封包 + 音轨透传 |
| **🚀 综合评估** | **12.32 ms / 帧** | **100%** | **单机处理速度 81.2 FPS** |

---

## 📁 项目结构

```text
AsciiFlow/
├── src/
│   ├── AsciiFlow.App/             # 命令行应用层 (CLI & Pipeline Orchestration)
│   └── AsciiFlow.Core/            # 核心领域逻辑库
│       ├── Video/                 # FFmpeg 解码器与音轨处理
│       ├── Processing/            # SIMD & 多线程灰度转换
│       ├── AsciiMapping/          # 灰度到 ASCII 字符与颜色映射器
│       ├── Rendering/             # SkiaSharp 字符位图渲染引擎
│       └── Encoding/              # FFmpeg H.264 编码器
├── ffmpeg/                        # FFmpeg 原生动态库
├── output/                        # 默认输出目录
└── README.md                      # 项目说明文档
```

---

## 📄 开源协议

本项目基于 [MIT License](LICENSE) 协议开源。
