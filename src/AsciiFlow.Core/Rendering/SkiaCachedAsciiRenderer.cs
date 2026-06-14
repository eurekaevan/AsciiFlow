using SkiaSharp;
using System.Runtime.InteropServices;

namespace AsciiFlow.Core.Rendering;

/// <summary>
/// 基于 SkiaSharp 3.x + 字符位图缓存的高性能 ASCII 渲染器
/// 性能目标：≤ 0.5ms/帧（1080p → 80x40）
/// </summary>
public class SkiaCachedAsciiRenderer : IAsciiRenderer
{
    private readonly CharacterSetConfig _config;
    private readonly int _targetWidth;
    private readonly int _targetHeight;

    // 字符缓存：256 个 ASCII 字符的 RGB24 位图数据
    private byte[][] _charBitmaps = new byte[256][];

    // 预分配的 RGB24 输出缓冲区
    private byte[] _rgbBuffer;

    private bool _initialized;
    private bool _disposed;

    public int OutputWidth { get; private set; }
    public int OutputHeight { get; private set; }
    public int CharWidth => _config.CharWidth;
    public int CharHeight => _config.CharHeight;

    /// <summary>
    /// 构造函数
    /// </summary>
    public SkiaCachedAsciiRenderer(
        CharacterSetConfig config,
        int targetWidth,
        int targetHeight)
    {
        _config = config ?? throw new ArgumentNullException(nameof(config));
        _targetWidth = targetWidth;
        _targetHeight = targetHeight;

        var (outputW, outputH) = config.CalculateOutputSize(targetWidth, targetHeight);
        OutputWidth = outputW;
        OutputHeight = outputH;
        _rgbBuffer = new byte[outputW * outputH * 3];
    }

    /// <summary>
    /// 初始化渲染器，预渲染 256 个 ASCII 字符到缓存
    /// </summary>
    public void Initialize()
    {
        if (_initialized) return;

        Console.WriteLine($"[渲染器] 预渲染字符缓存（字体: {_config.FontFamily}, 大小: {_config.FontSize}px）...");
        var sw = System.Diagnostics.Stopwatch.StartNew();

        // 使用 SkiaSharp 3.x 新 API：SKFont + SKPaint 分离
        using var font = CreateSkFont();
        using var paint = CreateSkPaint();

        for (int i = 0; i < 256; i++)
        {
            _charBitmaps[i] = RenderCharToBitmap((char)i, font, paint);
        }

        sw.Stop();
        Console.WriteLine($"[渲染器] 预渲染完成，耗时 {sw.Elapsed.TotalMilliseconds:F1}ms");

        _initialized = true;
    }

    /// <summary>
    /// 创建 SKFont 对象（SkiaSharp 3.x 新 API：字体属性从 SKPaint 移到 SKFont）
    /// </summary>
    private SKFont CreateSkFont()
    {
        var typeface = SKTypeface.FromFamilyName(
            _config.FontFamily,
            SKFontStyleWeight.Normal,
            SKFontStyleWidth.Normal,
            SKFontStyleSlant.Upright);

        // SkiaSharp 3.x: 使用 SKFont 设置字体属性
        return new SKFont(typeface, _config.FontSize)
        {
            Edging = SKFontEdging.SubpixelAntialias,
            Hinting = SKFontHinting.None  // 关闭 hinting 以保证固定宽度
        };
    }

    /// <summary>
    /// 创建 SKPaint 对象（仅设置颜色等效果属性）
    /// </summary>
    private SKPaint CreateSkPaint()
    {
        return new SKPaint
        {
            // SkiaSharp 3.x: SKPaint 只负责颜色/样式，不负责字体
            Color = new SKColor(
                _config.ForegroundColor.R,
                _config.ForegroundColor.G,
                _config.ForegroundColor.B),
            IsAntialias = false,  // 关闭抗锯齿以提升字符画清晰度
            Style = SKPaintStyle.Fill
        };
    }

    /// <summary>
    /// 将单个字符渲染为 RGB24 位图数据
    /// </summary>
    private byte[] RenderCharToBitmap(char c, SKFont font, SKPaint paint)
    {
        var bg = _config.BackgroundColor;
        var fg = _config.ForegroundColor;
        int cw = CharWidth;
        int ch = CharHeight;

        // 1. 创建临时 RGBA 位图（SkiaSharp 使用 BGRA 内存布局）
        using var bitmap = new SKBitmap(cw, ch, SKColorType.Bgra8888, SKAlphaType.Premul);
        using var canvas = new SKCanvas(bitmap);

        // 2. 填充背景
        canvas.Clear(new SKColor(bg.R, bg.G, bg.B));

        // 3. 绘制字符（仅可打印 ASCII 范围 33-126）
        if (c > 32 && c < 127)
        {
            string charStr = c.ToString();

            // 使用 SKFont 测量字符边界（3.x 新 API）
            var bounds = new SKRect();
            font.MeasureText(charStr, out bounds);

            // 居中定位
            float x = (cw - bounds.Width) * 0.5f - bounds.Left;
            float y = (ch - bounds.Height) * 0.5f - bounds.Top + bounds.Height * 0.85f;

            // 使用 3.x 新 API：DrawText(text, x, y, font, paint)
            canvas.DrawText(charStr, x, y, font, paint);
        }

        // 4. BGRA → RGB24 转换（使用 GetPixels + 指针操作）
        byte[] rgbData = new byte[cw * ch * 3];
        IntPtr pixelsAddr = bitmap.GetPixels();

        unsafe
        {
            byte* srcPtr = (byte*)pixelsAddr;
            int srcStride = cw * 4;  // BGRA: 4 bytes per pixel

            fixed (byte* dstPtr = rgbData)
            {
                byte* dst = dstPtr;
                for (int y = 0; y < ch; y++)
                {
                    byte* srcRow = srcPtr + y * srcStride;
                    for (int x = 0; x < cw; x++)
                    {
                        int pixelOffset = x * 4;
                        // BGRA → RGB 转换
                        // bitmap pixel: [B, G, R, A]
                        *dst++ = srcRow[pixelOffset + 2];  // R
                        *dst++ = srcRow[pixelOffset + 1];  // G
                        *dst++ = srcRow[pixelOffset];      // B
                    }
                }
            }
        }

        return rgbData;
    }

    /// <summary>
    /// 将 ASCII 字符串渲染为 RGB24 字节数组（核心方法）
    /// </summary>
    public byte[] RenderFrame(string asciiArt)
    {
        if (!_initialized)
            throw new InvalidOperationException("渲染器未初始化，请先调用 Initialize()");

        if (string.IsNullOrEmpty(asciiArt))
            return _rgbBuffer;

        int ow = OutputWidth;
        int rowStride = ow * 3;  // 每行 RGB 字节数

        // 预分配行缓冲区，避免重复 Split
        ReadOnlySpan<char> ascii = asciiArt.AsSpan();
        int lineIndex = 0;
        int lineStart = 0;

        for (int i = 0; i <= ascii.Length; i++)
        {
            bool isLineEnd = (i == ascii.Length) || (ascii[i] == '\n');
            if (!isLineEnd) continue;

            if (lineIndex >= _targetHeight) break;

            var lineSpan = ascii[lineStart..i];
            int linePixelY = lineIndex * CharHeight;
            int lineByteY = linePixelY * rowStride;

            for (int charIdx = 0; charIdx < Math.Min(lineSpan.Length, _targetWidth); charIdx++)
            {
                char ch = lineSpan[charIdx];
                int charCode = ch < 256 ? ch : 32;  // 不可打印字符用空格替代

                // O(1) 查表获取字符位图数据
                byte[] charBitmap = _charBitmaps[charCode];

                // 目标位置：第 charIdx 个字符的左上角
                int destPixelX = charIdx * CharWidth;
                int destByteX = destPixelX * 3;

                // 逐行复制字符位图到输出缓冲区
                int charStride = CharWidth * 3;
                for (int cy = 0; cy < CharHeight; cy++)
                {
                    int srcOffset = cy * charStride;
                    int destOffset = lineByteY + cy * rowStride + destByteX;

                    // 高效复制：使用 Span.CopyTo
                    charBitmap.AsSpan(srcOffset, charStride)
                              .CopyTo(_rgbBuffer.AsSpan(destOffset, charStride));
                }
            }

            lineIndex++;
            lineStart = i + 1;
        }

        return _rgbBuffer;
    }

    /// <summary>
    /// 释放资源
    /// </summary>
    public void Dispose()
    {
        if (_disposed) return;
        _charBitmaps = null!;
        _rgbBuffer = null!;
        _disposed = true;
    }
}