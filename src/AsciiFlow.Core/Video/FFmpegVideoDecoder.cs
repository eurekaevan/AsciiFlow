using System;
using System.IO;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Text;
using FFmpeg.AutoGen;
using AsciiFlow.Core.Video;

namespace AsciiFlow.Core.Video;

/// <summary>
/// 基于 FFmpeg.AutoGen 8.1.0 的高性能视频解码器
/// 支持 H.264/H.265/VP9 等主流视频格式
/// 修复版本：完整适配 8.1.0 API 变更
/// </summary>
public unsafe class FFmpegVideoDecoder : IVideoDecoder
{
    private AVFormatContext* _formatContext;
    private AVCodecContext* _codecContext;
    private AVFrame* _frame;
    private AVPacket* _packet;
    private SwsContext* _swsContext;

    private string? _videoPath;
    private int _width;
    private int _height;
    private double _frameRate;
    private long _frameCount;
    private long _currentFrame;
    private bool _initialized;
    private bool _disposed;
    private int _streamIndex = -1;

    private byte[]? _frameBuffer;
    private int _frameBufferSize;

    // FFmpeg 错误码常量
    private const int AVERROR_EOF = unchecked((int)0x20464F45); // FFERRTAG('E','O','F',' ')
    private const int AVERROR_EAGAIN = -11;
    private const long AV_TIME_BASE = 1000000;

    public int Width => _width;
    public int Height => _height;
    public double FrameRate => _frameRate;
    public long FrameCount => _frameCount;
    public long CurrentFrame => _currentFrame;
    public bool IsInitialized => _initialized;

    public FFmpegVideoDecoder()
    {
        FFmpegInitializer.Initialize();
    }

    public void Initialize(string videoPath)
    {
        if (_initialized)
            throw new InvalidOperationException("解码器已初始化，请先调用 Reset() 重置");

        if (string.IsNullOrEmpty(videoPath))
            throw new ArgumentNullException(nameof(videoPath), "视频文件路径不能为空");

        if (!File.Exists(videoPath))
            throw new FileNotFoundException($"找不到视频文件：{videoPath}", videoPath);

        try
        {
            _videoPath = videoPath;

            // ===== 修复 CS1503：avformat_open_input 使用正确的字符串编码方式 =====
            // FFmpeg.AutoGen 8.x 需要 null 终止的 UTF-8 字符串
            AVFormatContext* formatContext = null;
            // FFmpeg.AutoGen v8 的签名接收 string，这里直接传入托管字符串以避免类型不匹配
            int ret = ffmpeg.avformat_open_input(
                &formatContext,
                videoPath,
                null,
                null);

            if (ret < 0)
                throw new FFmpegDecoderException($"无法打开视频文件：{videoPath}", ret);

            _formatContext = formatContext;

            // 查找流信息
            int findStreamRet = ffmpeg.avformat_find_stream_info(_formatContext, null);
            if (findStreamRet < 0)
                throw new FFmpegDecoderException("无法获取视频流信息", findStreamRet);

            // 查找视频流
            _streamIndex = FindVideoStream();
            if (_streamIndex < 0)
                throw new FFmpegDecoderException("未找到视频流");

            // 获取解码器参数
            AVStream* videoStream = _formatContext->streams[_streamIndex];
            AVCodecParameters* codecParams = videoStream->codecpar;

            // 查找解码器
            AVCodec* codec = ffmpeg.avcodec_find_decoder(codecParams->codec_id);
            if (codec == null)
                throw new FFmpegDecoderException($"找不到解码器，编解码器ID: {codecParams->codec_id}");

            // 分配解码器上下文
            _codecContext = ffmpeg.avcodec_alloc_context3(codec);
            if (_codecContext == null)
                throw new FFmpegDecoderException("无法分配解码器上下文");

            // 复制编解码器参数
            int copyParamsRet = ffmpeg.avcodec_parameters_to_context(_codecContext, codecParams);
            if (copyParamsRet < 0)
                throw new FFmpegDecoderException("无法复制编解码器参数", copyParamsRet);

            // 打开解码器
            int openCodecRet = ffmpeg.avcodec_open2(_codecContext, codec, null);
            if (openCodecRet < 0)
                throw new FFmpegDecoderException("无法打开解码器", openCodecRet);

            // 获取视频信息
            _width = _codecContext->width;
            _height = _codecContext->height;

            // 获取帧率
            if (videoStream->avg_frame_rate.den > 0)
                _frameRate = (double)videoStream->avg_frame_rate.num / videoStream->avg_frame_rate.den;
            else
                _frameRate = 30.0;

            // 获取总帧数
            if (videoStream->nb_frames > 0)
                _frameCount = videoStream->nb_frames;
            else if (_formatContext->duration > 0 && _frameRate > 0)
                _frameCount = (long)(_formatContext->duration * _frameRate / AV_TIME_BASE);
            else
                _frameCount = 0;

            // 分配帧和包
            _frame = ffmpeg.av_frame_alloc();
            if (_frame == null)
                throw new FFmpegDecoderException("无法分配 AVFrame");

            _packet = ffmpeg.av_packet_alloc();
            if (_packet == null)
                throw new FFmpegDecoderException("无法分配 AVPacket");

            // 创建缩放上下文
            CreateSwsContext();

            // 初始化帧缓冲区
            InitializeFrameBuffer();

            _currentFrame = 0;
            _initialized = true;

            Console.WriteLine($"[解码器] 初始化成功：{_width}x{_height}, {_frameRate:F2} FPS, {_frameCount} 帧");
        }
        catch (Exception ex)
        {
            Cleanup();
            throw new FFmpegDecoderException($"初始化视频解码器失败：{ex.Message}", ex);
        }
    }

    private int FindVideoStream()
    {
        if (_formatContext == null) return -1;
        for (uint i = 0; i < _formatContext->nb_streams; i++)
        {
            AVStream* stream = _formatContext->streams[i];
            if (stream->codecpar->codec_type == AVMediaType.AVMEDIA_TYPE_VIDEO)
                return (int)i;
        }
        return -1;
    }

    /// <summary>
    /// 创建缩放上下文
    /// </summary>
    private void CreateSwsContext()
    {
        if (_codecContext == null) return;

        // ===== 修复 CS0117：SWS_BILINEAR 常量在 8.x 中的写法 =====
        // FFmpeg.AutoGen 8.x 使用枚举或常量 2 代表 SWS_BILINEAR
        const int SWS_BILINEAR = 2;

        _swsContext = ffmpeg.sws_getContext(
            _width, _height,
            _codecContext->pix_fmt,      // 源像素格式
            _width, _height,
            AVPixelFormat.AV_PIX_FMT_RGB24, // 目标 RGB24
            SWS_BILINEAR,                // 缩放算法
            null, null, null);

        if (_swsContext == null)
            throw new FFmpegDecoderException("无法创建 SWS 缩放上下文");
    }

    private void InitializeFrameBuffer()
    {
        _frameBufferSize = _width * _height * 3; // RGB24: 3 bytes per pixel
        _frameBuffer = new byte[_frameBufferSize];
    }

    public byte[]? GetNextFrame()
    {
        if (!_initialized)
            throw new InvalidOperationException("解码器未初始化");
        if (_frameBuffer == null)
            throw new InvalidOperationException("帧缓冲区未初始化");

        try
        {
            while (true)
            {
                // 读取数据包
                int readRet = ffmpeg.av_read_frame(_formatContext, _packet);

                // 处理 EOF
                if (readRet == AVERROR_EOF || readRet < 0)
                    return FlushDecoder();

                // 检查是否是视频流
                if (_packet->stream_index == _streamIndex)
                {
                    // 发送数据包到解码器
                    int sendRet = ffmpeg.avcodec_send_packet(_codecContext, _packet);
                    ffmpeg.av_packet_unref(_packet);

                    // EAGAIN：需要接收帧后才能发送新包
                    if (sendRet == AVERROR_EAGAIN)
                        continue;

                    if (sendRet < 0)
                        throw new FFmpegDecoderException($"发送数据包到解码器失败", sendRet);

                    return ReceiveFrame();
                }

                ffmpeg.av_packet_unref(_packet);
            }
        }
        catch (Exception ex)
        {
            throw new FFmpegDecoderException($"解码视频帧失败：{ex.Message}", ex);
        }
    }

    private byte[]? ReceiveFrame()
    {
        if (_frame == null || _frameBuffer == null)
            return null;

        int receiveRet = ffmpeg.avcodec_receive_frame(_codecContext, _frame);

        if (receiveRet == AVERROR_EAGAIN)
            return null;

        if (receiveRet == AVERROR_EOF || receiveRet < 0)
            return null;

        // 转换像素格式并复制
        CopyFrameToBuffer();

        _currentFrame++;
        return _frameBuffer;
    }

    private byte[]? FlushDecoder()
    {
        if (_codecContext == null) return null;
        try
        {
            ffmpeg.avcodec_send_packet(_codecContext, null);
            return ReceiveFrame();
        }
        catch
        {
            return null;
        }
    }

    /// <summary>
    /// 复制帧数据到 RGB24 缓冲区
    /// </summary>
    private void CopyFrameToBuffer()
    {
        if (_frame == null || _frameBuffer == null || _swsContext == null)
            return;

        // ===== 修复 CS0212：重构 fixed 语句 =====
        // 将 dstData 和 dstLinesize 在 fixed 外部声明
        byte_ptrArray4 dstData = default;
        int_array4 dstLinesize = default;

        fixed (byte* bufferPtr = _frameBuffer)
        {
            dstData[0] = bufferPtr;
            dstLinesize[0] = _width * 3; // RGB24 每行字节数

            ffmpeg.sws_scale(
                _swsContext,
                _frame->data,
                _frame->linesize,
                0,
                _height,
                dstData,
                dstLinesize);
        }
    }

    public void SeekToFrame(long frameNumber)
    {
        if (!_initialized)
            throw new InvalidOperationException("解码器未初始化");
        if (_formatContext == null)
            throw new InvalidOperationException("格式上下文为空");
        if (frameNumber < 0 || (_frameCount > 0 && frameNumber >= _frameCount))
            throw new ArgumentOutOfRangeException(nameof(frameNumber),
                $"帧号 {frameNumber} 超出范围 [0, {_frameCount - 1}]");

        try
        {
            long timestamp = CalculateTimestamp(frameNumber);
            int seekRet = ffmpeg.av_seek_frame(
                _formatContext, _streamIndex,
                timestamp, ffmpeg.AVSEEK_FLAG_BACKWARD);

            if (seekRet < 0)
                throw new FFmpegDecoderException($"跳转到帧 {frameNumber} 失败", seekRet);

            ffmpeg.avcodec_flush_buffers(_codecContext);
            _currentFrame = frameNumber;
        }
        catch (Exception ex)
        {
            throw new FFmpegDecoderException($"跳转失败：{ex.Message}", ex);
        }
    }

    private long CalculateTimestamp(long frameNumber)
    {
        if (_frameRate <= 0) return 0;
        double seconds = frameNumber / _frameRate;
        return (long)(seconds * AV_TIME_BASE);
    }

    public VideoInfo GetVideoInfo()
    {
        if (!_initialized)
            throw new InvalidOperationException("解码器未初始化");

        string codecName = _codecContext != null && _codecContext->codec != null
            ? Marshal.PtrToStringAnsi((IntPtr)_codecContext->codec->name) ?? "unknown"
            : "unknown";

        string pixelFormat = _codecContext != null
            ? ffmpeg.av_get_pix_fmt_name(_codecContext->pix_fmt).ToString()
            : "unknown";

        return new VideoInfo(_width, _height, _frameRate, _frameCount, codecName, pixelFormat);
    }

    public void Reset()
    {
        if (_initialized)
            Cleanup();
        if (!string.IsNullOrEmpty(_videoPath))
            Initialize(_videoPath);
    }

    private void Cleanup()
    {
        fixed (AVFrame** pframe = &_frame)
        {
            if (*pframe != null)
            {
                ffmpeg.av_frame_free(pframe);
            }
        }
        _frame = null;

        fixed (AVPacket** ppacket = &_packet)
        {
            if (*ppacket != null)
            {
                ffmpeg.av_packet_free(ppacket);
            }
        }
        _packet = null;

        if (_swsContext != null)
        {
            ffmpeg.sws_freeContext(_swsContext);
            _swsContext = null;
        }

        fixed (AVCodecContext** pcodecContext = &_codecContext)
        {
            if (*pcodecContext != null)
            {
                ffmpeg.avcodec_free_context(pcodecContext);
            }
        }
        _codecContext = null;

        fixed (AVFormatContext** pformatContext = &_formatContext)
        {
            if (*pformatContext != null)
            {
                ffmpeg.avformat_close_input(pformatContext);
            }
        }
        _formatContext = null;

        _initialized = false;
    }

    public void Dispose()
    {
        Dispose(true);
        GC.SuppressFinalize(this);
    }

    protected virtual void Dispose(bool disposing)
    {
        if (_disposed) return;
        Cleanup();
        if (disposing)
        {
            _frameBuffer = null;
            _videoPath = null;
        }
        _disposed = true;
    }

    ~FFmpegVideoDecoder()
    {
        Dispose(false);
    }
}