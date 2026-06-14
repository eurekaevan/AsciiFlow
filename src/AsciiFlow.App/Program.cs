using System.Diagnostics;
using AsciiFlow.App.Core;
using AsciiFlow.Core.Video;
using AsciiFlow.Core.Processing;
using AsciiFlow.Core.AsciiMapping;
using AsciiFlow.Core.Rendering;
using AsciiFlow.Core.Encoding;
using CommandLine;

namespace AsciiFlow.App;

class Program
{
    static async Task<int> Main(string[] args)
    {
        // 解析命令行参数
        return await Parser.Default.ParseArguments<CommandLineOptions>(args)
            .MapResult(
                async opts => await RunAsync(opts),
                errs => Task.FromResult(1));
    }

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

            // 完成编码
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
            Console.WriteLine($"文件大小: {new FileInfo(options.OutputFile).Length / 1024.0 / 1024.0:F2} MB");

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

    static void PrintConfiguration(CommandLineOptions options)
    {
        Console.WriteLine("【配置信息】");
        Console.WriteLine($"  输入文件: {options.InputFile}");
        Console.WriteLine($"  输出文件: {options.OutputFile}");
        Console.WriteLine($"  ASCII 尺寸: {options.Width} × {options.Height} 字符");
        Console.WriteLine($"  输出尺寸: {options.Width * 12} × {options.Height * 20} 像素");
        Console.WriteLine($"  帧率: {options.FrameRate} fps");
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

    static void PrintPerformanceReport(PerformanceStats stats, int totalFrames, Stopwatch stopwatch)
    {
        Console.WriteLine();
        Console.WriteLine(new string('═', 50));
        Console.WriteLine("                    📊 性能报告");
        Console.WriteLine(new string('═', 50));
        Console.WriteLine();

        // 统计信息
        // var stats = pipeline.GetStatistics();

        Console.WriteLine($"【总览】");
        Console.WriteLine($"  处理帧数: {totalFrames} 帧");
        Console.WriteLine($"  总耗时: {stopwatch.Elapsed.TotalSeconds:F2} 秒");
        Console.WriteLine($"  平均 FPS: {(totalFrames / stopwatch.Elapsed.TotalSeconds):F2}");
        Console.WriteLine();

        Console.WriteLine($"【各阶段耗时】");
        Console.WriteLine($"  ├─ 解码: {stats.DecodeTimeMs / 1000.0:F3}s ({stats.DecodeTimeMs / (double)totalFrames:F2}ms/帧)");
        Console.WriteLine($"  ├─ 灰度转换: {stats.GrayscaleTimeMs / 1000.0:F3}s ({stats.GrayscaleTimeMs / (double)totalFrames:F2}ms/帧)");
        Console.WriteLine($"  ├─ ASCII映射: {stats.MappingTimeMs / 1000.0:F3}s ({stats.MappingTimeMs / (double)totalFrames:F2}ms/帧)");
        Console.WriteLine($"  ├─ 渲染: {stats.RenderTimeMs / 1000.0:F3}s ({stats.RenderTimeMs / (double)totalFrames:F2}ms/帧)");
        Console.WriteLine($"  └─ 编码: {stats.EncodeTimeMs / 1000.0:F3}s ({stats.EncodeTimeMs / (double)totalFrames:F2}ms/帧)");
        Console.WriteLine();

        double avgFrameTime = (stats.DecodeTimeMs + stats.GrayscaleTimeMs + 
                               stats.MappingTimeMs + stats.RenderTimeMs + stats.EncodeTimeMs) / totalFrames;
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