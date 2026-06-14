namespace AsciiFlow.Core.Rendering;

/// <summary>
/// ASCII 渲染配置
/// </summary>
public record CharacterSetConfig
{
    /// <summary>字体族名称</summary>
    public string FontFamily { get; init; } = "Consolas";

    /// <summary>字体大小（像素）</summary>
    public float FontSize { get; init; } = 16f;

    /// <summary>字符宽度（像素）</summary>
    public int CharWidth { get; init; } = 12;

    /// <summary>字符高度（像素）</summary>
    public int CharHeight { get; init; } = 20;

    /// <summary>背景颜色（RGB）</summary>
    public (byte R, byte G, byte B) BackgroundColor { get; init; } = (0, 0, 0);

    /// <summary>前景颜色（RGB）</summary>
    public (byte R, byte G, byte B) ForegroundColor { get; init; } = (255, 255, 255);

    /// <summary>
    /// 根据配置计算输出图像尺寸
    /// </summary>
    public (int Width, int Height) CalculateOutputSize(int targetWidth, int targetHeight)
    {
        return (targetWidth * CharWidth, targetHeight * CharHeight);
    }

    /// <summary>
    /// 默认配置
    /// </summary>
    public static readonly CharacterSetConfig Default = new CharacterSetConfig();

    /// <summary>
    /// 高清配置
    /// </summary>
    public static readonly CharacterSetConfig HighDefinition = new CharacterSetConfig
    {
        FontFamily = "Consolas",
        FontSize = 20f,
        CharWidth = 16,
        CharHeight = 24
    };
}