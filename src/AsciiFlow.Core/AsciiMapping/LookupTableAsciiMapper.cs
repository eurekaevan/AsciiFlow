using System.Text;
using System.Threading.Tasks;

namespace AsciiFlow.Core.AsciiMapping;

/// <summary>
/// 基于查找表的高性能 ASCII 字符映射器
/// </summary>
public class LookupTableAsciiMapper : IAsciiMapper
{
    private readonly char[] _characterSet;
    private readonly char[] _lookupTable;
    private readonly ParallelOptions _parallelOptions;

    /// <summary>
    /// 字符集预设
    /// </summary>
    public static readonly string Standard = 
        "$@B%8&WM#*oahkbdpqwmZO0QLCJUYXzcvunxrjft/\\|()1{}[]?-_+~<>i!lI;:,\"^`'. ";

    public static readonly string Detailed = 
        "@#W$8bWM%*oahkdqwmZO0QLUYXZ";

    /// <summary>
    /// 构造函数
    /// </summary>
    /// <param name="characterSet">
    /// 字符集字符串（默认使用 Standard）
    /// </param>
    /// <param name="maxDegreeOfParallelism">最大并行度（CPU 核心数）</param>
    public LookupTableAsciiMapper(
        string? characterSet = null,
        int maxDegreeOfParallelism = 0)
    {
        _characterSet = (characterSet ?? Standard).ToCharArray();
        _lookupTable = BuildLookupTable(_characterSet);
        
        _parallelOptions = new ParallelOptions
        {
            MaxDegreeOfParallelism = maxDegreeOfParallelism > 0 
                ? maxDegreeOfParallelism 
                : Environment.ProcessorCount
        };
    }

    /// <summary>
    /// 构建查找表（结合 S-Curve 伽马对比度增强算法）
    /// </summary>
    private static char[] BuildLookupTable(char[] charset)
    {
        var table = new char[256];
        int charsetLength = charset.Length;

        for (int grayValue = 0; grayValue < 256; grayValue++)
        {
            // S-Curve 对比度增强：增强暗部与高光的梯度的清晰度
            double norm = grayValue / 255.0;
            double sCurve = norm < 0.5 
                ? 2.0 * norm * norm 
                : 1.0 - 2.0 * (1.0 - norm) * (1.0 - norm);

            int index = (int)(sCurve * (charsetLength - 1) + 0.5);
            table[grayValue] = charset[Math.Clamp(index, 0, charsetLength - 1)];
        }

        return table;
    }

    /// <summary>
    /// 将灰度数据映射为 ASCII 字符串
    /// </summary>
    public string MapToAscii(
        byte[] grayData, 
        int width, 
        int height,
        int targetWidth, 
        int targetHeight)
    {
        ValidateParameters(grayData, width, height, targetWidth, targetHeight);

        // 计算缩放比例
        double scaleX = (double)width / targetWidth;
        double scaleY = (double)height / targetHeight;

        var lines = new string[targetHeight];

        // 并行处理每一行
        Parallel.For(0, targetHeight, _parallelOptions, targetY =>
        {
            int srcY = (int)(targetY * scaleY);
            var lineBuffer = new char[targetWidth];
            int endY = Math.Min((int)((targetY + 1) * scaleY), height);

            for (int targetX = 0; targetX < targetWidth; targetX++)
            {
                int srcX = (int)(targetX * scaleX);
                int endX = Math.Min((int)((targetX + 1) * scaleX), width);

                int sum = 0;
                int count = 0;

                for (int y = srcY; y < endY; y++)
                {
                    int rowOffset = y * width;
                    for (int x = srcX; x < endX; x++)
                    {
                        int index = rowOffset + x;
                        if (index < grayData.Length)
                        {
                            sum += grayData[index];
                            count++;
                        }
                    }
                }

                byte avgGray = count > 0 ? (byte)(sum / count) : (byte)0;
                lineBuffer[targetX] = _lookupTable[avgGray];
            }

            lines[targetY] = new string(lineBuffer);
        });

        return string.Join("\n", lines);
    }

    /// <summary>
    /// 将图像映射为 ASCII 字符串，并同时采样每个字符单元格的原视频 RGB 颜色（用于彩色 ASCII）
    /// </summary>
    public (string AsciiArt, (byte R, byte G, byte B)[] Colors) MapToAsciiWithColor(
        byte[] rgbData,
        byte[] grayData,
        int width,
        int height,
        int targetWidth,
        int targetHeight)
    {
        ValidateParameters(grayData, width, height, targetWidth, targetHeight);

        double scaleX = (double)width / targetWidth;
        double scaleY = (double)height / targetHeight;

        var lines = new string[targetHeight];
        var colors = new (byte R, byte G, byte B)[targetWidth * targetHeight];

        Parallel.For(0, targetHeight, _parallelOptions, targetY =>
        {
            int srcY = (int)(targetY * scaleY);
            var lineBuffer = new char[targetWidth];
            int endY = Math.Min((int)((targetY + 1) * scaleY), height);

            for (int targetX = 0; targetX < targetWidth; targetX++)
            {
                int srcX = (int)(targetX * scaleX);
                int endX = Math.Min((int)((targetX + 1) * scaleX), width);

                int sumR = 0, sumG = 0, sumB = 0, sumGray = 0;
                int count = 0;

                for (int y = srcY; y < endY; y++)
                {
                    int rowOffset = y * width;
                    for (int x = srcX; x < endX; x++)
                    {
                        int pixelIdx = rowOffset + x;
                        int rgbIdx = pixelIdx * 3;
                        if (rgbIdx + 2 < rgbData.Length)
                        {
                            sumR += rgbData[rgbIdx];
                            sumG += rgbData[rgbIdx + 1];
                            sumB += rgbData[rgbIdx + 2];
                            sumGray += grayData[pixelIdx];
                            count++;
                        }
                    }
                }

                byte avgGray = count > 0 ? (byte)(sumGray / count) : (byte)0;
                lineBuffer[targetX] = _lookupTable[avgGray];

                byte avgR = count > 0 ? (byte)(sumR / count) : (byte)255;
                byte avgG = count > 0 ? (byte)(sumG / count) : (byte)255;
                byte avgB = count > 0 ? (byte)(sumB / count) : (byte)255;

                colors[targetY * targetWidth + targetX] = (avgR, avgG, avgB);
            }

            lines[targetY] = new string(lineBuffer);
        });

        return (string.Join("\n", lines), colors);
    }

    /// <summary>
    /// 参数验证
    /// </summary>
    private static void ValidateParameters(
        byte[] grayData, 
        int width, 
        int height,
        int targetWidth, 
        int targetHeight)
    {
        if (grayData == null)
            throw new ArgumentNullException(nameof(grayData));
        
        if (width <= 0 || height <= 0)
            throw new ArgumentOutOfRangeException(
                $"Width and height must be positive: {width}x{height}");
        
        if (grayData.Length != width * height)
            throw new ArgumentException(
                $"Gray data length {grayData.Length} doesn't match {width}x{height}");
        
        if (targetWidth <= 0 || targetHeight <= 0)
            throw new ArgumentOutOfRangeException(
                $"Target dimensions must be positive: {targetWidth}x{targetHeight}");
    }
}