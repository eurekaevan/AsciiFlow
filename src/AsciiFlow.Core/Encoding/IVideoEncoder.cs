namespace AsciiFlow.Core.Encoding;

/// <summary>
/// 视频编码器接口
/// 将 RGB24 帧数据编码为 H.264 MP4 视频文件
/// </summary>
public interface IVideoEncoder : IDisposable
{
    /// <summary>编码器是否已初始化</summary>
    bool IsInitialized { get; }

    /// <summary>输出视频宽度</summary>
    int Width { get; }

    /// <summary>输出视频高度</summary>
    int Height { get; }

    /// <summary>视频帧率</summary>
    double FrameRate { get; }

    /// <summary>已编码帧数</summary>
    long EncodedFrames { get; }

    /// <summary>
    /// 初始化编码器
    /// </summary>
    /// <param name="outputPath">输出 MP4 文件路径</param>
    /// <param name="width">视频宽度（像素）</param>
    /// <param name="height">视频高度（像素）</param>
    /// <param name="frameRate">视频帧率（fps）</param>
    void Initialize(string outputPath, int width, int height, double frameRate = 30.0);

    /// <summary>
    /// 编码一帧 RGB24 图像数据
    /// </summary>
    /// <param name="rgbData">RGB24 字节数组（R0,G0,B0,R1,G1,B1,...）</param>
    void EncodeFrame(byte[] rgbData);

    /// <summary>
    /// 完成编码并写入文件尾部（MP4 索引等）
    /// 调用后编码器不可再用，需 Dispose
    /// </summary>
    void Finish();
}