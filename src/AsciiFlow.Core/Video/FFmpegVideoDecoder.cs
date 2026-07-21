using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using FFmpeg.AutoGen;

namespace AsciiFlow.Core.Video;

/// <summary>
/// 基于 FFmpeg.AutoGen 8.1.0 的高性能视频解码器
/// 修复版本：正确处理 H.264 B 帧延迟，修复所有编译错误
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

    // flush 模式标志（用于处理 H.264 解码延迟）
    private bool _inFlushMode = false;

    // FFmpeg 错误码常量
    private const int AVERROR_EOF = unchecked((int)0x20464F45);
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
            _inFlushMode = false;

            // ===== 打开视频文件 =====
            AVFormatContext* formatContext = null;
            int ret = ffmpeg.avformat_open_input(
                &formatContext,
                videoPath,
                null,
                null);

            if (ret < 0)
                throw new FFmpegDecoderException($"无法打开视频文件：{videoPath}", ret);

            _formatContext = formatContext;

            // ===== 查找流信息 =====
            int findStreamRet = ffmpeg.avformat_find_stream_info(_formatContext, null);
            if (findStreamRet < 0)
                throw new FFmpegDecoderException("无法获取视频流信息", findStreamRet);

            // ===== 查找视频流与音频流 =====
            _streamIndex = FindVideoStream();
            if (_streamIndex < 0)
                throw new FFmpegDecoderException("未找到视频流");

            _audioStreamIndex = FindAudioStream();

            AVStream* videoStream = _formatContext->streams[_streamIndex];
            AVCodecParameters* codecParams = videoStream->codecpar;

            // ===== 查找解码器 =====
            AVCodec* codec = ffmpeg.avcodec_find_decoder(codecParams->codec_id);
            if (codec == null)
                throw new FFmpegDecoderException($"找不到解码器，编解码器ID: {codecParams->codec_id}");

            // ===== 分配解码器上下文 =====
            _codecContext = ffmpeg.avcodec_alloc_context3(codec);
            if (_codecContext == null)
                throw new FFmpegDecoderException("无法分配解码器上下文");

            int copyParamsRet = ffmpeg.avcodec_parameters_to_context(_codecContext, codecParams);
            if (copyParamsRet < 0)
                throw new FFmpegDecoderException("无法复制编解码器参数", copyParamsRet);

            int openCodecRet = ffmpeg.avcodec_open2(_codecContext, codec, null);
            if (openCodecRet < 0)
                throw new FFmpegDecoderException("无法打开解码器", openCodecRet);

            // ===== 获取视频信息 =====
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

            // ===== 分配帧和包 =====
            _frame = ffmpeg.av_frame_alloc();
            if (_frame == null)
                throw new FFmpegDecoderException("无法分配 AVFrame");

            _packet = ffmpeg.av_packet_alloc();
            if (_packet == null)
                throw new FFmpegDecoderException("无法分配 AVPacket");

            // ===== 创建缩放上下文 =====
            const int SWS_BILINEAR = 2;
            _swsContext = ffmpeg.sws_getContext(
                _width, _height,
                _codecContext->pix_fmt,
                _width, _height,
                AVPixelFormat.AV_PIX_FMT_RGB24,
                SWS_BILINEAR,
                null, null, null);

            if (_swsContext == null)
                throw new FFmpegDecoderException("无法创建 SWS 缩放上下文");

            // ===== 初始化帧缓冲区 =====
            _frameBuffer = new byte[_width * _height * 3];

            _currentFrame = 0;
            _initialized = true;

            // ✓ 使用下划线访问私有字段
            Console.WriteLine($"[解码器] 初始化成功：{_width}x{_height}, {_frameRate:F2} FPS, {_frameCount} 帧");
        }
        catch (Exception ex)
        {
            Cleanup();
            throw new FFmpegDecoderException($"初始化视频解码器失败：{ex.Message}", ex);
        }
    }

    private int _audioStreamIndex = -1;

    public int AudioStreamIndex => _audioStreamIndex;

    public delegate void AudioPacketHandler(AVPacket* packet, AVStream* stream);
    public AudioPacketHandler? OnAudioPacket;

    public AVStream* GetAudioStream()
    {
        if (_formatContext == null || _audioStreamIndex < 0) return null;
        return _formatContext->streams[_audioStreamIndex];
    }

    private int FindAudioStream()
    {
        if (_formatContext == null) return -1;
        for (uint i = 0; i < _formatContext->nb_streams; i++)
        {
            AVStream* stream = _formatContext->streams[i];
            if (stream->codecpar->codec_type == AVMediaType.AVMEDIA_TYPE_AUDIO)
                return (int)i;
        }
        return -1;
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

    // ==================================================================
    // 核心解码接口（循环式，正确处理 H.264 B 帧延迟）
    // ==================================================================

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
                // ① 首先尝试从解码器接收一帧（可能之前 send 的包已经产出帧）
                byte[]? existing = TryReceiveFrame();
                if (existing != null)
                    return existing;

                // ② 如果已进入 flush 模式（EOF 后），继续 flush 直到无帧
                if (_inFlushMode)
                {
                    int flushRet = ffmpeg.avcodec_send_packet(_codecContext, null);

                    byte[]? flushFrame = TryReceiveFrame();
                    if (flushFrame != null)
                        return flushFrame;

                    // 真正的 EOF，彻底结束
                    return null;
                }

                // ③ 从容器读取下一个包
                int readRet = ffmpeg.av_read_frame(_formatContext, _packet);

                if (readRet < 0)
                {
                    // 容器 EOF 或读取出错 → 进入 flush 模式
                    _inFlushMode = true;
                    ffmpeg.avcodec_send_packet(_codecContext, null);
                    continue;
                }

                // ④ 处理音频包：将原音频数据包透传通知
                if (_packet->stream_index == _audioStreamIndex)
                {
                    if (_formatContext != null && _audioStreamIndex >= 0)
                    {
                        OnAudioPacket?.Invoke(_packet, _formatContext->streams[_audioStreamIndex]);
                    }
                    ffmpeg.av_packet_unref(_packet);
                    continue;
                }

                // 只处理视频流的包（跳过字幕等其他包）
                if (_packet->stream_index != _streamIndex)
                {
                    ffmpeg.av_packet_unref(_packet);
                    continue;
                }

                // ⑤ 发送视频包到解码器
                int sendRet = ffmpeg.avcodec_send_packet(_codecContext, _packet);
                ffmpeg.av_packet_unref(_packet);

                // EAGAIN：解码器缓冲区满，需要先接收帧才能继续发送
                if (sendRet == AVERROR_EAGAIN)
                {
                    continue;
                }

                // 其他错误
                if (sendRet < 0)
                    throw new FFmpegDecoderException("发送数据包到解码器失败", sendRet);

                // 发送成功，回到 ① 尝试接收帧
            }
        }
        catch (Exception ex)
        {
            if (ex is FFmpegDecoderException) throw;
            throw new FFmpegDecoderException($"解码视频帧失败：{ex.Message}", ex);
        }
    }

    private byte[]? TryReceiveFrame()
    {
        if (_frame == null || _frameBuffer == null)
            return null;

        int receiveRet = ffmpeg.avcodec_receive_frame(_codecContext, _frame);

        // 需要更多输入数据 —— 不是错误，也不是 EOF
        if (receiveRet == AVERROR_EAGAIN)
            return null;

        // 真正的 EOF 或错误
        if (receiveRet == AVERROR_EOF || receiveRet < 0)
            return null;

        // 成功收到一帧，转换像素格式
        CopyFrameToBuffer();
        _currentFrame++;
        return _frameBuffer;
    }

    /// <summary>
    /// 复制帧数据到 RGB24 缓冲区（修复版：正确处理 fixed 语句）
    /// </summary>
    private void CopyFrameToBuffer()
    {
        if (_frame == null || _frameBuffer == null || _swsContext == null)
            return;

        // 【关键修复】在 fixed 内部声明并初始化 dstData/dstLinesize
        // 避免 CS0212 "只能获取 fixed 语句初始值设定项内的未固定表达式的地址"
        // 避免 CS8887 "使用了未赋值的局部变量"
        fixed (byte* bufferPtr = _frameBuffer)
        {
            byte_ptrArray4 dstData = default;
            int_array4 dstLinesize = default;
            dstData[0] = bufferPtr;
            dstLinesize[0] = _width * 3; // RGB24 每行字节数

            // 源数据来自 _frame->data (已是稳定的帧内数据，可直接传)
            ffmpeg.sws_scale(
                _swsContext,
                _frame->data,       // byte_ptrArray8 (源)
                _frame->linesize,   // int_array8 (源)
                0,                  // slice Y start
                _height,
                dstData,            // byte_ptrArray4 (目标)
                dstLinesize);       // int_array4 (目标)
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
            long timestamp = (long)((frameNumber / _frameRate) * AV_TIME_BASE);
            int seekRet = ffmpeg.av_seek_frame(
                _formatContext, _streamIndex,
                timestamp, ffmpeg.AVSEEK_FLAG_BACKWARD);

            if (seekRet < 0)
                throw new FFmpegDecoderException($"跳转到帧 {frameNumber} 失败", seekRet);

            ffmpeg.avcodec_flush_buffers(_codecContext);

            _currentFrame = frameNumber;
            _inFlushMode = false;   // 重置 flush 模式
        }
        catch (Exception ex)
        {
            throw new FFmpegDecoderException($"跳转失败：{ex.Message}", ex);
        }
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
        {
            _inFlushMode = false;
            Initialize(_videoPath);
        }
    }

    private void Cleanup()
    {
        if (_frame != null)
        {
            AVFrame* frame = _frame;
            ffmpeg.av_frame_free(&frame);
            _frame = null;
        }

        if (_packet != null)
        {
            AVPacket* packet = _packet;
            ffmpeg.av_packet_free(&packet);
            _packet = null;
        }

        if (_swsContext != null)
        {
            ffmpeg.sws_freeContext(_swsContext);
            _swsContext = null;
        }

        if (_codecContext != null)
        {
            AVCodecContext* codecContext = _codecContext;
            ffmpeg.avcodec_free_context(&codecContext);
            _codecContext = null;
        }

        if (_formatContext != null)
        {
            AVFormatContext* formatContext = _formatContext;
            ffmpeg.avformat_close_input(&formatContext);
            _formatContext = null;
        }

        _initialized = false;
        _inFlushMode = false;
    }

    public void Dispose()
    {
        Dispose(true);
        GC.SuppressFinalize(this);
    }

    protected virtual void Dispose(bool disposing)
    {
        if (_disposed) return;
        if (disposing)
        {
            Cleanup();
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