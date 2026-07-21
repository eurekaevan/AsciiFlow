using System;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using FFmpeg.AutoGen;
using CommandLine;
using AsciiFlow.App.Core;

namespace AsciiFlow.App;

/// <summary>
/// AsciiFlow 主程序入口
/// ASCII 视频转换器，将普通视频转换为 ASCII 风格视频
/// </summary>
class Program
{
    static async Task<int> Main(string[] args)
    {
        // ======================================================
        // 🔑 关键：在任何 FFmpeg API 调用之前必须设置路径
        // ======================================================
        SetupFFmpegRootPath();
        // ======================================================

        // 解析命令行参数
        return await Parser.Default.ParseArguments<CommandLineOptions>(args)
            .MapResult(
                async opts => await RunAsync(opts),
                errs => Task.FromResult(1));
    }

    /// <summary>
    /// 自动检测并设置 FFmpeg 动态库路径
    /// </summary>
    private static void SetupFFmpegRootPath()
    {
        try
        {
            string? resolvedPath = ResolveFFmpegPath();

            if (resolvedPath != null)
            {
                FFmpeg.AutoGen.ffmpeg.RootPath = resolvedPath;
                Console.WriteLine($"[FFmpeg] ✓ 动态库路径: {resolvedPath}");
            }
            else
            {
                // 未找到路径 —— 显示详细帮助
                Console.ForegroundColor = ConsoleColor.Yellow;
                Console.WriteLine(GetFFmpegHelpMessage());
                Console.ResetColor();
            }
        }
        catch (Exception ex)
        {
            Console.ForegroundColor = ConsoleColor.Yellow;
            Console.WriteLine($"[FFmpeg] ⚠ 路径检测异常: {ex.Message}");
            Console.ResetColor();
        }
    }

    /// <summary>
    /// 按优先级查找 FFmpeg 库路径
    /// 
    /// 优先级：
    /// 1. FFMPEG_ROOT 环境变量
    /// 2. 应用所在目录的 ffmpeg/{platform}/ 子目录（发布场景）
    /// 3. 项目根目录的 ffmpeg/{platform}/ 子目录（dotnet run 场景）
    /// 4. 当前工作目录的 ffmpeg/{platform}/ 子目录（备用）
    /// </summary>
    private static string? ResolveFFmpegPath()
    {
        // 1. 环境变量
        string? envPath = Environment.GetEnvironmentVariable("FFMPEG_ROOT");
        if (!string.IsNullOrEmpty(envPath) && Directory.Exists(envPath))
            return envPath;

        // 2. 应用目录下的 ffmpeg/{platform}/
        string appDir = AppContext.BaseDirectory;
        string? appPath = GetPlatformSpecificPath(appDir);
        if (appPath != null && Directory.Exists(appPath) && HasFFmpegDlls(appPath))
            return appPath;

        // 3. 项目根目录下的 ffmpeg/{platform}/（向上查找 .sln 或 .git）
        string? projectRoot = FindProjectRoot();
        if (projectRoot != null)
        {
            string? projPath = GetPlatformSpecificPath(projectRoot);
            if (projPath != null && Directory.Exists(projPath) && HasFFmpegDlls(projPath))
                return projPath;
        }

        // 4. 当前工作目录
        try
        {
            string cwd = Directory.GetCurrentDirectory();
            string? cwdPath = GetPlatformSpecificPath(cwd);
            if (cwdPath != null && Directory.Exists(cwdPath) && HasFFmpegDlls(cwdPath))
                return cwdPath;
        }
        catch { /* 忽略权限错误 */ }

        return null;
    }

    /// <summary>
    /// 根据当前操作系统返回对应的平台子文件夹名
    /// </summary>
    private static string? GetPlatformSpecificPath(string baseDir)
    {
        string subDir = RuntimeInformation.IsOSPlatform(OSPlatform.Windows)
            ? "windows"
            : RuntimeInformation.IsOSPlatform(OSPlatform.OSX)
                ? "macos"
                : "linux";

        return Path.Combine(baseDir, "ffmpeg", subDir);
    }

    /// <summary>
    /// 检查目录中是否有 FFmpeg 库文件
    /// </summary>
    private static bool HasFFmpegDlls(string directory)
    {
        if (!Directory.Exists(directory)) return false;

        try
        {
            string searchPattern = RuntimeInformation.IsOSPlatform(OSPlatform.Windows)
                ? "avcodec-*.dll"
                : "libavcodec.so*";

            var files = Directory.GetFiles(directory, searchPattern);
            return files.Length > 0;
        }
        catch
        {
            return false;
        }
    }

    /// <summary>
    /// 从应用目录向上查找项目根目录（包含 .git 或 .sln）
    /// </summary>
    private static string? FindProjectRoot()
    {
        try
        {
            var dir = new DirectoryInfo(AppContext.BaseDirectory);
            for (int i = 0; i < 8 && dir != null; i++, dir = dir.Parent)
            {
                bool hasGit = Directory.Exists(Path.Combine(dir.FullName, ".git"));
                bool hasSln = Directory.GetFiles(dir.FullName, "*.sln", SearchOption.TopDirectoryOnly).Length > 0;
                if (hasGit || hasSln)
                    return dir.FullName;
            }
        }
        catch { /* 忽略权限等错误 */ }

        return null;
    }

    /// <summary>
    /// 详细的 FFmpeg 库缺失帮助信息
    /// </summary>
    private static string GetFFmpegHelpMessage()
    {
        string platform = RuntimeInformation.IsOSPlatform(OSPlatform.Windows) ? "Windows"
                        : RuntimeInformation.IsOSPlatform(OSPlatform.OSX) ? "macOS"
                        : "Linux";

        string platformDir = RuntimeInformation.IsOSPlatform(OSPlatform.Windows) ? "windows"
                           : RuntimeInformation.IsOSPlatform(OSPlatform.OSX) ? "macos"
                           : "linux";

        return $@"
┌────────────────────────────────────────────────────────────────┐
│  ❌ FFmpeg 动态库未找到                                         │
├────────────────────────────────────────────────────────────────┤
│                                                                │
│  当前平台: {platform,-55} │
│  应用目录: {AppContext.BaseDirectory,-55} │
│                                                                │
│  期望目录结构:                                                  │
│    AsciiFlow/                                                  │
│    └── ffmpeg/                                                 │
│        └── {platformDir}/                                        │
│            ├─ {(RuntimeInformation.IsOSPlatform(OSPlatform.Windows) ? "avcodec-61.dll" : "libavcodec.so.*",-50)} │
│            ├─ {(RuntimeInformation.IsOSPlatform(OSPlatform.Windows) ? "avformat-61.dll" : "libavformat.so.*",-50)} │
│            ├─ {(RuntimeInformation.IsOSPlatform(OSPlatform.Windows) ? "avutil-59.dll" : "libavutil.so.*",-50)} │
│            └─ ...                                              │
│                                                                │
│  解决方式:                                                      │
│    1. 将 FFmpeg 库文件放入 ffmpeg/{platformDir,-7}/ 文件夹                   │
│    2. 设置环境变量 FFMPEG_ROOT=/path/to/ffmpeg                 │
│    3. 将 FFmpeg 库所在目录加入系统 PATH                        │
│                                                                │
│  获取 FFmpeg:                                                  │
│    Windows: https://www.gyan.dev/ffmpeg/builds/                │
│    Linux:   sudo apt install libavcodec-dev libavformat-dev ... │
│    macOS:   brew install ffmpeg                                │
│                                                                │
└────────────────────────────────────────────────────────────────┘
";
    }

    /// <summary>
    /// 主执行流程
    /// </summary>
    static async Task<int> RunAsync(CommandLineOptions options)
    {
        var stopwatch = Stopwatch.StartNew();
        var pipeline = new VideoPipeline();

        try
        {
            Console.WriteLine("╔══════════════════════════════════════════════╗");
            Console.WriteLine("║     AsciiFlow - ASCII 视频转换器 v1.0.0      ║");
            Console.WriteLine("╚══════════════════════════════════════════════╝");
            Console.WriteLine();

            // 打印配置信息
            PrintConfiguration(options);

            // 初始化流水线
            pipeline.Initialize(options);

            Console.WriteLine();
            Console.WriteLine("开始处理...");
            Console.WriteLine(new string('─', 50));

            // 处理视频
            int totalFrames = pipeline.Process(options);

            // 完成编码（使用 Finish 而不是 Finalize，避免与 object.Finalize 冲突）
            pipeline.WriteTrailer();

            stopwatch.Stop();

            // 打印性能报告
            var stats = pipeline.GetStatistics();
            PrintPerformanceReport(stats, totalFrames, stopwatch);

            Console.WriteLine();
            Console.WriteLine("╔══════════════════════════════════════════════╗");
            Console.WriteLine("║              ✅ 处理完成！                    ║");
            Console.WriteLine("╚══════════════════════════════════════════════╝");
            Console.WriteLine();
            Console.WriteLine($"输出文件: {Path.GetFullPath(options.OutputFile)}");
            
            if (File.Exists(options.OutputFile))
            {
                Console.WriteLine($"文件大小: {new FileInfo(options.OutputFile).Length / 1024.0 / 1024.0:F2} MB");
            }

            return 0;
        }
        catch (Exception ex)
        {
            Console.WriteLine();
            Console.WriteLine($"╔══════════════════════════════════════════════╗");
            Console.WriteLine($"║  ❌ 处理失败！                                ║");
            Console.WriteLine($"╚══════════════════════════════════════════════╝");
            Console.WriteLine();

            if (options.Verbose)
            {
                Console.WriteLine($"错误类型: {ex.GetType().Name}");
                Console.WriteLine($"错误信息: {ex.Message}");
                Console.WriteLine();
                Console.WriteLine("堆栈跟踪:");
                Console.WriteLine(ex.StackTrace);

                if (ex.InnerException != null)
                {
                    Console.WriteLine();
                    Console.WriteLine("内部异常:");
                    Console.WriteLine($"  类型: {ex.InnerException.GetType().Name}");
                    Console.WriteLine($"  信息: {ex.InnerException.Message}");
                }
            }
            else
            {
                Console.WriteLine($"错误信息: {ex.Message}");
                Console.WriteLine();
                Console.WriteLine("提示: 使用 --verbose 选项查看详细信息");
            }

            return 1;
        }
        finally
        {
            pipeline?.Dispose();
        }
    }

    /// <summary>
    /// 打印配置信息
    /// </summary>
    static void PrintConfiguration(CommandLineOptions options)
    {
        Console.WriteLine("【配置信息】");
        Console.WriteLine($"  输入文件: {options.InputFile}");
        Console.WriteLine($"  输出文件: {options.OutputFile}");
        Console.WriteLine($"  ASCII 尺寸: {options.Width} × {options.Height} 字符");
        Console.WriteLine($"  输出尺寸: {options.Width * 16} × {options.Height * 16} 像素");
        string fpsText = options.FrameRate > 0 ? $"{options.FrameRate} fps" : "自动（与原视频保持一致）";
        Console.WriteLine($"  帧率: {fpsText}");
        Console.WriteLine($"  字符集: {options.CharSet}");
        Console.WriteLine($"  字体: {options.FontFamily} {options.FontSize}px");

        if (options.MaxFrames > 0)
        {
            Console.WriteLine($"  最大帧数: {options.MaxFrames}");
        }

        Console.WriteLine();
        Console.WriteLine("【处理流水线】");
        Console.WriteLine("  [解码] FFmpeg → SIMD灰度 → ASCII映射 → SkiaSharp渲染 → H.264编码 [输出]");
    }

    /// <summary>
    /// 打印性能报告
    /// </summary>
    static void PrintPerformanceReport(PerformanceStats stats, int totalFrames, Stopwatch stopwatch)
    {
        Console.WriteLine();
        Console.WriteLine(new string('═', 50));
        Console.WriteLine("                    📊 性能报告");
        Console.WriteLine(new string('═', 50));
        Console.WriteLine();

        // ====== [修复] 零帧数保护 ======
        if (totalFrames == 0)
        {
            Console.WriteLine("【总览】");
            Console.WriteLine("  处理帧数: 0 帧");
            Console.WriteLine("  ⚠️  警告: 未处理任何帧");
            Console.WriteLine();
            Console.WriteLine("可能原因：");
            Console.WriteLine("  • 输入视频为空或无效");
            Console.WriteLine("  • 解码器未能读取任何帧");
            Console.WriteLine("  • --max-frames 设置为 0 且视频读取立即失败");
            return;
        }
        // =================================

        double totalSeconds = stopwatch.Elapsed.TotalSeconds;
        if (totalSeconds <= 0) totalSeconds = 0.001; // 防止除零

        Console.WriteLine($"【总览】");
        Console.WriteLine($"  处理帧数: {totalFrames} 帧");
        Console.WriteLine($"  总耗时: {totalSeconds:F2} 秒");
        Console.WriteLine($"  平均 FPS: {(totalFrames / totalSeconds):F2}");
        Console.WriteLine();

        Console.WriteLine($"【各阶段耗时】");
        Console.WriteLine($"  ├─ 解码: {stats.DecodeTimeMs / 1000.0:F3}s ({stats.DecodeTimeMs / (double)totalFrames:F2}ms/帧)");
        Console.WriteLine($"  ├─ 灰度转换: {stats.GrayscaleTimeMs / 1000.0:F3}s ({stats.GrayscaleTimeMs / (double)totalFrames:F2}ms/帧)");
        Console.WriteLine($"  ├─ ASCII映射: {stats.MappingTimeMs / 1000.0:F3}s ({stats.MappingTimeMs / (double)totalFrames:F2}ms/帧)");
        Console.WriteLine($"  ├─ 渲染: {stats.RenderTimeMs / 1000.0:F3}s ({stats.RenderTimeMs / (double)totalFrames:F2}ms/帧)");
        Console.WriteLine($"  └─ 编码: {stats.EncodeTimeMs / 1000.0:F3}s ({stats.EncodeTimeMs / (double)totalFrames:F2}ms/帧)");
        Console.WriteLine();

        double avgFrameTime = (stats.DecodeTimeMs + stats.GrayscaleTimeMs +
                               stats.MappingTimeMs + stats.RenderTimeMs + stats.EncodeTimeMs)
                              / (double)totalFrames;
        double theoreticalFps = 1000.0 / avgFrameTime;

        Console.WriteLine($"【性能评估】");
        Console.WriteLine($"  单帧平均耗时: {avgFrameTime:F2}ms");
        Console.WriteLine($"  理论 FPS: {theoreticalFps:F1}");
        Console.WriteLine();

        if (avgFrameTime <= 30)
        {
            Console.WriteLine("  ✅ 性能优异！可以轻松处理 1080p30");
        }
        else if (avgFrameTime <= 50)
        {
            Console.WriteLine("  ✅ 性能良好！可以流畅处理 720p30");
        }
        else if (avgFrameTime <= 100)
        {
            Console.WriteLine("  ⚠️  性能一般！建议降低 ASCII 分辨率");
        }
        else
        {
            Console.WriteLine("  ❌ 性能不足！建议:");
            Console.WriteLine("     • 降低 ASCII 分辨率（如 40x20）");
            Console.WriteLine("     • 降低输出帧率（如 15fps）");
            Console.WriteLine("     • 检查 CPU 性能和 FFmpeg 编解码器");
        }
    }
}