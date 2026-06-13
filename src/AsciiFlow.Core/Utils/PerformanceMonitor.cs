using System.Diagnostics;

namespace AsciiFlow.Core.Utils;

/// <summary>
/// 性能监控器
/// 用于跟踪处理速度、FPS 和内存使用
/// </summary>
public class PerformanceMonitor : IDisposable
{
    private readonly Stopwatch _stopwatch = new();
    private long _frameCount;
    private long _totalElapsedMs;
    private bool _disposed;

    /// <summary>
    /// 开始监控
    /// </summary>
    public void Start()
    {
        _stopwatch.Restart();
    }

    /// <summary>
    /// 记录一帧处理完成
    /// </summary>
    /// <param name="elapsedMs">该帧处理耗时（毫秒）</param>
    public void RecordFrame(long elapsedMs)
    {
        _frameCount++;
        _totalElapsedMs += elapsedMs;
    }

    /// <summary>
    /// 获取平均 FPS
    /// </summary>
    public double GetAverageFps()
    {
        if (_totalElapsedMs == 0) return 0;
        return _frameCount / (_totalElapsedMs / 1000.0);
    }

    /// <summary>
    /// 获取平均帧处理时间
    /// </summary>
    public double GetAverageFrameTimeMs()
    {
        return _frameCount > 0 ? _totalElapsedMs / (double)_frameCount : 0;
    }

    /// <summary>
    /// 打印性能统计
    /// </summary>
    public void PrintStatistics()
    {
        Console.WriteLine($"性能统计:");
        Console.WriteLine($"  总帧数：{_frameCount}");
        Console.WriteLine($"  总耗时：{_totalElapsedMs / 1000.0:F2}s");
        Console.WriteLine($"  平均帧时间：{GetAverageFrameTimeMs():F2}ms");
        Console.WriteLine($"  平均 FPS: {GetAverageFps():F1}");
        Console.WriteLine($"  GC 收集次数：Gen0={GC.CollectionCount(0)}, Gen1={GC.CollectionCount(1)}, Gen2={GC.CollectionCount(2)}");
    }

    public void Dispose()
    {
        if (_disposed) return;
        _stopwatch.Stop();
        _disposed = true;
        PrintStatistics();
    }
}