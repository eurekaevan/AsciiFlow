namespace AsciiFlow.Core.Ascii;

/// <summary>
/// ASCII 映射器接口
/// 负责将灰度数据映射为 ASCII 字符串
/// </summary>
public interface IAsciiMapper
{
    /// <summary>
    /// 将灰度数据映射为 ASCII 字符串
    /// </summary>
    /// <param name="grayData">灰度字节数组</param>
    /// <param name="width">图像宽度（字符数）</param>
    /// <param name="height">图像高度（行数）</param>
    /// <returns>ASCII 艺术字符串（每行以\n分隔）</returns>
    string MapToAscii(byte[] grayData, int width, int height);
}