namespace AsciiFlow.Core.Rendering;

/// <summary>
/// ASCII 渲染器接口（SkiaSharp + 字符缓存）
/// 负责将 ASCII 字符串渲染为 RGB 图像
/// </summary>
public interface IAsciiRenderer : IDisposable
{
    /// <summary>输出图像宽度（像素）</summary>
    int OutputWidth { get; }
    
    /// <summary>输出图像高度（像素）</summary>
    int OutputHeight { get; }
    
    /// <summary>
    /// 初始化渲染器（预渲染字符缓存）
    /// </summary>
    void Initialize();
    
    /// <summary>
    /// 将 ASCII 字符串渲染为 RGB 图像
    /// </summary>
    /// <param name="asciiArt">ASCII 艺术字符串</param>
    /// <returns>RGB24 格式的字节数组</returns>
    byte[] RenderFrame(string asciiArt);
}