namespace AsciiFlow.Core.Encoding;

/// <summary>
/// 视频编码器接口
/// 负责将 RGB 帧编码为 H.264 MP4 视频
/// </summary>
public interface IVideoEncoder : IDisposable
{
    /// <summary>
    /// 初始化视频编码器
    /// </summary>
    /// <param name="outputPath">输出文件路径</param>
    /// <param name="width">视频宽度（像素）</param>
    /// <param name="height">视频高度（像素）</param>
    /// <param name="frameRate">帧率（fps）</param>
    void Initialize(string outputPath, int width, int height, float frameRate = 30f);
    
    /// <summary>
    /// 编码一帧图像
    /// </summary>
    /// <param name="rgbData">RGB24 字节数组</param>
    void EncodeFrame(byte[] rgbData);
    
    /// <summary>
    /// 完成编码并保存文件
    /// </summary>
    void Finish();
}