using System.Diagnostics;
using AsciiFlow.Core.Rendering;
using System.Text;

namespace AsciiFlow.App;

class Program
{
    static void Main(string[] args)
    {
        Console.WriteLine("=== 阶段 5: SkiaSharp ASCII 渲染器测试 ===\n");

        // 测试参数
        const int TARGET_WIDTH = 80;   // 目标字符宽度
        const int TARGET_HEIGHT = 40;  // 目标字符高度

        Console.WriteLine($"配置: {TARGET_WIDTH}x{TARGET_HEIGHT} 字符");
        Console.WriteLine($"字体: Consolas, 16px");
        Console.WriteLine($"输出尺寸: {TARGET_WIDTH * 12}x{TARGET_HEIGHT * 20} 像素");
        Console.WriteLine($"目标性能: ≤ 0.5ms/帧\n");


        // 测试 2：字符缓存版本（高性能）
        TestCachedRenderer(TARGET_WIDTH, TARGET_HEIGHT);

        // 显示示例输出前 5 行的前 10 字符
        Console.WriteLine("=== 示例输出预览（前 5 行） ===\n");
        ShowSampleOutput(TARGET_WIDTH, TARGET_HEIGHT);

        Console.WriteLine("=== 阶段 5 测试完成 ===");
    }

    /// <summary>
    /// 测试字符缓存渲染器性能
    /// </summary>
    static void TestCachedRenderer(int targetWidth, int targetHeight)
    {
        Console.WriteLine("【SkiaCachedAsciiRenderer - 字符缓存版本（推荐）】");

        try
        {
            // 创建渲染器
            using var renderer = new SkiaCachedAsciiRenderer(
                CharacterSetConfig.Default,
                targetWidth,
                targetHeight);

            // 初始化（预渲染字符）
            renderer.Initialize();

            Console.WriteLine($"输出尺寸: {renderer.OutputWidth}x{renderer.OutputHeight} 像素");

            // 生成测试 ASCII 字符串
            string testAscii = GenerateTestAscii(targetWidth, targetHeight);

            // 预热
            for (int i = 0; i < 5; i++)
                renderer.RenderFrame(testAscii);

            // 性能测试
            const int ITERATIONS = 100;
            var sw = Stopwatch.StartNew();

            for (int i = 0; i < ITERATIONS; i++)
            {
                byte[] result = renderer.RenderFrame(testAscii);
                Console.WriteLine($"  帧 {i+1}: {result.Length:N0} RGB24 字节");
            }

            sw.Stop();
            double avgTimeMs = sw.Elapsed.TotalMilliseconds / ITERATIONS;
            double fps = 1000.0 / avgTimeMs;

            Console.WriteLine($"平均渲染时间: {avgTimeMs:F3} ms");
            Console.WriteLine($"性能: {fps:N0} FPS ({avgTimeMs:F2}ms/帧)");
            Console.WriteLine($"目标: ≤ 0.5ms/帧 (≥2000 FPS)");

            // 性能评估
            if (avgTimeMs <= 0.5)
                Console.WriteLine("✅ 性能达标！");
            else if (avgTimeMs <= 1.0)
                Console.WriteLine("⚠️ 性能接近目标，可接受");
            else
                Console.WriteLine("❌ 性能不达标，需要优化");

            Console.WriteLine();
        }
        catch (Exception ex)
        {
            Console.WriteLine($"❌ 测试失败: {ex.Message}");
            Console.WriteLine($"   详细信息: {ex.GetType().Name}");
            Console.WriteLine();
        }
    }

    /// <summary>
    /// 生成测试用 ASCII 字符串（渐变效果）
    /// </summary>
    static string GenerateTestAscii(int width, int height)
    {
        // 从 ASCII 字符集 $@B%8&WM#*oahkbdpqwmZO0QLCJUYXzcvunxrjft/\\|()1{}[]?-_+~<>i!lI;:,"^`'.
        string charset = "$@B%8&WM#*oahkbdpqwmZO0QLCJUYXzcvunxrjft/\\|()1{}[]?-_+~<>i!lI;:,\"^`'. ";

        var sb = new StringBuilder();
        for (int y = 0; y < height; y++)
        {
            for (int x = 0; x < width; x++)
            {
                // 创建渐变：从左上到右下
                int idx = (x + y) % charset.Length;
                sb.Append(charset[idx]);
            }
            sb.AppendLine();
        }
        return sb.ToString();
    }

    /// <summary>
    /// 显示示例输出
    /// </summary>
    static void ShowSampleOutput(int targetWidth, int targetHeight)
    {
        string testAscii = GenerateTestAscii(targetWidth, Math.Min(5, targetHeight));
        var lines = testAscii.Split('\n');

        for (int i = 0; i < lines.Length && i < 5; i++)
        {
            // 只显示前 20 字符
            string line = lines[i];
            if (line.Length > 20)
                line = line[..20] + "...";
            Console.WriteLine(line);
        }

        Console.WriteLine();
    }
}