using System.Diagnostics;
using AsciiFlow.Core.AsciiMapping;
using AsciiFlow.Core.Encoding;
using AsciiFlow.Core.Processing;
using AsciiFlow.Core.Rendering;
using AsciiFlow.Core.Video;

namespace AsciiFlow.App.Core;

/// <summary>
/// 视频处理流水线管理器
/// 整合：解码 → 灰度 → 映射 → 渲染 → 编码
/// </summary>
public class VideoPipeline : IDisposable
{
    // 各模块实例
    private IVideoDecoder _decoder = null!;
    private IGrayscaleConverter _grayscaleConverter = null!;
    private IAsciiMapper _asciiMapper = null!;
    private IAsciiRenderer _renderer = null!;
    private IVideoEncoder _encoder = null!;

    // 配置信息
    private int _asciiWidth;
    private int _asciiHeight;
    private int _videoWidth;
    private int _videoHeight;

    // 性能统计（毫秒）
    private long _decodeTimeMs;
    private long _grayscaleTimeMs;
    private long _mappingTimeMs;
    private long _renderTimeMs;
    private long _encodeTimeMs;

    private bool _disposed = false;

    // ─────────────────────────────────────────
    // 初始化
    // ─────────────────────────────────────────

    public void Initialize(CommandLineOptions options)
    {
        _asciiWidth = options.Width;
        _asciiHeight = options.Height;
        _videoWidth = options.Width * 16;     // 16px per char
        _videoHeight = options.Height * 16;   // 16px per char

        // 1. 初始化视频解码器
        _decoder = new FFmpegVideoDecoder();
        _decoder.Initialize(options.InputFile);

        var videoInfo = _decoder.GetVideoInfo();
        Console.WriteLine($"✓ 视频解码器: {videoInfo.Resolution}, {videoInfo.FrameRate:F2}fps, {videoInfo.FrameCount} 帧");

        // 2. 初始化 SIMD 灰度转换器（无参构造）
        _grayscaleConverter = new SimdGrayscaleConverter();
        Console.WriteLine($"✓ SIMD 灰度转换器已就绪");

        // 3. 初始化 ASCII 字符映射器（查找表）
        string charSet = options.CharSet.ToLower() switch
        {
            "standard" => LookupTableAsciiMapper.Standard,
            "detailed" => LookupTableAsciiMapper.Detailed,
            _ => LookupTableAsciiMapper.Standard
        };
        _asciiMapper = new LookupTableAsciiMapper(charSet);
        Console.WriteLine($"✓ ASCII 映射器已就绪（字符集: {options.CharSet}, {charSet.Length} 字符）");

        // 4. 初始化 SkiaSharp 渲染器（字符缓存版）
        var config = new CharacterSetConfig
        {
            FontFamily = options.FontFamily,
            FontSize = options.FontSize,
            CharWidth = 16,
            CharHeight = 16,
            BackgroundColor = (0, 0, 0),
            ForegroundColor = (255, 255, 255)
        };
        _renderer = new SkiaCachedAsciiRenderer(config, _asciiWidth, _asciiHeight);
        _renderer.Initialize();
        _videoWidth = _renderer.OutputWidth;
        _videoHeight = _renderer.OutputHeight;
        Console.WriteLine($"✓ SkiaSharp 渲染器已就绪（{_videoWidth}x{_videoHeight} 像素）");

        // 5. 初始化 H.264 编码器
        _encoder = new FFmpegVideoEncoder();
        _encoder.Initialize(options.OutputFile, _videoWidth, _videoHeight, options.FrameRate);
        Console.WriteLine($"✓ H.264 编码器已就绪（{_videoWidth}x{_videoHeight} @ {options.FrameRate}fps）");
    }

    // ─────────────────────────────────────────
    // 处理流水线
    // ─────────────────────────────────────────

    public int Process(CommandLineOptions options)
    {
        if (_decoder == null)
            throw new InvalidOperationException("流水线未初始化，请先调用 Initialize()");

        int totalFrames = 0;
        int maxFrames = options.MaxFrames > 0
            ? options.MaxFrames
            : (int)Math.Min(_decoder.FrameCount, int.MaxValue);

        var progressSw = Stopwatch.StartNew();
        int lastProgress = 0;

        Console.WriteLine();
        Console.WriteLine("【处理开始】");

        var overallSw = Stopwatch.StartNew();

        while (true)
        {
            if (totalFrames >= maxFrames)
                break;

            // ① 解码一帧 (~30ms)
            var sw = Stopwatch.StartNew();
            byte[]? rgbFrame = _decoder.GetNextFrame();
            _decodeTimeMs += sw.ElapsedMilliseconds;

            if (rgbFrame == null)
                break; // 视频结束

            // ② RGB24 → Grayscale (~4ms, SIMD)
            sw.Restart();
            byte[] grayFrame = _grayscaleConverter.ConvertToGrayscale(rgbFrame, _videoWidth, _videoHeight);
            _grayscaleTimeMs += sw.ElapsedMilliseconds;

            // ③ Grayscale → ASCII string (~1ms, 查找表)
            sw.Restart();
            string asciiArt = _asciiMapper.MapToAscii(
                grayFrame, _videoWidth, _videoHeight, _asciiWidth, _asciiHeight);
            _mappingTimeMs += sw.ElapsedMilliseconds;

            // ④ ASCII string → RGB24 image (~0.5ms, SkiaSharp+Cache)
            sw.Restart();
            byte[] renderedFrame = _renderer.RenderFrame(asciiArt);
            _renderTimeMs += sw.ElapsedMilliseconds;

            // ⑤ RGB24 → H.264 packet (~20ms, libx264)
            sw.Restart();
            _encoder.EncodeFrame(renderedFrame);
            _encodeTimeMs += sw.ElapsedMilliseconds;

            totalFrames++;

            // 进度显示（每秒一次，或 --no-progress 禁用）
            if (!options.NoProgress && progressSw.ElapsedMilliseconds >= 1000)
            {
                progressSw.Restart();
                int framesThisSec = totalFrames - lastProgress;
                lastProgress = totalFrames;

                double progress = maxFrames > 0
                    ? (double)totalFrames / maxFrames * 100
                    : 0;
                double fps = framesThisSec;

                // 进度条
                int barWidth = 30;
                int filled = (int)(progress / 100 * barWidth);
                string bar = new string('█', filled) + new string('░', barWidth - filled);

                Console.Write(
                    $"\r  ➤ [{bar}] {progress:F1}% | " +
                    $"帧: {totalFrames}/{(maxFrames > 0 ? maxFrames.ToString() : "?")} | " +
                    $"FPS: {fps:F0}");
            }
        }

        if (!options.NoProgress)
            Console.WriteLine(); // 换行

        Console.WriteLine();
        Console.WriteLine($"【处理完成】共处理 {totalFrames} 帧 (耗时 {overallSw.Elapsed.TotalSeconds:F2}s)");

        return totalFrames;
    }

    // ─────────────────────────────────────────
    // 完成编码
    // ─────────────────────────────────────────

    public void WriteTrailer()
    {
        if (_encoder != null && _encoder.IsInitialized)
        {
            _encoder.Finish();  // 使用 Finish() 而不是 Finalize()
        }
    }

    // ─────────────────────────────────────────
    // 性能统计
    // ─────────────────────────────────────────

    public PerformanceStats GetStatistics()
    {
        return new PerformanceStats
        {
            DecodeTimeMs = _decodeTimeMs,
            GrayscaleTimeMs = _grayscaleTimeMs,
            MappingTimeMs = _mappingTimeMs,
            RenderTimeMs = _renderTimeMs,
            EncodeTimeMs = _encodeTimeMs
        };
    }

    public void Dispose()
    {
        if (_disposed) return;

        // 注意：IGrayscaleConverter 没有 IDisposable，跳过
        (_decoder as IDisposable)?.Dispose();
        (_renderer as IDisposable)?.Dispose();
        (_encoder as IDisposable)?.Dispose();

        _disposed = true;
    }
}

/// <summary>
/// 性能统计数据
/// </summary>
public record PerformanceStats
{
    public long DecodeTimeMs { get; init; }
    public long GrayscaleTimeMs { get; init; }
    public long MappingTimeMs { get; init; }
    public long RenderTimeMs { get; init; }
    public long EncodeTimeMs { get; init; }

    /// <summary>总耗时（毫秒）</summary>
    public long TotalTimeMs => 
        DecodeTimeMs + GrayscaleTimeMs + MappingTimeMs + 
        RenderTimeMs + EncodeTimeMs;

    /// <summary>每帧平均耗时</summary>
    public double AvgFrameTimeMs(int totalFrames) =>
        totalFrames > 0 ? (double)TotalTimeMs / totalFrames : 0;
}