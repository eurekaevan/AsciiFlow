using System;
using System.IO;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Text;
using FFmpeg.AutoGen;

namespace AsciiFlow.Core.Encoding;

/// <summary>
/// 基于 FFmpeg.AutoGen 8.1.0 的 H.264 视频编码器
/// 性能目标：~20ms/帧（1080p，CRF 23）
/// </summary>
public unsafe class FFmpegVideoEncoder : IVideoEncoder
{
    // FFmpeg 上下文
    private AVFormatContext* _formatContext;
    private AVCodecContext* _codecContext;
    private SwsContext* _swsContext;

    // 帧和包
    private AVFrame* _yuvFrame;
    private byte* _yuvBuffer;  // YUV420P 帧数据缓冲区
    private AVPacket* _packet;

    // 配置
    private int _width;
    private int _height;
    private double _frameRate;
    private long _encodedFrames;
    private bool _initialized;
    private bool _disposed;
    private string _outputPath;

    // FFmpeg 错误码常量
    private const int AVERROR_EAGAIN = -11;
    private const int AVERROR_EOF = unchecked((int)0x20464F45);  // ← 新增
    private const int AV_TIME_BASE = 1000000;

    // 编码器配置
    private const string CodecName = "libx264";
    private const int DefaultCRF = 23;
    private const string DefaultPreset = "fast";  // fast 预设兼顾速度和质量

    /// <summary>编码器是否已初始化</summary>
    public bool IsInitialized => _initialized;

    /// <summary>输出视频宽度（像素）</summary>
    public int Width => _width;

    /// <summary>输出视频高度（像素）</summary>
    public int Height => _height;

    /// <summary>视频帧率（fps）</summary>
    public double FrameRate => _frameRate;

    /// <summary>已编码帧数</summary>
    public long EncodedFrames => _encodedFrames;

    public FFmpegVideoEncoder()
    {
        FFmpeg.AutoGen.ffmpeg.av_log_set_level(FFmpeg.AutoGen.ffmpeg.AV_LOG_QUIET);
        // Ensure non-nullable fields are initialized to satisfy the compiler
        _outputPath = string.Empty;
    }

    /// <summary>
    /// 初始化编码器
    /// </summary>
    public void Initialize(string outputPath, int width, int height, double frameRate = 30.0)
    {
        if (_initialized)
            throw new InvalidOperationException("编码器已初始化");

        if (string.IsNullOrEmpty(outputPath))
            throw new ArgumentNullException(nameof(outputPath));

        if (width <= 0 || height <= 0)
            throw new ArgumentOutOfRangeException($"尺寸不合法: {width}x{height}");

        _outputPath = outputPath;
        _width = width;
        _height = height;
        _frameRate = frameRate;

        try
        {
            // 1. 创建输出格式上下文
            CreateOutputFormatContext();

            // 2. 创建编码器
            CreateEncoder();

            // 3. 创建 RGB→YUV 格式转换上下文
            CreateSwsContext();

            // 4. 分配 YUV 帧
            AllocateYUVFrame();

            // 5. 写入文件头（MP4 容器头）
            int headerRet = ffmpeg.avformat_write_header(_formatContext, null);
            if (headerRet < 0)
                throw new FFmpegEncoderException("无法写入视频文件头", headerRet);

            _initialized = true;
            _encodedFrames = 0;

            Console.WriteLine($"[编码器] 初始化完成: {outputPath}");
            Console.WriteLine($"[编码器] 分辨率: {width}x{height}, 帧率: {frameRate:F2} fps");
            Console.WriteLine($"[编码器] 编码: {CodecName}, CRF: {DefaultCRF}, 预设: {DefaultPreset}");
        }
        catch (Exception ex)
        {
            Cleanup();
            throw new FFmpegEncoderException($"初始化编码器失败: {ex.Message}", ex);
        }
    }

    /// <summary>
    /// 创建输出格式上下文（MP4 容器）
    /// </summary>
    private void CreateOutputFormatContext()
    {
        AVFormatContext* formatContext = null;
        // 创建输出格式上下文
        int ret = ffmpeg.avformat_alloc_output_context2(
            &formatContext,
            null,
            null,
            _outputPath);

        if (ret < 0 || formatContext == null)
            throw new FFmpegEncoderException("无法创建输出格式上下文", ret);

        _formatContext = formatContext;

        // 打开输出文件
        if ((_formatContext->oformat->flags & ffmpeg.AVFMT_NOFILE) == 0)
        {
            // 直接使用字符串重载以避免指针/字符串转换问题
            ret = ffmpeg.avio_open(
                (AVIOContext**)&_formatContext->pb,
                _outputPath,
                ffmpeg.AVIO_FLAG_WRITE);

            if (ret < 0)
                throw new FFmpegEncoderException("无法打开输出文件", ret);
        }
    }

    /// <summary>
    /// 创建 H.264 编码器
    /// </summary>
    private void CreateEncoder()
    {
        // 查找 libx264 编码器
        AVCodec* codec = ffmpeg.avcodec_find_encoder_by_name(CodecName);
        if (codec == null)
            throw new FFmpegEncoderException($"找不到编码器: {CodecName}，请确保 FFmpeg 包含 libx264");

        // 创建视频流
        AVStream* stream = ffmpeg.avformat_new_stream(_formatContext, null);
        if (stream == null)
            throw new FFmpegEncoderException("无法创建视频流");

        // 分配编码器上下文
        _codecContext = ffmpeg.avcodec_alloc_context3(codec);
        if (_codecContext == null)
            throw new FFmpegEncoderException("无法分配编码器上下文");

        // 配置编码器参数
        _codecContext->width = _width;
        _codecContext->height = _height;
        _codecContext->pix_fmt = AVPixelFormat.AV_PIX_FMT_YUV420P;
        _codecContext->time_base = new AVRational { num = 1, den = (int)_frameRate };
        _codecContext->framerate = new AVRational { num = (int)_frameRate, den = 1 };

        // GOP 结构：每 2 秒一个关键帧
        _codecContext->gop_size = (int)(_frameRate * 2);
        _codecContext->max_b_frames = 2;
        _codecContext->bit_rate = 4_000_000;  // 4 Mbps 基础码率

        // 设置 H.264 特定选项（CRF + 预设）
        AVDictionary* options = null;
        SetDictionaryOption(ref options, "preset", DefaultPreset);
        SetDictionaryOption(ref options, "crf", DefaultCRF.ToString());
        SetDictionaryOption(ref options, "tune", "fastdecode");  // 针对快速解码优化

        // 打开编码器
        int ret = ffmpeg.avcodec_open2(_codecContext, codec, &options);
        if (options != null)
            ffmpeg.av_dict_free(&options);

        if (ret < 0)
            throw new FFmpegEncoderException($"无法打开编码器: {CodecName}", ret);

        // 复制编码参数到流
        ret = ffmpeg.avcodec_parameters_from_context(stream->codecpar, _codecContext);
        if (ret < 0)
            throw new FFmpegEncoderException("无法复制编码参数到流", ret);

        // 设置流的 time_base
        stream->time_base = _codecContext->time_base;
    }

    /// <summary>
    /// 设置字典选项
    /// </summary>
    private static void SetDictionaryOption(ref AVDictionary* dict, string key, string value)
    {
        // Use the string overload to avoid unsafe pointer conversions
        fixed (AVDictionary** dictPtr = &dict)
        {
            ffmpeg.av_dict_set(dictPtr, key, value, 0);
        }
    }

    /// <summary>
    /// 创建 RGB24 → YUV420P 格式转换上下文
    /// </summary>
    private void CreateSwsContext()
    {
        const int SWS_FAST_BILINEAR = 1;

        _swsContext = ffmpeg.sws_getContext(
            _width, _height, AVPixelFormat.AV_PIX_FMT_RGB24,
            _width, _height, AVPixelFormat.AV_PIX_FMT_YUV420P,
            SWS_FAST_BILINEAR,
            null, null, null);

        if (_swsContext == null)
            throw new FFmpegEncoderException("无法创建格式转换上下文");
    }

    /// <summary>
    /// 分配 YUV420P 帧
    /// </summary>
    private void AllocateYUVFrame()
    {
        _yuvFrame = ffmpeg.av_frame_alloc();
        if (_yuvFrame == null)
            throw new FFmpegEncoderException("无法分配 YUV 帧");

        _yuvFrame->format = (int)AVPixelFormat.AV_PIX_FMT_YUV420P;
        _yuvFrame->width = _width;
        _yuvFrame->height = _height;

        // 分配帧缓冲区
        int bufferSize = ffmpeg.av_image_get_buffer_size(
            AVPixelFormat.AV_PIX_FMT_YUV420P, _width, _height, 1);

        _yuvBuffer = (byte*)ffmpeg.av_malloc((ulong)bufferSize);
        if (_yuvBuffer == null)
            throw new FFmpegEncoderException("无法分配 YUV 缓冲区");

        // 填充帧数据指针
        byte_ptrArray4 yuvData = default;
        int_array4 yuvLinesize = default;

        int ret = ffmpeg.av_image_fill_arrays(
            ref yuvData,
            ref yuvLinesize,
            _yuvBuffer,
            AVPixelFormat.AV_PIX_FMT_YUV420P,
            _width, _height, 1);

        if (ret < 0)
            throw new FFmpegEncoderException("无法填充 YUV 帧数组", ret);

        // 将数据指针赋值给帧（通过逐平面拷贝）
        for (uint i = 0; i < 4; i++)
        {
            _yuvFrame->data[i] = (byte*)yuvData[i];
            _yuvFrame->linesize[i] = yuvLinesize[i];
        }

        // 分配数据包
        _packet = ffmpeg.av_packet_alloc();
        if (_packet == null)
            throw new FFmpegEncoderException("无法分配数据包");
    }

    /// <summary>
    /// 编码一帧 RGB24 数据
    /// </summary>
    public void EncodeFrame(byte[] rgbData)
    {
        if (!_initialized)
            throw new InvalidOperationException("编码器未初始化");

        if (rgbData.Length != _width * _height * 3)
            throw new ArgumentException(
                $"RGB 数据长度错误: 期望 {_width * _height * 3}, 实际 {rgbData.Length}");

        // 1. RGB24 → YUV420P 格式转换
        ConvertRGBToYUV(rgbData);

        // 2. 设置帧时间戳
        _yuvFrame->pts = _encodedFrames;
        _encodedFrames++;

        // 3. 发送到编码器
        int sendRet = ffmpeg.avcodec_send_frame(_codecContext, _yuvFrame);
        if (sendRet == AVERROR_EAGAIN)
        {
            // 先读取编码后的包
            ReceiveAndWritePackets();
            sendRet = ffmpeg.avcodec_send_frame(_codecContext, _yuvFrame);
        }
        if (sendRet < 0)
            throw new FFmpegEncoderException("发送帧到编码器失败", sendRet);

        // 4. 接收并写入编码包
        ReceiveAndWritePackets();
    }

    /// <summary>
    /// RGB24 → YUV420P 格式转换
    /// </summary>
    [MethodImpl(MethodImplOptions.AggressiveInlining)]
    private void ConvertRGBToYUV(byte[] rgbData)
    {
        fixed (byte* rgbPtr = rgbData)
        {
            // 构造源数据指针数组
            byte_ptrArray4 srcData = new byte_ptrArray4();
            int_array4 srcLinesize = new int_array4();
            srcData[0] = rgbPtr;
            srcLinesize[0] = _width * 3;

            // 调用 SWS 进行格式转换
            ffmpeg.sws_scale(
                _swsContext,
                srcData,
                srcLinesize,
                0,
                _height,
                _yuvFrame->data,
                _yuvFrame->linesize);
        }
    }

    /// <summary>
    /// 接收编码包并写入文件
    /// 修复：正确处理 EOF（flush 阶段正常返回）
    /// </summary>
    private void ReceiveAndWritePackets()
    {
        while (true)
        {
            int ret = ffmpeg.avcodec_receive_packet(_codecContext, _packet);

            if (ret == AVERROR_EAGAIN)
                break;  // 需要更多帧

            // 【修复】EOF 在 flush 阶段是正常的，不是错误
            if (ret == AVERROR_EOF)
                break;  // flush 完成

            if (ret < 0)
                throw new FFmpegEncoderException("接收编码包失败", ret);

            // 写入文件
            WritePacketToFile();

            // 释放包
            ffmpeg.av_packet_unref(_packet);
        }
    }
    /// <summary>
    /// 将数据包写入文件
    /// </summary>
    [MethodImpl(MethodImplOptions.AggressiveInlining)]
    private void WritePacketToFile()
    {
        AVStream* stream = _formatContext->streams[0];

        // 转换时间基准
        _packet->stream_index = stream->index;
        ffmpeg.av_packet_rescale_ts(_packet, _codecContext->time_base, stream->time_base);

        // 写入容器
        int ret = ffmpeg.av_interleaved_write_frame(_formatContext, _packet);
        if (ret < 0)
            throw new FFmpegEncoderException("写入视频包失败", ret);
    }

    /// <summary>
    /// 完成编码：刷新编码器缓冲区 + 写入文件尾
    /// </summary>
    public void Finish()
    {
        if (!_initialized) return;

        Console.WriteLine("[编码器] 正在完成编码...");

        try
        {
            // 1. 刷新编码器（发送 null 帧表示结束）
            int flushRet = ffmpeg.avcodec_send_frame(_codecContext, null);
            
            // 尝试接收 flush 出的包（EOF 是正常的）
            ReceiveAndWritePackets();

            // 2. 写入文件尾（MP4 索引等）- 必须调用！
            int ret = ffmpeg.av_write_trailer(_formatContext);
            if (ret < 0)
                throw new FFmpegEncoderException("写入视频文件尾失败", ret);

            Console.WriteLine($"[编码器] 编码完成: 共 {_encodedFrames} 帧");
            if (_encodedFrames > 0)
            {
                var file = new FileInfo(_outputPath);
                double fileSizeMB = file.Length / (1024.0 * 1024.0);
                Console.WriteLine($"[编码器] 输出文件: {_outputPath} ({fileSizeMB:F2} MB)");
            }
        }
        catch (Exception ex)
        {
            Console.WriteLine($"[编码器错误] Finish 失败: {ex.Message}");
            // 即使出错也要清理资源
        }
        finally
        {
            _initialized = false;
            Cleanup();
        }
    }
    /// <summary>
    /// 清理 FFmpeg 资源
    /// </summary>
    private void Cleanup()
    {
        // 关闭输出文件
        if (_formatContext != null)
        {
            if ((_formatContext->oformat->flags & ffmpeg.AVFMT_NOFILE) == 0)
            {
                ffmpeg.avio_closep((AVIOContext**)&_formatContext->pb);
            }
            ffmpeg.avformat_free_context(_formatContext);
            _formatContext = null;
        }

        if (_codecContext != null)
        {
            // Need to pin the field address before passing to avcodec_free_context
            fixed (AVCodecContext** codecCtxPtr = &_codecContext)
            {
                ffmpeg.avcodec_free_context(codecCtxPtr);
            }
            _codecContext = null;
        }

        if (_swsContext != null)
        {
            ffmpeg.sws_freeContext(_swsContext);
            _swsContext = null;
        }

        if (_yuvFrame != null)
        {
            fixed (AVFrame** framePtr = &_yuvFrame)
            {
                ffmpeg.av_frame_free(framePtr);
            }
            _yuvFrame = null;
        }

        if (_yuvBuffer != null)
        {
            ffmpeg.av_free(_yuvBuffer);
            _yuvBuffer = null;
        }

        if (_packet != null)
        {
            fixed (AVPacket** packetPtr = &_packet)
            {
                ffmpeg.av_packet_free(packetPtr);
            }
            _packet = null;
        }
    }

    /// <summary>
    /// 释放资源
    /// </summary>
    public void Dispose()
    {
        if (_disposed) return;

        if (_initialized)
        {
            Finish();
        }
        else
        {
            Cleanup();
        }

        _disposed = true;
    }
}

/// <summary>
/// 编码器异常
/// </summary>
public class FFmpegEncoderException : Exception
{
    public int ErrorCode { get; }

    public FFmpegEncoderException(string message) : base(message) { }

    public FFmpegEncoderException(string message, int errorCode)
        : base($"{message} (错误码: {errorCode})")
    {
        ErrorCode = errorCode;
    }

    public FFmpegEncoderException(string message, Exception inner)
        : base(message, inner) { }
}