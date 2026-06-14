using System;
using FFmpeg.AutoGen;

namespace AsciiFlow.Core.Video;

/// <summary>
/// FFmpeg 库初始化器（全局单例）
/// </summary>
public static class FFmpegInitializer
{
    private static string? _ffmpegRootPath;
    private static bool _initialized = false;
    private static readonly object _lock = new();

    /// <summary>
    /// 错误码常量（FFmpeg 标准错误码）
    /// </summary>
    public static class ErrorCode
    {
        /// <summary>AVERROR_EOF - 文件结束</summary>
        public const int EOF = unchecked((int)0x20464F45); // FFERRTAG('E','O','F',' ')
        
        /// <summary>AVERROR_EAGAIN - 需要更多数据</summary>
        public const int EAGAIN = -11;
        
        /// <summary>AVERROR_EIO - I/O 错误</summary>
        public const int EIO = -5;
        
        /// <summary>AVERROR_EINVAL - 无效参数</summary>
        public const int EINVAL = -22;
    }

    /// <summary>
    /// 初始化 FFmpeg 库
    /// </summary>
    /// <param name="ffmpegRootPath">FFmpeg 动态库路径</param>
    public static void Initialize(string? ffmpegRootPath = null)
    {
        lock (_lock)
        {
            if (_initialized)
                return;

            if (!string.IsNullOrEmpty(ffmpegRootPath))
            {
                _ffmpegRootPath = ffmpegRootPath;
                ffmpeg.RootPath = ffmpegRootPath;
            }

            try
            {
                // FFmpeg.AutoGen 8.1.0 使用静态类初始化
                // 通过访问任意方法触发初始化
                var version = ffmpeg.av_version_info();
                Console.WriteLine($"[FFmpeg] 初始化成功");
                Console.WriteLine($"[FFmpeg] 版本信息: {version}");
                
                _initialized = true;
            }
            catch (DllNotFoundException ex)
            {
                // 使用 FFmpegPathResolver 的详细帮助信息
                throw new DllNotFoundException(
                    $"FFmpeg 动态库加载失败：{ex.Message}\n\n{FFmpegPathResolver.GetHelpMessage()}", ex);
            }
            catch (NotSupportedException ex)
            {
                // "Specified method is not supported" 通常是 ABI 版本不匹配
                throw new NotSupportedException(
                    $"FFmpeg ABI 不兼容：{ex.Message}\n\n" +
                    "可能原因：FFmpeg.AutoGen 8.1.0 需要 FFmpeg 7.x 系列库。\n" +
                    "请确认您的 FFmpeg 库版本与 FFmpeg.AutoGen 版本匹配。\n\n" +
                    "检查系统 FFmpeg 版本: ffmpeg -version\n" +
                    FFmpegPathResolver.GetHelpMessage(), ex);
            }        
        }
    }

    /// <summary>
    /// 重置初始化状态（仅用于测试）
    /// </summary>
    internal static void Reset()
    {
        lock (_lock)
        {
            _initialized = false;
            _ffmpegRootPath = null;
        }
    }
}
