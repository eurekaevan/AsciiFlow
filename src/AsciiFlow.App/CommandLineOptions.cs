using CommandLine;

namespace AsciiFlow.App;

/// <summary>
/// 命令行选项定义
/// </summary>
public class CommandLineOptions
{
    [Option('i', "input", Required = true, HelpText = "输入视频文件路径")]
    public string InputFile { get; set; } = string.Empty;

    [Option('o', "output", Required = false, Default = "output.mp4", HelpText = "输出视频文件路径")]
    public string OutputFile { get; set; } = "output.mp4";

    [Option('w', "width", Required = false, Default = 80, HelpText = "ASCII 艺术宽度（字符数）")]
    public int Width { get; set; } = 80;

    [Option('h', "height", Required = false, Default = 40, HelpText = "ASCII 艺术高度（字符数）")]
    public int Height { get; set; } = 40;

    [Option('f', "framerate", Required = false, Default = 24.0, HelpText = "输出视频帧率")]
    public double FrameRate { get; set; } = 24.0;

    [Option('c', "charset", Required = false, Default = "standard", 
        HelpText = "字符集: standard(69字符) 或 detailed(25字符)")]
    public string CharSet { get; set; } = "standard";

    [Option("font-size", Required = false, Default = 14.0f, HelpText = "渲染字体大小（像素）")]
    public float FontSize { get; set; } = 14.0f;

    [Option("font-family", Required = false, Default = "Consolas", HelpText = "渲染字体族")]
    public string FontFamily { get; set; } = "Consolas";

    [Option("max-frames", Required = false, Default = 0, HelpText = "最大处理帧数（0 = 全部）")]
    public int MaxFrames { get; set; } = 0;

    [Option('v', "verbose", Required = false, Default = false, HelpText = "显示详细日志")]
    public bool Verbose { get; set; } = false;

    [Option("no-progress", Required = false, Default = false, HelpText = "禁用进度显示")]
    public bool NoProgress { get; set; } = false;
}