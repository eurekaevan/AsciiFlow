using System;

namespace AsciiFlow.Core.Processing;

/// <summary>
/// 标量灰度转换器（非向量化，用于性能对比）
/// </summary>
public class ScalarGrayscaleConverter : IGrayscaleConverter
{
    // ITU-R BT.709 人眼感知亮度系数：0.2126 * R + 0.7152 * G + 0.0722 * B
    private const int R_COEFF = 54;
    private const int G_COEFF = 183;
    private const int B_COEFF = 19;

    /// <summary>
    /// 将 RGB 数据转换为灰度数据（标量版本）
    /// </summary>
    public byte[] ConvertToGrayscale(byte[] rgbData, int width, int height)
    {
        if (rgbData == null)
            throw new ArgumentNullException(nameof(rgbData));

        if (width <= 0 || height <= 0)
            throw new ArgumentException($"Width 和 height 必须是正数");

        int expectedLength = width * height * 3;
        if (rgbData.Length != expectedLength)
            throw new ArgumentException(
                $"RGB 数据长度不匹配，期望 {expectedLength}，实际 {rgbData.Length}");

        byte[] grayData = new byte[width * height];
        int pixelCount = width * height;

        for (int i = 0; i < pixelCount; i++)
        {
            int rgbBaseIndex = i * 3;
            int r = rgbData[rgbBaseIndex];
            int g = rgbData[rgbBaseIndex + 1];
            int b = rgbData[rgbBaseIndex + 2];

            int gray = (R_COEFF * r + G_COEFF * g + B_COEFF * b) >> 8;
            grayData[i] = (byte)Math.Clamp(gray, 0, 255);
        }

        return grayData;
    }
}