namespace AsciiFlow.Core.Rendering;

/// <summary>
/// ASCII 渲染器接口
/// 将 ASCII 字符串渲染为 RGB24 字节数组
/// </summary>
public interface IAsciiRenderer : IDisposable
{
    /// <summary>输出图像宽度（像素）</summary>
    int OutputWidth { get; }

    /// <summary>输出图像高度（像素）</summary>
    int OutputHeight { get; }

    /// <summary>单个字符宽度（像素）</summary>
    int CharWidth { get; }

    /// <summary>单个字符高度（像素）</summary>
    int CharHeight { get; }

    /// <summary>
    /// 初始化渲染器（预渲染字符缓存）
    /// </summary>
    void Initialize();

    /// <summary>
    /// 将 ASCII 字符串渲染为 RGB24 字节数组
    /// </summary>
    /// <param name="asciiArt">ASCII 艺术字符串（每行以换行符分隔）</param>
    /// <returns>RGB24 字节数组（R0,G0,B0,R1,G1,B1,...）</returns>
    byte[] RenderFrame(string asciiArt);

    /// <summary>
    /// 将 ASCII 字符串渲染为 RGB24 字节数组（支持彩色字符）
    /// </summary>
    /// <param name="asciiArt">ASCII 艺术字符串</param>
    /// <param name="colors">每个字符单元格的 RGB 颜色数组</param>
    /// <param name="useColor">是否启用彩色模式</param>
    /// <returns>RGB24 字节数组</returns>
    byte[] RenderFrameWithColor(string asciiArt, (byte R, byte G, byte B)[] colors, bool useColor = true);
}