use super::{
    audio::{AudioOutputTemplate, AudioPacketSender, MuxMessage},
    codec::{again, check, ffmpeg_error},
    ffi,
    frame::Frame,
    hwdevice::HardwareDevice,
    hwframes::{HardwareFrame, HardwareFramesPool, VaapiEncoderFrame, VaapiEncoderFrames},
    packet::Packet,
    vaapi::{EncodeMode, VaapiOptions},
};
use asciiflow_core::{
    CancellationToken, CapabilitySupport, ChromaLocation, ColorMatrix, ColorPrimaries, ColorRange,
    EncodeDiagnostics, Error, FrameDesc, FrameSink, HostFrame, PipelineStage, PixelFormat,
    Rational, Result, SinkTimings, TransferCharacteristic, VideoCodec, VideoFrame,
};
use crossbeam_channel::{Receiver, SendTimeoutError, Sender, bounded};
use std::{
    ffi::CString,
    path::Path,
    ptr::{self, NonNull},
    sync::{Arc, Mutex},
    thread::JoinHandle,
    time::{Duration, Instant},
};

const MUX_CHANNEL_CAPACITY: usize = 64;
const MUX_POLL: Duration = Duration::from_millis(20);

pub struct Encoder {
    codec: NonNull<ffi::AVCodecContext>,
    video_stream_index: i32,
    video_time_base: ffi::AVRational,
    frame: Frame,
    hardware_frame: Option<HardwareFrame>,
    frames_pool: Option<HardwareFramesPool>,
    _hardware_device: Option<HardwareDevice>,
    packet: Packet,
    mode: EncodeMode,
    output_codec: VideoCodec,
    timings: SinkTimings,
    desc: FrameDesc,
    next_pts: i64,
    finished: bool,
    finish_attempted: bool,
    mux_sender: Sender<MuxMessage>,
    mux_thread: Option<JoinHandle<Result<()>>>,
    mux_failure: Arc<Mutex<Option<String>>>,
    mux_stats: Arc<Mutex<MuxStats>>,
    mux_stop: CancellationToken,
    cancellation: CancellationToken,
    #[cfg(feature = "encode-characterization")]
    submitted_total: u64,
    #[cfg(feature = "encode-characterization")]
    packets_total: u64,
    #[cfg(test)]
    inject_send_failure: bool,
    #[cfg(test)]
    inject_receive_failure: bool,
}

#[derive(Default)]
struct MuxStats {
    audio_passthrough: Duration,
    audio_packets: u64,
    audio_bytes: u64,
    diagnostics: EncodeDiagnostics,
}

struct AudioMuxRoute {
    input_index: usize,
    output_index: i32,
    output_time_base: ffi::AVRational,
}

struct MuxOutput {
    format: NonNull<ffi::AVFormatContext>,
    video_stream_index: i32,
    video_time_base: ffi::AVRational,
    audio_routes: Vec<AudioMuxRoute>,
}

struct EncoderCreateOptions {
    codec: VideoCodec,
    mode: EncodeMode,
    vaapi: VaapiOptions,
    require_host_upload: bool,
    audio: Vec<AudioOutputTemplate>,
    cancellation: CancellationToken,
}

unsafe impl Send for MuxOutput {}

pub struct VaapiEncoderProbe {
    pub frames: VaapiEncoderFrames,
    pub host_upload: CapabilitySupport,
}

pub struct OutputEncoding {
    pub codec: VideoCodec,
    pub mode: EncodeMode,
}

pub fn probe_vaapi_encoder(
    desc: FrameDesc,
    frame_rate: Rational,
    vaapi: VaapiOptions,
) -> Result<VaapiEncoderProbe> {
    probe_vaapi_encoder_for(VideoCodec::H264, desc, frame_rate, vaapi)
}

pub fn probe_vaapi_encoder_for(
    output_codec: VideoCodec,
    desc: FrameDesc,
    frame_rate: Rational,
    vaapi: VaapiOptions,
) -> Result<VaapiEncoderProbe> {
    probe_vaapi_encoder_internal(output_codec, desc, frame_rate, vaapi)
}

/// Retained for the opt-in descriptor qualification test. Production probing
/// uses the same codec-specific configuration.
#[cfg(feature = "av1-encode-diagnostic")]
pub fn probe_vaapi_av1_encoder_diagnostic(
    desc: FrameDesc,
    frame_rate: Rational,
    vaapi: VaapiOptions,
) -> Result<VaapiEncoderProbe> {
    probe_vaapi_encoder_internal(VideoCodec::Av1, desc, frame_rate, vaapi)
}

fn probe_vaapi_encoder_internal(
    output_codec: VideoCodec,
    desc: FrameDesc,
    frame_rate: Rational,
    vaapi: VaapiOptions,
) -> Result<VaapiEncoderProbe> {
    require_supported_output(&output_codec, &desc, EncodeMode::Vaapi)?;
    let codec_name = output_codec_name(&output_codec)?;
    let width = i32::try_from(desc.width)
        .map_err(|_| Error::Media("encoder probe width exceeds FFmpeg i32 range".into()))?;
    let height = i32::try_from(desc.height)
        .map_err(|_| Error::Media("encoder probe height exceeds FFmpeg i32 range".into()))?;
    let name = CString::new(format!("{codec_name}_vaapi")).unwrap();
    let encoder = unsafe { ffi::avcodec_find_encoder_by_name(name.as_ptr()) };
    if encoder.is_null() {
        return Err(asciiflow_core::Error::Media(format!(
            "this FFmpeg build has no {codec_name}_vaapi encoder"
        )));
    }
    require_output_codec_id(encoder, &output_codec)?;
    require_vaapi_encoder(encoder)?;
    let codec = NonNull::new(unsafe { ffi::avcodec_alloc_context3(encoder) }).ok_or_else(|| {
        asciiflow_core::Error::Media(format!(
            "failed to allocate {output_codec} encoder probe context"
        ))
    })?;
    let _guard = CodecGuard(Some(codec));
    unsafe {
        (*codec.as_ptr()).codec_id = (*encoder).id;
        (*codec.as_ptr()).codec_type = ffi::AVMediaType::AVMEDIA_TYPE_VIDEO;
        (*codec.as_ptr()).width = width;
        (*codec.as_ptr()).height = height;
        (*codec.as_ptr()).pix_fmt = ffi::AVPixelFormat::AV_PIX_FMT_VAAPI;
        (*codec.as_ptr()).time_base = ffi::AVRational {
            num: frame_rate.denominator,
            den: frame_rate.numerator,
        };
        (*codec.as_ptr()).framerate = ffi::AVRational {
            num: frame_rate.numerator,
            den: frame_rate.denominator,
        };
        (*codec.as_ptr()).color_range = output_color_range(&desc);
        (*codec.as_ptr()).colorspace = ffi::AVColorSpace::AVCOL_SPC_BT709;
        (*codec.as_ptr()).color_primaries = ffi::AVColorPrimaries::AVCOL_PRI_BT709;
        (*codec.as_ptr()).color_trc = ffi::AVColorTransferCharacteristic::AVCOL_TRC_BT709;
        (*codec.as_ptr()).chroma_sample_location = ffi::AVChromaLocation::AVCHROMA_LOC_LEFT;
        if output_codec == VideoCodec::Hevc {
            (*codec.as_ptr()).profile = hevc_profile(desc.format);
        } else if output_codec == VideoCodec::Av1 {
            (*codec.as_ptr()).global_quality = 25;
        }
        (*codec.as_ptr()).max_b_frames = 0;
        (*codec.as_ptr()).gop_size = 250;
    }
    let device = vaapi.create_device()?;
    let pool = encoder_pool(&device, &desc)?;
    unsafe { (*codec.as_ptr()).hw_frames_ctx = pool.try_clone_ref()? };
    let mut options = ptr::null_mut();
    let options_for_codec = vaapi_codec_options(&output_codec);
    for &(key, value) in options_for_codec {
        let key = CString::new(key).unwrap();
        let value = CString::new(value).unwrap();
        check(
            unsafe { ffi::av_dict_set(&mut options, key.as_ptr(), value.as_ptr(), 0) },
            "failed to set encoder probe option",
        )?;
    }
    let opened = unsafe { ffi::avcodec_open2(codec.as_ptr(), encoder, &mut options) };
    let unused_options = unsafe { ffi::av_dict_count(options) };
    unsafe { ffi::av_dict_free(&mut options) };
    check(
        opened,
        &format!(
            "failed to configure {codec_name}_vaapi encoder probe for {}x{} {:?}",
            desc.width, desc.height, desc.format
        ),
    )?;
    if unused_options != 0 {
        return Err(asciiflow_core::Error::Media(format!(
            "{output_codec} encoder probe rejected {unused_options} configuration option(s)"
        )));
    }
    let host_upload = match pool_supports_upload(&pool, desc.format) {
        Ok(true) => CapabilitySupport::supported(),
        Ok(false) => CapabilitySupport::unsupported(format!(
            "VAAPI frames context cannot upload Host {:?}",
            desc.format
        )),
        Err(error) => CapabilitySupport::not_probed(error.to_string()),
    };
    if output_codec == VideoCodec::Av1 && desc.format == PixelFormat::P010Le {
        if !host_upload.is_supported() {
            return Err(Error::Media(format!(
                "AV1 10-bit submission probe needs a P010 uploadable encoder pool: {}",
                host_upload.unavailable_reason().unwrap_or("unknown reason")
            )));
        }
        probe_av1_10bit_submission(codec, &pool, &desc)?;
    }
    Ok(VaapiEncoderProbe {
        frames: VaapiEncoderFrames::from_pool(&pool, desc)?,
        host_upload,
    })
}

fn probe_av1_10bit_submission(
    codec: NonNull<ffi::AVCodecContext>,
    pool: &HardwareFramesPool,
    desc: &FrameDesc,
) -> Result<()> {
    let frames = VaapiEncoderFrames::from_pool(pool, desc.clone())?;
    let mut surface = frames.acquire(0)?;
    let blank = VideoFrame::new_host(desc.clone(), Some(0), HostFrame::new_zeroed(desc))?;
    surface.upload_p010(&blank)?;
    check(
        unsafe { ffi::avcodec_send_frame(codec.as_ptr(), surface.as_mut_ptr()) },
        "AV1 10-bit encoder rejected an encoder-owned P010 frame",
    )?;
    check(
        unsafe { ffi::avcodec_send_frame(codec.as_ptr(), ptr::null()) },
        "AV1 10-bit encoder probe drain failed",
    )?;
    let mut packet = Packet::new()?;
    let mut received = 0usize;
    loop {
        let result = unsafe { ffi::avcodec_receive_packet(codec.as_ptr(), packet.as_mut_ptr()) };
        if result == ffi::AVERROR_EOF {
            break;
        }
        if result < 0 {
            return Err(ffmpeg_error(
                "AV1 10-bit encoder probe did not produce a drained packet",
                result,
            ));
        }
        if packet.size() == 0 {
            return Err(Error::Media(
                "AV1 10-bit encoder probe produced an empty packet".into(),
            ));
        }
        received += 1;
        packet.unref();
        if received > 16 {
            return Err(Error::Media(
                "AV1 10-bit encoder probe produced too many packets".into(),
            ));
        }
    }
    if received == 0 {
        return Err(Error::Media(
            "AV1 10-bit encoder probe produced no packet".into(),
        ));
    }
    Ok(())
}

impl Encoder {
    pub fn create(path: impl AsRef<Path>, desc: FrameDesc, frame_rate: Rational) -> Result<Self> {
        Self::create_with(
            path,
            desc,
            frame_rate,
            EncodeMode::Software,
            VaapiOptions::default(),
        )
    }

    pub fn create_with(
        path: impl AsRef<Path>,
        desc: FrameDesc,
        frame_rate: Rational,
        mode: EncodeMode,
        vaapi: VaapiOptions,
    ) -> Result<Self> {
        Self::create_internal(
            path,
            desc,
            frame_rate,
            EncoderCreateOptions {
                codec: VideoCodec::H264,
                mode,
                vaapi,
                require_host_upload: true,
                audio: Vec::new(),
                cancellation: CancellationToken::new(),
            },
        )
    }

    pub fn create_with_audio(
        path: impl AsRef<Path>,
        desc: FrameDesc,
        frame_rate: Rational,
        mode: EncodeMode,
        vaapi: VaapiOptions,
        audio: Vec<AudioOutputTemplate>,
        cancellation: CancellationToken,
    ) -> Result<Self> {
        Self::create_internal(
            path,
            desc,
            frame_rate,
            EncoderCreateOptions {
                codec: VideoCodec::H264,
                mode,
                vaapi,
                require_host_upload: true,
                audio,
                cancellation,
            },
        )
    }

    pub fn create_with_hardware_frames(
        path: impl AsRef<Path>,
        desc: FrameDesc,
        frame_rate: Rational,
        vaapi: VaapiOptions,
    ) -> Result<Self> {
        Self::create_internal(
            path,
            desc,
            frame_rate,
            EncoderCreateOptions {
                codec: VideoCodec::H264,
                mode: EncodeMode::Vaapi,
                vaapi,
                require_host_upload: false,
                audio: Vec::new(),
                cancellation: CancellationToken::new(),
            },
        )
    }

    pub fn create_with_hardware_frames_and_audio(
        path: impl AsRef<Path>,
        desc: FrameDesc,
        frame_rate: Rational,
        vaapi: VaapiOptions,
        audio: Vec<AudioOutputTemplate>,
        cancellation: CancellationToken,
    ) -> Result<Self> {
        Self::create_internal(
            path,
            desc,
            frame_rate,
            EncoderCreateOptions {
                codec: VideoCodec::H264,
                mode: EncodeMode::Vaapi,
                vaapi,
                require_host_upload: false,
                audio,
                cancellation,
            },
        )
    }

    pub fn create_with_codec_and_audio(
        path: impl AsRef<Path>,
        desc: FrameDesc,
        frame_rate: Rational,
        output: OutputEncoding,
        vaapi: VaapiOptions,
        audio: Vec<AudioOutputTemplate>,
        cancellation: CancellationToken,
    ) -> Result<Self> {
        Self::create_internal(
            path,
            desc,
            frame_rate,
            EncoderCreateOptions {
                codec: output.codec,
                mode: output.mode,
                vaapi,
                require_host_upload: true,
                audio,
                cancellation,
            },
        )
    }

    pub fn create_with_hardware_frames_codec_and_audio(
        path: impl AsRef<Path>,
        desc: FrameDesc,
        frame_rate: Rational,
        codec: VideoCodec,
        vaapi: VaapiOptions,
        audio: Vec<AudioOutputTemplate>,
        cancellation: CancellationToken,
    ) -> Result<Self> {
        Self::create_internal(
            path,
            desc,
            frame_rate,
            EncoderCreateOptions {
                codec,
                mode: EncodeMode::Vaapi,
                vaapi,
                require_host_upload: false,
                audio,
                cancellation,
            },
        )
    }

    fn create_internal(
        path: impl AsRef<Path>,
        desc: FrameDesc,
        frame_rate: Rational,
        options: EncoderCreateOptions,
    ) -> Result<Self> {
        require_supported_output(&options.codec, &desc, options.mode)?;
        let EncoderCreateOptions {
            codec: output_codec,
            mode,
            vaapi,
            require_host_upload,
            audio,
            cancellation,
        } = options;
        let path = path.as_ref();
        let native = CString::new(path.as_os_str().as_encoded_bytes())
            .map_err(|_| asciiflow_core::Error::Media("output path contains a NUL byte".into()))?;
        let mut format = ptr::null_mut();
        check(
            unsafe {
                ffi::avformat_alloc_output_context2(
                    &mut format,
                    ptr::null_mut(),
                    ptr::null(),
                    native.as_ptr(),
                )
            },
            "failed to select MP4 output format",
        )?;
        let format = NonNull::new(format).ok_or_else(|| {
            asciiflow_core::Error::Media("failed to allocate output context".into())
        })?;
        let mut format_guard = OutputGuard(Some(format));
        if mode == EncodeMode::Software && output_codec != VideoCodec::H264 {
            return Err(Error::Media(format!(
                "{output_codec} software encoding is not implemented"
            )));
        }
        let codec_name = output_codec_name(&output_codec)?;
        let encoder_name = if mode == EncodeMode::Vaapi {
            format!("{codec_name}_vaapi")
        } else {
            "libx264".into()
        };
        let name = CString::new(encoder_name.clone()).unwrap();
        let mut encoder = unsafe { ffi::avcodec_find_encoder_by_name(name.as_ptr()) };
        if encoder.is_null() && mode == EncodeMode::Software {
            encoder = unsafe { ffi::avcodec_find_encoder(ffi::AVCodecID::AV_CODEC_ID_H264) };
        }
        if encoder.is_null() {
            return Err(asciiflow_core::Error::Media(if mode == EncodeMode::Vaapi {
                format!("this FFmpeg build has no {encoder_name} encoder")
            } else {
                "this FFmpeg build has no H.264 encoder".into()
            }));
        }
        require_output_codec_id(encoder, &output_codec)?;
        if mode == EncodeMode::Vaapi {
            require_vaapi_encoder(encoder)?;
        }
        let stream =
            NonNull::new(unsafe { ffi::avformat_new_stream(format.as_ptr(), ptr::null()) })
                .ok_or_else(|| {
                    asciiflow_core::Error::Media("failed to create output video stream".into())
                })?;
        let codec =
            NonNull::new(unsafe { ffi::avcodec_alloc_context3(encoder) }).ok_or_else(|| {
                asciiflow_core::Error::Media(format!(
                    "failed to allocate {output_codec} encoder context"
                ))
            })?;
        let mut codec_guard = CodecGuard(Some(codec));
        unsafe {
            (*codec.as_ptr()).codec_id = (*encoder).id;
            (*codec.as_ptr()).codec_type = ffi::AVMediaType::AVMEDIA_TYPE_VIDEO;
            (*codec.as_ptr()).width = desc.width as i32;
            (*codec.as_ptr()).height = desc.height as i32;
            (*codec.as_ptr()).pix_fmt = if mode == EncodeMode::Vaapi {
                ffi::AVPixelFormat::AV_PIX_FMT_VAAPI
            } else {
                ffi::AVPixelFormat::AV_PIX_FMT_NV12
            };
            (*codec.as_ptr()).time_base = ffi::AVRational {
                num: frame_rate.denominator,
                den: frame_rate.numerator,
            };
            (*codec.as_ptr()).framerate = ffi::AVRational {
                num: frame_rate.numerator,
                den: frame_rate.denominator,
            };
            (*codec.as_ptr()).color_range = output_color_range(&desc);
            (*codec.as_ptr()).colorspace = ffi::AVColorSpace::AVCOL_SPC_BT709;
            (*codec.as_ptr()).color_primaries = ffi::AVColorPrimaries::AVCOL_PRI_BT709;
            (*codec.as_ptr()).color_trc = ffi::AVColorTransferCharacteristic::AVCOL_TRC_BT709;
            (*codec.as_ptr()).chroma_sample_location = ffi::AVChromaLocation::AVCHROMA_LOC_LEFT;
            if output_codec == VideoCodec::Hevc {
                (*codec.as_ptr()).profile = hevc_profile(desc.format);
            } else if output_codec == VideoCodec::Av1 {
                (*codec.as_ptr()).global_quality = 25;
            }
            if mode == EncodeMode::Vaapi {
                (*codec.as_ptr()).max_b_frames = 0;
                (*codec.as_ptr()).gop_size = 250;
            }
            if (*(*format.as_ptr()).oformat).flags & ffi::AVFMT_GLOBALHEADER != 0 {
                (*codec.as_ptr()).flags |= ffi::AV_CODEC_FLAG_GLOBAL_HEADER as i32;
            }
        }
        let (hardware_device, frames_pool) = if mode == EncodeMode::Vaapi {
            let device = vaapi.create_device()?;
            let pool = encoder_pool(&device, &desc)?;
            if require_host_upload && !pool_supports_upload(&pool, desc.format)? {
                return Err(asciiflow_core::Error::UnsupportedFrame(format!(
                    "VAAPI device cannot upload Host {:?} frames",
                    desc.format
                )));
            }
            unsafe {
                (*codec.as_ptr()).hw_frames_ctx = pool.try_clone_ref()?;
            }
            (Some(device), Some(pool))
        } else {
            (None, None)
        };
        let mut options = ptr::null_mut();
        let codec_options: &[(&str, &str)] = if mode == EncodeMode::Vaapi {
            vaapi_codec_options(&output_codec)
        } else {
            &[
                ("preset", "ultrafast"),
                ("tune", "zerolatency"),
                ("crf", "20"),
            ]
        };
        for (key, value) in codec_options {
            let k = CString::new(*key).unwrap();
            let v = CString::new(*value).unwrap();
            check(
                unsafe { ffi::av_dict_set(&mut options, k.as_ptr(), v.as_ptr(), 0) },
                "failed to set encoder option",
            )?;
        }
        let opened = unsafe { ffi::avcodec_open2(codec.as_ptr(), encoder, &mut options) };
        let unused_options = unsafe { ffi::av_dict_count(options) };
        unsafe { ffi::av_dict_free(&mut options) };
        let configure_operation = if mode == EncodeMode::Vaapi {
            format!(
                "failed to configure {encoder_name} encoder for {}x{} {:?}",
                desc.width, desc.height, desc.format
            )
        } else {
            "failed to configure software H.264 encoder".into()
        };
        check(opened, &configure_operation)?;
        if unused_options != 0 {
            return Err(asciiflow_core::Error::Media(format!(
                "{output_codec} encoder rejected {unused_options} configuration option(s)"
            )));
        }
        check(
            unsafe {
                ffi::avcodec_parameters_from_context((*stream.as_ptr()).codecpar, codec.as_ptr())
            },
            "failed to copy encoder parameters",
        )?;
        unsafe {
            (*stream.as_ptr()).time_base = (*codec.as_ptr()).time_base;
        }
        let mut audio_routes = (|| -> Result<Vec<AudioMuxRoute>> {
            let mut routes = Vec::with_capacity(audio.len());
            for template in audio {
                let output_stream =
                    NonNull::new(unsafe { ffi::avformat_new_stream(format.as_ptr(), ptr::null()) })
                        .ok_or_else(|| {
                            Error::Media("failed to create output audio stream".into())
                        })?;
                check(
                    unsafe {
                        ffi::avcodec_parameters_copy(
                            (*output_stream.as_ptr()).codecpar,
                            template.parameters.as_ptr(),
                        )
                    },
                    "failed to copy passthrough audio parameters",
                )?;
                unsafe {
                    (*(*output_stream.as_ptr()).codecpar).codec_tag = 0;
                    (*output_stream.as_ptr()).time_base = template.input_time_base;
                    (*output_stream.as_ptr()).disposition = template.disposition;
                }
                if let Some(language) = template.language {
                    let key =
                        CString::new("language").expect("static metadata key contains no NUL");
                    let value = CString::new(language).map_err(|_| {
                        Error::Media("audio language metadata contains a NUL byte".into())
                    })?;
                    check(
                        unsafe {
                            ffi::av_dict_set(
                                &mut (*output_stream.as_ptr()).metadata,
                                key.as_ptr(),
                                value.as_ptr(),
                                0,
                            )
                        },
                        "failed to copy audio language metadata",
                    )?;
                }
                routes.push(AudioMuxRoute {
                    input_index: template.input_index,
                    output_index: unsafe { (*output_stream.as_ptr()).index },
                    output_time_base: template.input_time_base,
                });
            }
            Ok(routes)
        })()
        .map_err(|error| {
            Error::pipeline(
                PipelineStage::MuxInitialization,
                "create passthrough audio streams",
                error,
            )
        })?;
        if unsafe { (*(*format.as_ptr()).oformat).flags } & ffi::AVFMT_NOFILE == 0 {
            check(
                unsafe {
                    ffi::avio_open(
                        &mut (*format.as_ptr()).pb,
                        native.as_ptr(),
                        ffi::AVIO_FLAG_WRITE,
                    )
                },
                &format!("failed to create output {}", path.display()),
            )
            .map_err(|error| {
                Error::pipeline(PipelineStage::MuxInitialization, "open MP4 output", error)
            })?;
        }
        check(
            unsafe { ffi::avformat_write_header(format.as_ptr(), ptr::null_mut()) },
            "failed to write MP4 header",
        )
        .map_err(|error| {
            Error::pipeline(
                PipelineStage::MuxInitialization,
                "write MP4 container header",
                error,
            )
        })?;
        let video_stream_index = unsafe { (*stream.as_ptr()).index };
        let video_time_base = unsafe { (*stream.as_ptr()).time_base };
        for route in &mut audio_routes {
            let output_stream =
                unsafe { *(*format.as_ptr()).streams.add(route.output_index as usize) };
            route.output_time_base = unsafe { (*output_stream).time_base };
        }
        let mut frame = Frame::new()?;
        unsafe {
            (*frame.as_mut_ptr()).format = match desc.format {
                PixelFormat::Nv12 => ffi::AVPixelFormat::AV_PIX_FMT_NV12,
                PixelFormat::P010Le => ffi::AVPixelFormat::AV_PIX_FMT_P010LE,
            } as i32;
            (*frame.as_mut_ptr()).width = desc.width as i32;
            (*frame.as_mut_ptr()).height = desc.height as i32;
        }
        check(
            unsafe { ffi::av_frame_get_buffer(frame.as_mut_ptr(), 32) },
            "failed to allocate encoder frame storage",
        )?;
        let packet = Packet::new()?;
        let hardware_frame = (mode == EncodeMode::Vaapi)
            .then(HardwareFrame::new)
            .transpose()?;
        tracing::debug!(
            output = %path.display(),
            width = desc.width,
            height = desc.height,
            fps_numerator = frame_rate.numerator,
            fps_denominator = frame_rate.denominator,
            mode = ?mode,
            vaapi_device = vaapi.display_device(),
            codec = %output_codec,
            "opened video encoder"
        );
        let format = format_guard.take();
        let (mux_sender, mux_receiver) = bounded(MUX_CHANNEL_CAPACITY);
        let mux_failure = Arc::new(Mutex::new(None));
        let mux_stats = Arc::new(Mutex::new(MuxStats::default()));
        let mux_stop = CancellationToken::new();
        let output = MuxOutput {
            format,
            video_stream_index,
            video_time_base,
            audio_routes,
        };
        let worker_failure = mux_failure.clone();
        let worker_stats = mux_stats.clone();
        let worker_stop = mux_stop.clone();
        let worker_cancellation = cancellation.clone();
        let mux_thread = std::thread::Builder::new()
            .name("asciiflow-mux".into())
            .spawn(move || {
                let result = run_mux_worker(
                    output,
                    &mux_receiver,
                    worker_stats,
                    worker_failure.clone(),
                    worker_stop,
                    worker_cancellation,
                );
                if let Err(error) = &result
                    && !error.is_cancelled()
                {
                    *worker_failure.lock().expect("mux failure lock poisoned") =
                        Some(error.to_string());
                }
                result
            })
            .map_err(|error| {
                Error::pipeline_message(
                    PipelineStage::MuxInitialization,
                    "start mux worker",
                    error.to_string(),
                )
            })?;
        Ok(Self {
            codec: codec_guard.take(),
            video_stream_index,
            video_time_base,
            frame,
            hardware_frame,
            frames_pool,
            _hardware_device: hardware_device,
            packet,
            mode,
            output_codec,
            timings: SinkTimings::default(),
            desc,
            next_pts: 0,
            finished: false,
            finish_attempted: false,
            mux_sender,
            mux_thread: Some(mux_thread),
            mux_failure,
            mux_stats,
            mux_stop,
            cancellation,
            #[cfg(feature = "encode-characterization")]
            submitted_total: 0,
            #[cfg(feature = "encode-characterization")]
            packets_total: 0,
            #[cfg(test)]
            inject_send_failure: false,
            #[cfg(test)]
            inject_receive_failure: false,
        })
    }
    fn drain_packets(&mut self) -> Result<usize> {
        let mut received = 0;
        loop {
            #[cfg(test)]
            if self.inject_receive_failure {
                self.inject_receive_failure = false;
                return Err(Error::pipeline_message(
                    PipelineStage::EncodeRuntime,
                    "receive encoded packet",
                    format!("injected {} receive_packet failure", self.output_codec),
                ));
            }
            #[cfg(feature = "encode-characterization")]
            let receive_started = Instant::now();
            let result = unsafe {
                ffi::avcodec_receive_packet(self.codec.as_ptr(), self.packet.as_mut_ptr())
            };
            #[cfg(feature = "encode-characterization")]
            {
                self.timings.encode_diagnostics.receive_wall += receive_started.elapsed();
                if again(result) {
                    self.timings.encode_diagnostics.receive_eagain += 1;
                }
            }
            if again(result) || result == ffi::AVERROR_EOF {
                return Ok(received);
            }
            if result < 0 {
                return Err(ffmpeg_error(
                    &format!("failed to receive {} packet", self.output_codec),
                    result,
                ));
            }
            if self.packet.size() > 16 * 1024 * 1024 {
                return Err(Error::pipeline_message(
                    PipelineStage::MuxRuntime,
                    "validate video packet size",
                    "compressed packet exceeds the 16 MiB mux limit",
                ));
            }
            received += 1;
            #[cfg(feature = "encode-characterization")]
            {
                self.packets_total += 1;
                self.timings.encode_diagnostics.received_packets += 1;
                self.timings.encode_diagnostics.received_packet_bytes += self.packet.size();
            }
            unsafe {
                ffi::av_packet_rescale_ts(
                    self.packet.as_mut_ptr(),
                    (*self.codec.as_ptr()).time_base,
                    self.video_time_base,
                );
                if (*self.packet.as_mut_ptr()).duration == 0 {
                    (*self.packet.as_mut_ptr()).duration = ffi::av_rescale_q(
                        1,
                        (*self.codec.as_ptr()).time_base,
                        self.video_time_base,
                    );
                }
            }
            let mut packet = Packet::new()?;
            packet.take_from(&mut self.packet);
            self.send_mux_message(MuxMessage::Packet {
                packet,
                input_index: self.video_stream_index as usize,
                input_time_base: self.video_time_base,
                audio: false,
            })?;
        }
    }

    pub fn audio_packet_sender(&self) -> AudioPacketSender {
        AudioPacketSender::new(
            self.mux_sender.clone(),
            self.cancellation.clone(),
            self.mux_failure.clone(),
        )
    }

    fn send_mux_message(&self, mut message: MuxMessage) -> Result<()> {
        #[cfg(feature = "encode-characterization")]
        let send_started = Instant::now();
        loop {
            if self.cancellation.is_cancelled() {
                return Err(Error::Cancelled);
            }
            match self.mux_sender.send_timeout(message, MUX_POLL) {
                Ok(()) => {
                    #[cfg(feature = "encode-characterization")]
                    {
                        let send_wall = send_started.elapsed();
                        self.mux_stats
                            .lock()
                            .expect("mux stats lock poisoned")
                            .diagnostics
                            .mux_queue_send_wall += send_wall;
                    }
                    return Ok(());
                }
                Err(SendTimeoutError::Timeout(pending)) => message = pending,
                Err(SendTimeoutError::Disconnected(_)) => return Err(self.current_mux_failure()),
            }
        }
    }

    fn current_mux_failure(&self) -> Error {
        let message = self
            .mux_failure
            .lock()
            .expect("mux failure lock poisoned")
            .clone()
            .unwrap_or_else(|| "mux worker stopped unexpectedly".into());
        Error::pipeline_message(
            PipelineStage::MuxRuntime,
            "write interleaved packet",
            message,
        )
    }

    fn join_mux(&mut self) -> Result<()> {
        let Some(thread) = self.mux_thread.take() else {
            return Ok(());
        };
        match thread.join() {
            Ok(result) => result,
            Err(_) => Err(Error::pipeline_message(
                PipelineStage::MuxRuntime,
                "join mux worker",
                "mux worker panicked",
            )),
        }
    }

    pub fn encoder_frames(&self) -> Result<VaapiEncoderFrames> {
        if self.mode != EncodeMode::Vaapi {
            return Err(asciiflow_core::Error::Media(
                "encoder-compatible hardware frames require a VAAPI encoder".into(),
            ));
        }
        VaapiEncoderFrames::from_pool(
            self.frames_pool
                .as_ref()
                .expect("VAAPI encoder frames pool missing"),
            self.desc.clone(),
        )
    }

    pub fn encode_hardware_frame(&mut self, mut frame: VaapiEncoderFrame) -> Result<()> {
        if self.mode != EncodeMode::Vaapi {
            return Err(asciiflow_core::Error::Media(
                "hardware-frame submission requires a VAAPI encoder".into(),
            ));
        }
        if frame.desc() != &self.desc {
            return Err(asciiflow_core::Error::Media(
                "encoder received a hardware frame with a different descriptor".into(),
            ));
        }
        if !frame.belongs_to(
            self.frames_pool
                .as_ref()
                .expect("VAAPI encoder frames pool missing"),
        ) {
            return Err(asciiflow_core::Error::Media(
                "encoder hardware frame belongs to a different AVHWFramesContext".into(),
            ));
        }
        if frame.pts() != self.next_pts {
            return Err(asciiflow_core::Error::Media(format!(
                "encoder hardware-frame order violation: received PTS {}, expected {}",
                frame.pts(),
                self.next_pts
            )));
        }
        self.submit_native_frame(frame.as_mut_ptr())
    }

    fn submit_native_frame(&mut self, frame: *mut ffi::AVFrame) -> Result<()> {
        unsafe {
            let native = &mut *frame;
            native.color_range = ffi::AVColorRange::AVCOL_RANGE_MPEG;
            native.colorspace = ffi::AVColorSpace::AVCOL_SPC_BT709;
            native.color_primaries = ffi::AVColorPrimaries::AVCOL_PRI_BT709;
            native.color_trc = ffi::AVColorTransferCharacteristic::AVCOL_TRC_BT709;
            native.chroma_location = ffi::AVChromaLocation::AVCHROMA_LOC_LEFT;
        }
        let encode_started = Instant::now();
        #[cfg(test)]
        if self.inject_send_failure {
            self.inject_send_failure = false;
            return Err(Error::pipeline_message(
                PipelineStage::EncodeRuntime,
                "send frame to encoder",
                format!("injected {} send_frame failure", self.output_codec),
            ));
        }
        let mut retries = 0_u32;
        loop {
            #[cfg(feature = "encode-characterization")]
            let send_started = Instant::now();
            let sent = unsafe { ffi::avcodec_send_frame(self.codec.as_ptr(), frame) };
            #[cfg(feature = "encode-characterization")]
            {
                self.timings.encode_diagnostics.send_wall += send_started.elapsed();
                if again(sent) {
                    self.timings.encode_diagnostics.send_eagain += 1;
                }
            }
            if again(sent) {
                retries += 1;
                let drained = self.drain_packets()?;
                if drained == 0 || retries > 1024 {
                    return Err(Error::pipeline_message(
                        PipelineStage::EncodeRuntime,
                        "retry encoder frame submission",
                        "encoder returned EAGAIN without bounded packet-drain progress",
                    ));
                }
                continue;
            }
            check(
                sent,
                &format!("failed to send NV12 frame to {} encoder", self.output_codec),
            )?;
            break;
        }
        #[cfg(feature = "encode-characterization")]
        {
            self.submitted_total += 1;
            let diagnostics = &mut self.timings.encode_diagnostics;
            diagnostics.submitted_frames += 1;
            diagnostics.max_send_retries = diagnostics.max_send_retries.max(retries);
            diagnostics.peak_frame_packet_delta = diagnostics
                .peak_frame_packet_delta
                .max(self.submitted_total.saturating_sub(self.packets_total));
        }
        self.next_pts += 1;
        let result = self.drain_packets().map(|_| ());
        self.timings.submit_receive += encode_started.elapsed();
        result
    }
}

fn require_supported_output(codec: &VideoCodec, desc: &FrameDesc, mode: EncodeMode) -> Result<()> {
    desc.validate_layout()?;
    if desc.color_space.range != ColorRange::Limited {
        return Err(Error::UnsupportedFrame(
            "ASCII output currently emits limited-range code values; full/unspecified range cannot be tagged safely".into(),
        ));
    }
    if desc.format == PixelFormat::P010Le
        && (!matches!(codec, VideoCodec::Hevc | VideoCodec::Av1) || mode != EncodeMode::Vaapi)
    {
        return Err(Error::UnsupportedFrame(
            "P010LE output requires HEVC Main10 or AV1 Main 10-bit VAAPI encoding".into(),
        ));
    }
    if desc.format == PixelFormat::P010Le
        && (desc.color_space.matrix != ColorMatrix::Bt709
            || desc.color_space.primaries != ColorPrimaries::Bt709
            || desc.color_space.transfer != TransferCharacteristic::Bt709
            || desc.color_space.chroma_location != ChromaLocation::Left
            || desc.color_space.range != ColorRange::Limited)
    {
        return Err(Error::UnsupportedFrame(
            "10-bit output requires explicitly tagged BT.709 SDR color".into(),
        ));
    }
    Ok(())
}

fn output_color_range(desc: &FrameDesc) -> ffi::AVColorRange {
    debug_assert_eq!(desc.color_space.range, ColorRange::Limited);
    ffi::AVColorRange::AVCOL_RANGE_MPEG
}

fn hevc_profile(format: PixelFormat) -> i32 {
    match format {
        PixelFormat::Nv12 => ffi::FF_PROFILE_HEVC_MAIN,
        PixelFormat::P010Le => ffi::FF_PROFILE_HEVC_MAIN_10,
    }
}

fn encoder_pool(device: &HardwareDevice, desc: &FrameDesc) -> Result<HardwareFramesPool> {
    match desc.format {
        PixelFormat::Nv12 => HardwareFramesPool::vaapi_nv12(device, desc.width, desc.height),
        PixelFormat::P010Le => HardwareFramesPool::vaapi_p010(device, desc.width, desc.height),
    }
}

fn pool_supports_upload(pool: &HardwareFramesPool, format: PixelFormat) -> Result<bool> {
    match format {
        PixelFormat::Nv12 => pool.supports_upload_nv12(),
        PixelFormat::P010Le => pool.supports_upload_p010(),
    }
}
impl FrameSink for Encoder {
    fn encode(&mut self, frame: VideoFrame) -> Result<()> {
        if frame.desc() != &self.desc {
            return Err(asciiflow_core::Error::Media(
                "encoder received a frame with a different descriptor".into(),
            ));
        }
        self.frame.make_writable()?;
        let native = unsafe { &mut *self.frame.as_mut_ptr() };
        let (source_y, source_uv) = frame.host().planes(frame.desc());
        let width = self.desc.y_stride();
        let height = self.desc.height as usize;
        unsafe {
            for row in 0..height {
                ptr::copy_nonoverlapping(
                    source_y.as_ptr().add(row * width),
                    native.data[0].add(row * native.linesize[0] as usize),
                    width,
                );
            }
            for row in 0..height / 2 {
                ptr::copy_nonoverlapping(
                    source_uv.as_ptr().add(row * width),
                    native.data[1].add(row * native.linesize[1] as usize),
                    width,
                );
            }
        }
        native.pts = self.next_pts;
        let pts = native.pts;
        let frame_to_send = if self.mode == EncodeMode::Vaapi {
            let hardware_frame = self
                .hardware_frame
                .as_mut()
                .expect("VAAPI encoder hardware frame missing");
            hardware_frame.allocate(
                self.frames_pool
                    .as_ref()
                    .expect("VAAPI encoder frames pool missing"),
            )?;
            let upload_started = Instant::now();
            let uploaded = unsafe {
                ffi::av_hwframe_transfer_data(
                    hardware_frame.as_mut_ptr(),
                    self.frame.as_mut_ptr(),
                    0,
                )
            };
            self.timings.hardware_upload += upload_started.elapsed();
            check(uploaded, "failed to upload Host frame to VAAPI")?;
            unsafe { (*hardware_frame.as_mut_ptr()).pts = pts };
            hardware_frame.as_mut_ptr()
        } else {
            self.frame.as_mut_ptr()
        };
        self.submit_native_frame(frame_to_send)
    }
    fn finish(&mut self) -> Result<()> {
        if self.finished {
            return Ok(());
        }
        if self.finish_attempted {
            return Err(asciiflow_core::Error::Media(
                "encoder finalization already failed and cannot be retried".into(),
            ));
        }
        self.finish_attempted = true;
        let encode_started = Instant::now();
        if self.next_pts != 0 {
            #[cfg(feature = "encode-characterization")]
            let drain_started = Instant::now();
            #[cfg(feature = "encode-characterization")]
            let send_started = Instant::now();
            let result = unsafe { ffi::avcodec_send_frame(self.codec.as_ptr(), ptr::null()) };
            #[cfg(feature = "encode-characterization")]
            {
                self.timings.encode_diagnostics.send_wall += send_started.elapsed();
            }
            if result < 0 && result != ffi::AVERROR_EOF {
                return Err(ffmpeg_error(
                    &format!("failed to flush {} encoder", self.output_codec),
                    result,
                ));
            }
            self.drain_packets()?;
            #[cfg(feature = "encode-characterization")]
            {
                self.timings.encode_diagnostics.drain_wall += drain_started.elapsed();
            }
        }
        self.timings.submit_receive += encode_started.elapsed();
        self.send_mux_message(MuxMessage::Finish)?;
        self.join_mux()?;
        self.finished = true;
        Ok(())
    }

    fn take_timings(&mut self) -> SinkTimings {
        let mut timings = std::mem::take(&mut self.timings);
        let mut mux = self.mux_stats.lock().expect("mux stats lock poisoned");
        let mux = std::mem::take(&mut *mux);
        timings.audio_passthrough = mux.audio_passthrough;
        timings.audio_packets = mux.audio_packets;
        timings.audio_bytes = mux.audio_bytes;
        timings.encode_diagnostics.accumulate(mux.diagnostics);
        timings
    }
}
impl Drop for Encoder {
    fn drop(&mut self) {
        if self.mux_thread.is_some() {
            self.mux_stop.cancel();
            let _ = self.mux_sender.try_send(MuxMessage::Abort);
            let _ = self.join_mux();
        }
        unsafe {
            let mut codec = self.codec.as_ptr();
            ffi::avcodec_free_context(&mut codec);
        }
    }
}
unsafe impl Send for Encoder {}

fn run_mux_worker(
    output: MuxOutput,
    receiver: &Receiver<MuxMessage>,
    stats: Arc<Mutex<MuxStats>>,
    failure: Arc<Mutex<Option<String>>>,
    stop: CancellationToken,
    cancellation: CancellationToken,
) -> Result<()> {
    let mut audio_done = output.audio_routes.is_empty();
    let mut video_offset = 0_i64;
    let mut buffered_packets = 0_u32;
    let mut buffered_bytes = 0_u64;
    #[cfg(test)]
    let mut fail_audio_after: Option<u64> = None;
    #[cfg(test)]
    let mut fail_video_after: Option<u64> = None;
    loop {
        if stop.is_cancelled() || cancellation.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let message = match receiver.recv_timeout(MUX_POLL) {
            Ok(message) => message,
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                return Err(Error::pipeline_message(
                    PipelineStage::MuxRuntime,
                    "receive mux packet",
                    "all mux producers disconnected before finalization",
                ));
            }
        };
        match message {
            #[cfg(test)]
            MuxMessage::FailAudioAfter(count) => fail_audio_after = Some(count),
            #[cfg(test)]
            MuxMessage::FailVideoAfter(count) => fail_video_after = Some(count),
            MuxMessage::Packet {
                mut packet,
                input_index,
                input_time_base,
                audio,
            } => {
                #[cfg(test)]
                if audio && let Some(remaining) = &mut fail_audio_after {
                    if *remaining == 0 {
                        return Err(Error::pipeline_message(
                            PipelineStage::MuxRuntime,
                            "write audio packet",
                            "injected audio mux failure",
                        ));
                    }
                    *remaining -= 1;
                }
                #[cfg(test)]
                if !audio && let Some(remaining) = &mut fail_video_after {
                    if *remaining == 0 {
                        return Err(Error::pipeline_message(
                            PipelineStage::MuxRuntime,
                            "write video packet",
                            "injected video mux failure",
                        ));
                    }
                    *remaining -= 1;
                }
                let (output_index, output_time_base) = if audio {
                    let route = output
                        .audio_routes
                        .iter()
                        .find(|route| route.input_index == input_index)
                        .ok_or_else(|| {
                            Error::pipeline_message(
                                PipelineStage::MuxRuntime,
                                "map passthrough audio packet",
                                format!("no output mapping exists for audio stream #{input_index}"),
                            )
                        })?;
                    (route.output_index, route.output_time_base)
                } else {
                    (output.video_stream_index, output.video_time_base)
                };
                let bytes = packet.size();
                if bytes > 16 * 1024 * 1024 {
                    return Err(Error::pipeline_message(
                        PipelineStage::MuxRuntime,
                        "validate packet size",
                        "compressed packet exceeds the 16 MiB mux limit",
                    ));
                }
                let started = Instant::now();
                unsafe {
                    ffi::av_packet_rescale_ts(
                        packet.as_mut_ptr(),
                        input_time_base,
                        output_time_base,
                    );
                    (*packet.as_mut_ptr()).stream_index = output_index;
                    if !audio {
                        let native = &mut *packet.as_mut_ptr();
                        native.pts = native
                            .pts
                            .checked_add(video_offset)
                            .ok_or_else(|| Error::Media("video PTS offset overflow".into()))?;
                        native.dts = native
                            .dts
                            .checked_add(video_offset)
                            .ok_or_else(|| Error::Media("video DTS offset overflow".into()))?;
                    }
                }
                #[cfg(feature = "encode-characterization")]
                let write_started = Instant::now();
                let written = unsafe {
                    ffi::av_interleaved_write_frame(output.format.as_ptr(), packet.as_mut_ptr())
                };
                #[cfg(feature = "encode-characterization")]
                if !audio {
                    let write_wall = write_started.elapsed();
                    stats
                        .lock()
                        .expect("mux stats lock poisoned")
                        .diagnostics
                        .mux_video_write_wall += write_wall;
                }
                if let Err(error) = check(written, "failed to mux interleaved packet") {
                    let message = error.to_string();
                    *failure.lock().expect("mux failure lock poisoned") = Some(message.clone());
                    return Err(Error::pipeline_message(
                        PipelineStage::MuxRuntime,
                        "write interleaved packet",
                        message,
                    ));
                }
                if audio {
                    let mut stats = stats.lock().expect("mux stats lock poisoned");
                    stats.audio_passthrough += started.elapsed();
                    stats.audio_packets += 1;
                    stats.audio_bytes += bytes;
                }
                // Bound libavformat's interleaver as well as our channel, even
                // for non-interleaved or sparse-stream input. MP4 accepts chunks
                // arriving in different streams' timestamp order.
                buffered_packets += 1;
                buffered_bytes += bytes;
                if buffered_packets >= 64 || buffered_bytes >= 8 * 1024 * 1024 {
                    #[cfg(feature = "encode-characterization")]
                    let flush_started = Instant::now();
                    check(
                        unsafe {
                            ffi::av_interleaved_write_frame(output.format.as_ptr(), ptr::null_mut())
                        },
                        "flush bounded mux interleaver",
                    )?;
                    #[cfg(feature = "encode-characterization")]
                    {
                        let flush_wall = flush_started.elapsed();
                        stats
                            .lock()
                            .expect("mux stats lock poisoned")
                            .diagnostics
                            .mux_interleave_flush_wall += flush_wall;
                    }
                    buffered_packets = 0;
                    buffered_bytes = 0;
                }
            }
            MuxMessage::AudioDone => audio_done = true,
            MuxMessage::VideoOrigin { pts, time_base } => {
                video_offset = unsafe { ffi::av_rescale_q(pts, time_base, output.video_time_base) };
            }
            MuxMessage::Finish => {
                if !audio_done {
                    return Err(Error::pipeline_message(
                        PipelineStage::Finalization,
                        "finish mux producers",
                        "audio producer must finish before the video sink",
                    ));
                }
                #[cfg(feature = "encode-characterization")]
                let trailer_started = Instant::now();
                check(
                    unsafe { ffi::av_write_trailer(output.format.as_ptr()) },
                    "failed to finalize MP4 output",
                )
                .map_err(|error| {
                    let message = error.to_string();
                    *failure.lock().expect("mux failure lock poisoned") = Some(message);
                    Error::pipeline(
                        PipelineStage::Finalization,
                        "write MP4 container trailer",
                        error,
                    )
                })?;
                #[cfg(feature = "encode-characterization")]
                {
                    let trailer_wall = trailer_started.elapsed();
                    stats
                        .lock()
                        .expect("mux stats lock poisoned")
                        .diagnostics
                        .mux_trailer_wall += trailer_wall;
                }
                if unsafe { !(*output.format.as_ptr()).pb.is_null() } {
                    check(
                        unsafe { ffi::avio_closep(&mut (*output.format.as_ptr()).pb) },
                        "close finalized MP4 output",
                    )
                    .map_err(|error| {
                        Error::pipeline(PipelineStage::Finalization, "close MP4 output", error)
                    })?;
                }
                return Ok(());
            }
            MuxMessage::Abort => return Err(Error::Cancelled),
        }
    }
}

impl Drop for MuxOutput {
    fn drop(&mut self) {
        unsafe {
            if !(*self.format.as_ptr()).pb.is_null() {
                ffi::avio_closep(&mut (*self.format.as_ptr()).pb);
            }
            ffi::avformat_free_context(self.format.as_ptr());
        }
    }
}

fn require_vaapi_encoder(encoder: *const ffi::AVCodec) -> Result<()> {
    let mut index = 0;
    loop {
        let config = unsafe { ffi::avcodec_get_hw_config(encoder, index) };
        if config.is_null() {
            return Err(asciiflow_core::Error::Media(
                "VAAPI encoder has no VAAPI hardware-frames configuration".into(),
            ));
        }
        let config = unsafe { &*config };
        if config.device_type == ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI
            && config.pix_fmt == ffi::AVPixelFormat::AV_PIX_FMT_VAAPI
            && config.methods & ffi::AV_CODEC_HW_CONFIG_METHOD_HW_FRAMES_CTX as i32 != 0
        {
            return Ok(());
        }
        index += 1;
    }
}

fn output_codec_name(codec: &VideoCodec) -> Result<&'static str> {
    match codec {
        VideoCodec::H264 => Ok("h264"),
        VideoCodec::Hevc => Ok("hevc"),
        VideoCodec::Av1 => Ok("av1"),
        VideoCodec::Other(name) => Err(Error::Media(format!(
            "output codec {name} is not implemented"
        ))),
    }
}

fn vaapi_codec_options(codec: &VideoCodec) -> &'static [(&'static str, &'static str)] {
    match codec {
        // AV1 VAAPI exposes `profile=main` but no `qp` private option.
        // `global_quality=25` is set directly on the codec context.
        VideoCodec::Av1 => &[
            ("rc_mode", "CQP"),
            ("profile", "main"),
            ("async_depth", "2"),
        ],
        // These historical baselines are not cross-codec quality-equivalent.
        _ => &[("rc_mode", "CQP"), ("qp", "20"), ("async_depth", "2")],
    }
}

fn require_output_codec_id(encoder: *const ffi::AVCodec, codec: &VideoCodec) -> Result<()> {
    let expected = match codec {
        VideoCodec::H264 => ffi::AVCodecID::AV_CODEC_ID_H264,
        VideoCodec::Hevc => ffi::AVCodecID::AV_CODEC_ID_HEVC,
        VideoCodec::Av1 => ffi::AVCodecID::AV_CODEC_ID_AV1,
        VideoCodec::Other(name) => {
            return Err(Error::Media(format!(
                "output codec {name} is not implemented"
            )));
        }
    };
    if unsafe { (*encoder).id } != expected {
        return Err(Error::Media(format!(
            "selected encoder does not implement requested {codec} codec ID"
        )));
    }
    Ok(())
}
struct OutputGuard(Option<NonNull<ffi::AVFormatContext>>);
impl OutputGuard {
    fn take(&mut self) -> NonNull<ffi::AVFormatContext> {
        self.0.take().expect("output guard already empty")
    }
}
impl Drop for OutputGuard {
    fn drop(&mut self) {
        if let Some(pointer) = self.0 {
            unsafe {
                if !(*pointer.as_ptr()).pb.is_null() {
                    ffi::avio_closep(&mut (*pointer.as_ptr()).pb);
                }
                ffi::avformat_free_context(pointer.as_ptr());
            }
        }
    }
}
struct CodecGuard(Option<NonNull<ffi::AVCodecContext>>);
impl CodecGuard {
    fn take(&mut self) -> NonNull<ffi::AVCodecContext> {
        self.0.take().expect("codec guard already empty")
    }
}

impl Drop for CodecGuard {
    fn drop(&mut self) {
        if let Some(pointer) = self.0 {
            let mut p = pointer.as_ptr();
            unsafe { ffi::avcodec_free_context(&mut p) }
        }
    }
}

#[cfg(test)]
mod audio_regression_tests {
    use super::*;
    use crate::Decoder;
    use asciiflow_core::{
        AsciiBackend, AsciiConfig, AudioPlan, AudioPolicy, BackendOutput, BackendTimings,
        ColorSpace, FrameDesc, HostFrame, Pipeline, VideoCodec,
    };
    struct Identity;
    impl AsciiBackend for Identity {
        fn process(&mut self, frame: VideoFrame, _: &AsciiConfig) -> Result<BackendOutput> {
            Ok(BackendOutput {
                frame,
                timings: BackendTimings::default(),
            })
        }
    }
    struct OutputPath(std::path::PathBuf);
    impl Drop for OutputPath {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    fn p010_frame_is_rejected_before_encoder_or_output_initialization() {
        let desc = FrameDesc::host_p010_le(128, 96, ColorSpace::default()).unwrap();
        let output = std::env::temp_dir().join(format!(
            "asciiflow-p010-output-rejected-{}.mp4",
            std::process::id()
        ));
        let error = Encoder::create_with_codec_and_audio(
            &output,
            desc.clone(),
            Rational::new(50, 1).unwrap(),
            OutputEncoding {
                codec: VideoCodec::H264,
                mode: EncodeMode::Software,
            },
            VaapiOptions::default(),
            Vec::new(),
            CancellationToken::new(),
        )
        .err()
        .unwrap();
        assert!(
            error
                .to_string()
                .contains("requires HEVC Main10 or AV1 Main")
        );
        assert!(!output.exists());
        let error = probe_vaapi_encoder_for(
            VideoCodec::H264,
            desc,
            Rational::new(50, 1).unwrap(),
            VaapiOptions::default(),
        )
        .err()
        .unwrap();
        assert!(
            error
                .to_string()
                .contains("requires HEVC Main10 or AV1 Main")
        );
    }

    #[test]
    #[ignore = "requires HEVC Main10 VAAPI encode"]
    fn main10_empty_stream_writes_valid_trailer() {
        assert_ten_bit_empty_stream_trailer(VideoCodec::Hevc);
    }

    #[test]
    #[ignore = "requires AV1 10-bit VAAPI encode"]
    fn av1_10bit_empty_stream_writes_valid_trailer() {
        assert_ten_bit_empty_stream_trailer(VideoCodec::Av1);
    }

    fn assert_ten_bit_empty_stream_trailer(codec: VideoCodec) {
        let desc = FrameDesc::host_p010_le(128, 128, ColorSpace::default()).unwrap();
        let output = OutputPath(std::env::temp_dir().join(format!(
            "asciiflow-{codec}-10bit-empty-{}.mp4",
            std::process::id()
        )));
        let mut encoder = Encoder::create_with_hardware_frames_codec_and_audio(
            &output.0,
            desc,
            Rational::new(30, 1).unwrap(),
            codec,
            VaapiOptions::default(),
            Vec::new(),
            CancellationToken::new(),
        )
        .unwrap();
        encoder.finish().unwrap();
        drop(encoder);
        let bytes = std::fs::read(&output.0).unwrap();
        assert!(bytes.windows(4).any(|window| window == b"ftyp"));
        assert!(bytes.windows(4).any(|window| window == b"moov"));
    }

    #[test]
    fn injected_audio_mux_failure_is_the_pipeline_root_cause() {
        let input =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/media/single.mp4");
        let output = OutputPath(std::env::temp_dir().join(format!(
            "asciiflow-audio-mux-failure-{}.mp4",
            std::process::id()
        )));
        let mut decoder = Decoder::open(input).unwrap();
        let plan = AudioPlan::select(AudioPolicy::Copy, &decoder.info().audio_streams).unwrap();
        let cancellation = CancellationToken::new();
        let encoder = Encoder::create_with_audio(
            &output.0,
            decoder.info().frame_desc.clone(),
            decoder.info().frame_rate,
            EncodeMode::Software,
            VaapiOptions::default(),
            decoder.audio_output_templates(&plan).unwrap(),
            cancellation.clone(),
        )
        .unwrap();
        encoder
            .send_mux_message(MuxMessage::FailAudioAfter(2))
            .unwrap();
        decoder.attach_audio_passthrough(&plan, encoder.audio_packet_sender());
        let error = Pipeline::new(2)
            .unwrap()
            .run_with_cancellation(
                decoder,
                Identity,
                encoder,
                AsciiConfig::default(),
                cancellation.clone(),
            )
            .unwrap_err();
        assert_eq!(error.stage(), Some(PipelineStage::MuxRuntime));
        assert!(
            error.to_string().contains("injected audio mux failure"),
            "{error}"
        );
        assert!(!error.to_string().contains("disconnected"));
        assert!(cancellation.is_cancelled());
    }

    #[test]
    #[ignore = "requires Main10 VAAPI and a 10-bit AAC input at ASCIIFLOW_STAGE52C2_AAC_INPUT"]
    fn injected_main10_audio_mux_failure_preserves_root_cause() {
        assert_ten_bit_audio_mux_failure(VideoCodec::Hevc, "ASCIIFLOW_STAGE52C2_AAC_INPUT");
    }

    #[test]
    #[ignore = "requires AV1 10-bit VAAPI and a 10-bit AAC input at ASCIIFLOW_STAGE52C3_AAC_INPUT"]
    fn injected_av1_10bit_audio_mux_failure_preserves_root_cause() {
        assert_ten_bit_audio_mux_failure(VideoCodec::Av1, "ASCIIFLOW_STAGE52C3_AAC_INPUT");
    }

    fn assert_ten_bit_audio_mux_failure(codec: VideoCodec, input_env: &str) {
        let input = std::env::var_os(input_env)
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| "/tmp/asciiflow-main10-single-aac-input.mp4".into());
        let output = OutputPath(std::env::temp_dir().join(format!(
            "asciiflow-{codec}-10bit-audio-mux-failure-{}.mp4",
            std::process::id()
        )));
        let mut decoder = Decoder::open(&input).unwrap();
        assert_eq!(decoder.info().frame_desc.format, PixelFormat::P010Le);
        let plan = AudioPlan::select(AudioPolicy::Copy, &decoder.info().audio_streams).unwrap();
        let cancellation = CancellationToken::new();
        let encoder = Encoder::create_with_codec_and_audio(
            &output.0,
            decoder.info().frame_desc.clone(),
            decoder.info().frame_rate,
            OutputEncoding {
                codec,
                mode: EncodeMode::Vaapi,
            },
            VaapiOptions::default(),
            decoder.audio_output_templates(&plan).unwrap(),
            cancellation.clone(),
        )
        .unwrap();
        encoder
            .send_mux_message(MuxMessage::FailAudioAfter(2))
            .unwrap();
        decoder.attach_audio_passthrough(&plan, encoder.audio_packet_sender());
        let error = Pipeline::new(2)
            .unwrap()
            .run_with_cancellation(
                decoder,
                Identity,
                encoder,
                AsciiConfig::default(),
                cancellation.clone(),
            )
            .unwrap_err();
        assert_eq!(error.stage(), Some(PipelineStage::MuxRuntime));
        assert!(
            error.to_string().contains("injected audio mux failure"),
            "{error}"
        );
        assert!(cancellation.is_cancelled());
    }

    #[test]
    #[ignore = "requires Intel VAAPI HEVC encode"]
    fn injected_hevc_send_and_receive_failures_keep_encode_runtime_context() {
        let desc = FrameDesc::host_nv12(1920, 1080, ColorSpace::default()).unwrap();
        let frame =
            || VideoFrame::new_host(desc.clone(), Some(0), HostFrame::new_zeroed(&desc)).unwrap();
        for receive in [false, true] {
            let path = std::env::temp_dir().join(format!(
                "asciiflow-hevc-injected-{}-{}.mp4",
                if receive { "receive" } else { "send" },
                std::process::id()
            ));
            let mut encoder = Encoder::create_with_codec_and_audio(
                &path,
                desc.clone(),
                Rational::new(50, 1).unwrap(),
                OutputEncoding {
                    codec: VideoCodec::Hevc,
                    mode: EncodeMode::Vaapi,
                },
                VaapiOptions::default(),
                Vec::new(),
                CancellationToken::new(),
            )
            .unwrap();
            encoder.inject_send_failure = !receive;
            encoder.inject_receive_failure = receive;
            let error = encoder.encode(frame()).unwrap_err();
            assert_eq!(error.stage(), Some(PipelineStage::EncodeRuntime));
            assert!(error.to_string().contains(if receive {
                "injected HEVC receive_packet failure"
            } else {
                "injected HEVC send_frame failure"
            }));
            drop(encoder);
            std::fs::remove_file(path).unwrap();
        }
    }

    #[test]
    #[ignore = "requires Intel VAAPI HEVC Main10 encode"]
    fn injected_main10_send_receive_and_drain_failures_preserve_cause() {
        assert_ten_bit_send_receive_and_drain_failures(VideoCodec::Hevc);
    }

    #[test]
    #[ignore = "requires Intel VAAPI AV1 10-bit encode"]
    fn injected_av1_10bit_send_receive_and_drain_failures_preserve_cause() {
        assert_ten_bit_send_receive_and_drain_failures(VideoCodec::Av1);
    }

    fn assert_ten_bit_send_receive_and_drain_failures(codec: VideoCodec) {
        let desc = FrameDesc::host_p010_le(128, 128, ColorSpace::default()).unwrap();
        for failure in ["send", "receive", "drain"] {
            let path = std::env::temp_dir().join(format!(
                "asciiflow-{codec}-10bit-injected-{failure}-{}.mp4",
                std::process::id()
            ));
            let mut encoder = Encoder::create_with_codec_and_audio(
                &path,
                desc.clone(),
                Rational::new(30, 1).unwrap(),
                OutputEncoding {
                    codec: codec.clone(),
                    mode: EncodeMode::Vaapi,
                },
                VaapiOptions::default(),
                Vec::new(),
                CancellationToken::new(),
            )
            .unwrap();
            let frame = || {
                VideoFrame::new_host(desc.clone(), Some(0), HostFrame::new_zeroed(&desc)).unwrap()
            };
            let error = match failure {
                "send" => {
                    encoder.inject_send_failure = true;
                    encoder.encode(frame()).unwrap_err()
                }
                "receive" => {
                    encoder.inject_receive_failure = true;
                    encoder.encode(frame()).unwrap_err()
                }
                "drain" => {
                    encoder.encode(frame()).unwrap();
                    encoder.inject_receive_failure = true;
                    encoder.finish().unwrap_err()
                }
                _ => unreachable!(),
            };
            assert!(
                error.to_string().contains(if failure == "send" {
                    "send_frame failure"
                } else {
                    "receive_packet failure"
                }),
                "{error}"
            );
            assert!(error.to_string().contains(&codec.to_string()), "{error}");
            drop(encoder);
            std::fs::remove_file(path).unwrap();
        }
    }

    #[test]
    #[ignore = "requires Intel VAAPI AV1 encode"]
    fn injected_av1_send_and_receive_failures_keep_encode_runtime_context() {
        let desc = FrameDesc::host_nv12(1920, 1080, ColorSpace::default()).unwrap();
        let frame =
            || VideoFrame::new_host(desc.clone(), Some(0), HostFrame::new_zeroed(&desc)).unwrap();
        for receive in [false, true] {
            let path = std::env::temp_dir().join(format!(
                "asciiflow-av1-injected-{}-{}.mp4",
                if receive { "receive" } else { "send" },
                std::process::id()
            ));
            let mut encoder = Encoder::create_with_codec_and_audio(
                &path,
                desc.clone(),
                Rational::new(50, 1).unwrap(),
                OutputEncoding {
                    codec: VideoCodec::Av1,
                    mode: EncodeMode::Vaapi,
                },
                VaapiOptions::default(),
                Vec::new(),
                CancellationToken::new(),
            )
            .unwrap();
            encoder.inject_send_failure = !receive;
            encoder.inject_receive_failure = receive;
            let error = encoder.encode(frame()).unwrap_err();
            assert_eq!(error.stage(), Some(PipelineStage::EncodeRuntime));
            assert!(error.to_string().contains(if receive {
                "injected AV1 receive_packet failure"
            } else {
                "injected AV1 send_frame failure"
            }));
            drop(encoder);
            std::fs::remove_file(path).unwrap();
        }
    }

    #[test]
    #[ignore = "requires Intel VAAPI AV1 encode"]
    fn injected_av1_video_mux_failure_keeps_mux_root_cause() {
        let desc = FrameDesc::host_nv12(128, 96, ColorSpace::default()).unwrap();
        let path = std::env::temp_dir().join(format!(
            "asciiflow-av1-injected-video-mux-{}.mp4",
            std::process::id()
        ));
        let mut encoder = Encoder::create_with_codec_and_audio(
            &path,
            desc.clone(),
            Rational::new(50, 1).unwrap(),
            OutputEncoding {
                codec: VideoCodec::Av1,
                mode: EncodeMode::Vaapi,
            },
            VaapiOptions::default(),
            Vec::new(),
            CancellationToken::new(),
        )
        .unwrap();
        encoder
            .send_mux_message(MuxMessage::FailVideoAfter(0))
            .unwrap();
        let frame =
            VideoFrame::new_host(desc.clone(), Some(0), HostFrame::new_zeroed(&desc)).unwrap();
        let failure = encoder
            .encode(frame)
            .and_then(|_| encoder.finish())
            .unwrap_err();
        assert_eq!(failure.stage(), Some(PipelineStage::MuxRuntime));
        assert!(failure.to_string().contains("injected video mux failure"));
        drop(encoder);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    #[ignore = "requires Intel iHD AV1 encoder; dimensions are device-specific"]
    fn intel_av1_dimension_boundary_is_reported_during_encoder_probe() {
        let fps = Rational::new(50, 1).unwrap();
        for (width, height, supported) in [
            (64, 96, false),
            (128, 64, false),
            (126, 96, true),
            (128, 94, true),
            (128, 96, true),
        ] {
            let desc = FrameDesc::host_nv12(width, height, ColorSpace::default()).unwrap();
            let result =
                probe_vaapi_encoder_for(VideoCodec::Av1, desc, fps, VaapiOptions::default());
            if supported {
                assert!(result.is_ok(), "{width}x{height} AV1 probe should succeed");
            } else {
                let error = result.err().expect("sub-minimum AV1 encode should fail");
                let message = error.to_string();
                assert!(message.contains(&format!("{width}x{height}")), "{message}");
                assert!(message.contains("NV12"), "{message}");
            }
        }
    }

    #[test]
    #[ignore = "requires Intel iHD HEVC encoder; dimensions are device-specific"]
    fn intel_hevc_dimension_boundary_is_reported_during_encoder_probe() {
        let fps = Rational::new(50, 1).unwrap();
        // FFmpeg may pad a visible 126 dimension to a coded 128 surface.
        for (width, height, supported) in [
            (64, 128, false),
            (128, 64, false),
            (126, 128, true),
            (128, 126, true),
            (128, 128, true),
        ] {
            let desc = FrameDesc::host_nv12(width, height, ColorSpace::default()).unwrap();
            let result =
                probe_vaapi_encoder_for(VideoCodec::Hevc, desc, fps, VaapiOptions::default());
            if supported {
                assert!(result.is_ok(), "{width}x{height} HEVC probe should succeed");
            } else {
                let error = result.err().expect("sub-minimum HEVC encode should fail");
                let message = error.to_string();
                assert!(message.contains(&format!("{width}x{height}")), "{message}");
                assert!(message.contains("NV12"), "{message}");
            }
        }
    }
}
