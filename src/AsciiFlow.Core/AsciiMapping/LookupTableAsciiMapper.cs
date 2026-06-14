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
    /// 构建查找表
    /// </summary>
    /// <param name="charset">字符集</param>
    /// <returns>查找表（256 个元素）</returns>
    private static char[] BuildLookupTable(char[] charset)
    {
        var table = new char[256];
        int charsetLength = charset.Length;

        for (int grayValue = 0; grayValue < 256; grayValue++)
        {
            // 反向映射：深色用密集字符，浅色用稀疏字符
            // 0（黑色）→ charset[0]（最密集）
            // 255（白色）→ charset[end]（最稀疏）
            int index = (grayValue * (charsetLength - 1)) / 255;
            table[grayValue] = charset[index];
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

        // 预分配 StringBuilder 容量（避免动态扩容）
        // 格式: targetWidth 字符 + 换行符，共 targetHeight 行
        int capacity = (targetWidth + 1) * targetHeight;
        var lines = new string[targetHeight];

        // 并行处理每一行
        Parallel.For(0, targetHeight, _parallelOptions, targetY =>
        {
            // 计算源图像对应的 Y 范围
            int srcY = (int)(targetY * scaleY);
            
            // 预分配本行缓冲区
            var lineBuffer = new char[targetWidth];

            // 处理本行的每个字符
            for (int targetX = 0; targetX < targetWidth; targetX++)
            {
                // 计算源图像对应的 X 范围
                int srcX = (int)(targetX * scaleX);
                
                // 取区域内所有像素的平均灰度值
                int sum = 0;
                int count = 0;

                int endY = Math.Min((int)((targetY + 1) * scaleY), height);
                int endX = Math.Min((int)((targetX + 1) * scaleX), width);

                for (int y = srcY; y < endY; y++)
                {
                    for (int x = srcX; x < endX; x++)
                    {
                        int index = y * width + x;
                        if (index < grayData.Length)
                        {
                            sum += grayData[index];
                            count++;
                        }
                    }
                }

                // 计算平均灰度值
                byte avgGray = count > 0 ? (byte)(sum / count) : (byte)0;

                // O(1) 查表
                lineBuffer[targetX] = _lookupTable[avgGray];
            }

            // 保存本行结果
            lines[targetY] = new string(lineBuffer);
        });

        // 合并所有行
        return string.Join("\n", lines);
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