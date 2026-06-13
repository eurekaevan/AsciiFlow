namespace AsciiFlow.Core.Video;

/// <summary>
/// 视频解码器接口
/// 负责将视频文件解码为 RGB 帧
/// </summary>
public interface IVideoDecoder : IDisposable
{
    /// <summary>视频宽度（像素）</summary>
    int Width { get; }
    
    /// <summary>视频高度（像素）</summary>
    int Height { get; }
    
    /// <summary>视频帧率（fps）</summary>
    float FrameRate { get; }
    
    /// <summary>总帧数</summary>
    long FrameCount { get; }
    
    /// <summary>
    /// 初始化视频解码器
    /// </summary>
    /// <param name="videoPath">视频文件路径</param>
    void Initialize(string videoPath);
    
    /// <summary>
    /// 获取下一帧的 RGB 数据
    /// </summary>
    /// <returns>RGB24 格式的字节数组（R0,G0,B0, R1,G1,B1, ...），视频结束时返回 null</returns>
    byte[]? GetNextFrame();
}