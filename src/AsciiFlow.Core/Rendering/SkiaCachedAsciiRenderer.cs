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

    // 字符缓存：256 个 ASCII 字符的 RGB24 位图数据与 Alpha 遮罩数据
    private byte[][] _charBitmaps = new byte[256][];
    private byte[][] _charAlphaMasks = new byte[256][];

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
            var (rgb, alpha) = RenderCharToBitmap((char)i, font, paint);
            _charBitmaps[i] = rgb;
            _charAlphaMasks[i] = alpha;
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
        SKTypeface? typeface = null;

        // 候选字体列表（跨平台兼容：Windows / Linux / macOS）
        string[] candidates = new string[]
        {
            _config.FontFamily,
            "Consolas",
            "Cascadia Mono",
            "Cascadia Code",
            "DejaVu Sans Mono",
            "Liberation Mono",
            "FreeMono",
            "Courier New",
            "Courier"
        };

        var availableFamilies = SKFontManager.Default.FontFamilies.ToHashSet(StringComparer.OrdinalIgnoreCase);

        foreach (var candidate in candidates)
        {
            if (string.IsNullOrEmpty(candidate)) continue;
            if (availableFamilies.Contains(candidate))
            {
                var tf = SKTypeface.FromFamilyName(candidate, SKFontStyleWeight.Normal, SKFontStyleWidth.Normal, SKFontStyleSlant.Upright);
                if (tf != null && !string.IsNullOrEmpty(tf.FamilyName))
                {
                    typeface = tf;
                    break;
                }
            }
        }

        if (typeface == null || string.IsNullOrEmpty(typeface.FamilyName))
        {
            typeface = SKFontManager.Default.MatchFamily("monospace", SKFontStyle.Normal)
                       ?? SKTypeface.Default;
        }

        return new SKFont(typeface, _config.FontSize)
        {
            Edging = SKFontEdging.SubpixelAntialias,
            Hinting = SKFontHinting.None
        };
    }

    /// <summary>
    /// 创建 SKPaint 对象（仅设置颜色等效果属性）
    /// </summary>
    private SKPaint CreateSkPaint()
    {
        return new SKPaint
        {
            Color = new SKColor(
                _config.ForegroundColor.R,
                _config.ForegroundColor.G,
                _config.ForegroundColor.B),
            IsAntialias = false,  // 关闭抗锯齿以提升字符画清晰度
            Style = SKPaintStyle.Fill
        };
    }

    /// <summary>
    /// 将单个字符渲染为 RGB24 位图数据与 Alpha 遮罩数据
    /// </summary>
    private (byte[] Rgb, byte[] Alpha) RenderCharToBitmap(char c, SKFont font, SKPaint paint)
    {
        var bg = _config.BackgroundColor;
        var fg = _config.ForegroundColor;
        int cw = CharWidth;
        int ch = CharHeight;

        // 1. 创建临时 RGBA 位图（SkiaSharp 使用 BGRA 内存布局）
        using var bitmap = new SKBitmap(cw, ch, SKColorType.Bgra8888, SKAlphaType.Premul);
        using (var canvas = new SKCanvas(bitmap))
        {
            // 2. 填充背景
            canvas.Clear(new SKColor(bg.R, bg.G, bg.B));

            // 3. 绘制字符（仅可打印 ASCII 范围 33-126）
            if (c > 32 && c < 127)
            {
                string charStr = c.ToString();

                // 使用 SKFont 测量字符边界
                var bounds = new SKRect();
                font.MeasureText(charStr, out bounds);

                // 垂直居中计算：使用 font.Metrics 计算 baseline 避免文本超出 16px 底部
                var metrics = font.Metrics;
                float fontHeight = metrics.Descent - metrics.Ascent;
                float x = (cw - bounds.Width) * 0.5f - bounds.Left;
                float y = (ch - fontHeight) * 0.5f - metrics.Ascent;

                canvas.DrawText(charStr, x, y, font, paint);
            }

            canvas.Flush();
        }

        // 4. BGRA → RGB24 及 Alpha 遮罩提取
        byte[] rgbData = new byte[cw * ch * 3];
        byte[] alphaData = new byte[cw * ch];
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
                        byte r = srcRow[pixelOffset + 2];
                        byte g = srcRow[pixelOffset + 1];
                        byte b = srcRow[pixelOffset];
                        *dst++ = r;
                        *dst++ = g;
                        *dst++ = b;

                        byte a = Math.Max(r, Math.Max(g, b));
                        alphaData[y * cw + x] = a;
                    }
                }
            }
        }

        return (rgbData, alphaData);
    }

    /// <summary>
    /// 将 ASCII 字符串渲染为 RGB24 字节数组（黑白模式）
    /// </summary>
    public byte[] RenderFrame(string asciiArt)
    {
        if (!_initialized)
            throw new InvalidOperationException("渲染器未初始化，请先调用 Initialize()");

        if (string.IsNullOrEmpty(asciiArt))
            return _rgbBuffer;

        int ow = OutputWidth;
        int rowStride = ow * 3;  // 每行 RGB 字节数

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
                int charCode = ch < 256 ? ch : 32;

                byte[] charBitmap = _charBitmaps[charCode];

                int destPixelX = charIdx * CharWidth;
                int destByteX = destPixelX * 3;

                int charStride = CharWidth * 3;
                for (int cy = 0; cy < CharHeight; cy++)
                {
                    int srcOffset = cy * charStride;
                    int destOffset = lineByteY + cy * rowStride + destByteX;

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
    /// 将 ASCII 字符串渲染为 RGB24 字节数组（支持彩色字符，Parallel 并行加速）
    /// </summary>
    public byte[] RenderFrameWithColor(string asciiArt, (byte R, byte G, byte B)[] colors, bool useColor = true)
    {
        if (!_initialized)
            throw new InvalidOperationException("渲染器未初始化，请先调用 Initialize()");

        if (!useColor || colors == null || colors.Length == 0)
            return RenderFrame(asciiArt);

        if (string.IsNullOrEmpty(asciiArt))
            return _rgbBuffer;

        int ow = OutputWidth;
        int rowStride = ow * 3;
        var bg = _config.BackgroundColor;
        int charW = CharWidth;
        int charH = CharHeight;
        int targetW = _targetWidth;
        int targetH = _targetHeight;

        string[] lines = asciiArt.Split('\n');
        int lineCount = Math.Min(lines.Length, targetH);

        unsafe
        {
            fixed (byte* bufPtr = _rgbBuffer)
            {
                IntPtr bufAddr = (IntPtr)bufPtr;

                Parallel.For(0, lineCount, lineIndex =>
                {
                    byte* bPtr = (byte*)bufAddr;
                    string lineStr = lines[lineIndex];
                    int linePixelY = lineIndex * charH;
                    int lineByteY = linePixelY * rowStride;
                    int colorOffsetBase = lineIndex * targetW;
                    int lineLen = Math.Min(lineStr.Length, targetW);

                    for (int charIdx = 0; charIdx < lineLen; charIdx++)
                    {
                        char ch = lineStr[charIdx];
                        int charCode = ch < 256 ? ch : 32;

                        byte[] charAlpha = _charAlphaMasks[charCode];
                        var fgColor = colors[colorOffsetBase + charIdx];

                        int destByteX = charIdx * charW * 3;

                        for (int cy = 0; cy < charH; cy++)
                        {
                            int maskRowOffset = cy * charW;
                            byte* destRowPtr = bPtr + lineByteY + cy * rowStride + destByteX;

                            for (int cx = 0; cx < charW; cx++)
                            {
                                byte alpha = charAlpha[maskRowOffset + cx];
                                byte* pxPtr = destRowPtr + cx * 3;

                                if (alpha == 0)
                                {
                                    pxPtr[0] = bg.R;
                                    pxPtr[1] = bg.G;
                                    pxPtr[2] = bg.B;
                                }
                                else if (alpha == 255)
                                {
                                    pxPtr[0] = fgColor.R;
                                    pxPtr[1] = fgColor.G;
                                    pxPtr[2] = fgColor.B;
                                }
                                else
                                {
                                    int invAlpha = 255 - alpha;
                                    pxPtr[0] = (byte)((fgColor.R * alpha + bg.R * invAlpha) / 255);
                                    pxPtr[1] = (byte)((fgColor.G * alpha + bg.G * invAlpha) / 255);
                                    pxPtr[2] = (byte)((fgColor.B * alpha + bg.B * invAlpha) / 255);
                                }
                            }
                        }
                    }
                });
            }
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
        _charAlphaMasks = null!;
        _rgbBuffer = null!;
        _disposed = true;
    }
}