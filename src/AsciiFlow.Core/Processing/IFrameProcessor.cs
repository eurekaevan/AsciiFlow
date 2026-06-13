namespace AsciiFlow.Core.Processing;

/// <summary>
/// 帧处理器接口
/// 负责将 RGB 帧转换为灰度数据
/// </summary>
public interface IFrameProcessor : IDisposable
{
    /// <summary>
    /// 将 RGB 数据转换为灰度
    /// </summary>
    /// <param name="rgbData">RGB24 字节数组</param>
    /// <param name="width">图像宽度</param>
    /// <param name="height">图像高度</param>
    /// <returns>灰度字节数组（每个字节 0-255）</returns>
    byte[] ConvertToGrayscale(byte[] rgbData, int width, int height);
}