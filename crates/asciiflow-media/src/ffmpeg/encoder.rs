use super::{
    codec::{again, check, ffmpeg_error},
    ffi,
    frame::Frame,
    hwdevice::HardwareDevice,
    hwframes::{HardwareFrame, HardwareFramesPool, VaapiEncoderFrame, VaapiEncoderFrames},
    packet::Packet,
    vaapi::{EncodeMode, VaapiOptions},
};
use asciiflow_core::{
    Error, FrameDesc, FrameSink, PipelineStage, Rational, Result, SinkTimings, VideoFrame,
};
use std::{
    ffi::CString,
    path::Path,
    ptr::{self, NonNull},
    time::Instant,
};

pub struct Encoder {
    format: NonNull<ffi::AVFormatContext>,
    codec: NonNull<ffi::AVCodecContext>,
    stream: NonNull<ffi::AVStream>,
    frame: Frame,
    hardware_frame: Option<HardwareFrame>,
    frames_pool: Option<HardwareFramesPool>,
    _hardware_device: Option<HardwareDevice>,
    packet: Packet,
    mode: EncodeMode,
    timings: SinkTimings,
    desc: FrameDesc,
    next_pts: i64,
    finished: bool,
    finish_attempted: bool,
}

pub struct VaapiEncoderProbe {
    pub frames: VaapiEncoderFrames,
    pub host_upload_supported: bool,
}

pub fn probe_vaapi_encoder(
    desc: FrameDesc,
    frame_rate: Rational,
    vaapi: VaapiOptions,
) -> Result<VaapiEncoderProbe> {
    let name = CString::new("h264_vaapi").unwrap();
    let encoder = unsafe { ffi::avcodec_find_encoder_by_name(name.as_ptr()) };
    if encoder.is_null() {
        return Err(asciiflow_core::Error::Media(
            "this FFmpeg build has no h264_vaapi encoder".into(),
        ));
    }
    require_vaapi_encoder(encoder)?;
    let codec = NonNull::new(unsafe { ffi::avcodec_alloc_context3(encoder) }).ok_or_else(|| {
        asciiflow_core::Error::Media("failed to allocate H.264 encoder probe context".into())
    })?;
    let _guard = CodecGuard(Some(codec));
    unsafe {
        (*codec.as_ptr()).codec_id = (*encoder).id;
        (*codec.as_ptr()).codec_type = ffi::AVMediaType::AVMEDIA_TYPE_VIDEO;
        (*codec.as_ptr()).width = desc.width as i32;
        (*codec.as_ptr()).height = desc.height as i32;
        (*codec.as_ptr()).pix_fmt = ffi::AVPixelFormat::AV_PIX_FMT_VAAPI;
        (*codec.as_ptr()).time_base = ffi::AVRational {
            num: frame_rate.denominator,
            den: frame_rate.numerator,
        };
        (*codec.as_ptr()).framerate = ffi::AVRational {
            num: frame_rate.numerator,
            den: frame_rate.denominator,
        };
        (*codec.as_ptr()).color_range = ffi::AVColorRange::AVCOL_RANGE_MPEG;
        (*codec.as_ptr()).colorspace = ffi::AVColorSpace::AVCOL_SPC_BT709;
        (*codec.as_ptr()).color_primaries = ffi::AVColorPrimaries::AVCOL_PRI_BT709;
        (*codec.as_ptr()).color_trc = ffi::AVColorTransferCharacteristic::AVCOL_TRC_BT709;
        (*codec.as_ptr()).chroma_sample_location = ffi::AVChromaLocation::AVCHROMA_LOC_LEFT;
        (*codec.as_ptr()).max_b_frames = 0;
        (*codec.as_ptr()).gop_size = 250;
    }
    let device = vaapi.create_device()?;
    let pool = HardwareFramesPool::vaapi_nv12(&device, desc.width, desc.height)?;
    unsafe { (*codec.as_ptr()).hw_frames_ctx = pool.try_clone_ref()? };
    let mut options = ptr::null_mut();
    for (key, value) in [("rc_mode", "CQP"), ("qp", "20"), ("async_depth", "2")] {
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
    check(opened, "failed to configure h264_vaapi encoder probe")?;
    if unused_options != 0 {
        return Err(asciiflow_core::Error::Media(format!(
            "H.264 encoder probe rejected {unused_options} configuration option(s)"
        )));
    }
    Ok(VaapiEncoderProbe {
        frames: VaapiEncoderFrames::from_pool(&pool, desc)?,
        host_upload_supported: pool.supports_upload_nv12()?,
    })
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
        Self::create_internal(path, desc, frame_rate, mode, vaapi, true)
    }

    pub fn create_with_hardware_frames(
        path: impl AsRef<Path>,
        desc: FrameDesc,
        frame_rate: Rational,
        vaapi: VaapiOptions,
    ) -> Result<Self> {
        Self::create_internal(path, desc, frame_rate, EncodeMode::Vaapi, vaapi, false)
    }

    fn create_internal(
        path: impl AsRef<Path>,
        desc: FrameDesc,
        frame_rate: Rational,
        mode: EncodeMode,
        vaapi: VaapiOptions,
        require_host_upload: bool,
    ) -> Result<Self> {
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
        let name = CString::new(if mode == EncodeMode::Vaapi {
            "h264_vaapi"
        } else {
            "libx264"
        })
        .unwrap();
        let mut encoder = unsafe { ffi::avcodec_find_encoder_by_name(name.as_ptr()) };
        if encoder.is_null() && mode == EncodeMode::Software {
            encoder = unsafe { ffi::avcodec_find_encoder(ffi::AVCodecID::AV_CODEC_ID_H264) };
        }
        if encoder.is_null() {
            return Err(asciiflow_core::Error::Media(
                if mode == EncodeMode::Vaapi {
                    "this FFmpeg build has no h264_vaapi encoder"
                } else {
                    "this FFmpeg build has no H.264 encoder"
                }
                .into(),
            ));
        }
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
                asciiflow_core::Error::Media("failed to allocate H.264 encoder context".into())
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
            (*codec.as_ptr()).color_range = ffi::AVColorRange::AVCOL_RANGE_MPEG;
            (*codec.as_ptr()).colorspace = ffi::AVColorSpace::AVCOL_SPC_BT709;
            (*codec.as_ptr()).color_primaries = ffi::AVColorPrimaries::AVCOL_PRI_BT709;
            (*codec.as_ptr()).color_trc = ffi::AVColorTransferCharacteristic::AVCOL_TRC_BT709;
            (*codec.as_ptr()).chroma_sample_location = ffi::AVChromaLocation::AVCHROMA_LOC_LEFT;
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
            let pool = HardwareFramesPool::vaapi_nv12(&device, desc.width, desc.height)?;
            if require_host_upload && !pool.supports_upload_nv12()? {
                return Err(asciiflow_core::Error::UnsupportedFrame(
                    "VAAPI device cannot upload Host NV12 frames".into(),
                ));
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
            // Stable throughput baseline. CQP is deliberately not presented as
            // quality-equivalent to libx264 CRF 20.
            &[("rc_mode", "CQP"), ("qp", "20"), ("async_depth", "2")]
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
        check(
            opened,
            if mode == EncodeMode::Vaapi {
                "failed to configure h264_vaapi encoder"
            } else {
                "failed to configure software H.264 encoder"
            },
        )?;
        if unused_options != 0 {
            return Err(asciiflow_core::Error::Media(format!(
                "H.264 encoder rejected {unused_options} configuration option(s)"
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
        let mut frame = Frame::new()?;
        unsafe {
            (*frame.as_mut_ptr()).format = ffi::AVPixelFormat::AV_PIX_FMT_NV12 as i32;
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
            "opened H.264 encoder"
        );
        let format = format_guard.take();
        let codec = codec_guard.take();
        Ok(Self {
            format,
            codec,
            stream,
            frame,
            hardware_frame,
            frames_pool,
            _hardware_device: hardware_device,
            packet,
            mode,
            timings: SinkTimings::default(),
            desc,
            next_pts: 0,
            finished: false,
            finish_attempted: false,
        })
    }
    fn drain_packets(&mut self) -> Result<()> {
        loop {
            let result = unsafe {
                ffi::avcodec_receive_packet(self.codec.as_ptr(), self.packet.as_mut_ptr())
            };
            if again(result) || result == ffi::AVERROR_EOF {
                return Ok(());
            }
            if result < 0 {
                return Err(ffmpeg_error("failed to receive H.264 packet", result));
            }
            unsafe {
                ffi::av_packet_rescale_ts(
                    self.packet.as_mut_ptr(),
                    (*self.codec.as_ptr()).time_base,
                    (*self.stream.as_ptr()).time_base,
                );
                if (*self.packet.as_mut_ptr()).duration == 0 {
                    (*self.packet.as_mut_ptr()).duration = ffi::av_rescale_q(
                        1,
                        (*self.codec.as_ptr()).time_base,
                        (*self.stream.as_ptr()).time_base,
                    );
                }
                (*self.packet.as_mut_ptr()).stream_index = (*self.stream.as_ptr()).index;
            }
            let written = unsafe {
                ffi::av_interleaved_write_frame(self.format.as_ptr(), self.packet.as_mut_ptr())
            };
            self.packet.unref();
            check(written, "failed to mux H.264 packet").map_err(|error| {
                Error::pipeline(PipelineStage::MuxRuntime, "write encoded packet", error)
            })?;
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
        check(
            unsafe { ffi::avcodec_send_frame(self.codec.as_ptr(), frame) },
            "failed to send NV12 frame to H.264 encoder",
        )?;
        self.next_pts += 1;
        let result = self.drain_packets();
        self.timings.submit_receive += encode_started.elapsed();
        result
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
        let width = self.desc.width as usize;
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
            check(uploaded, "failed to upload Host NV12 frame to VAAPI")?;
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
            let result = unsafe { ffi::avcodec_send_frame(self.codec.as_ptr(), ptr::null()) };
            if result < 0 && result != ffi::AVERROR_EOF {
                return Err(ffmpeg_error("failed to flush H.264 encoder", result));
            }
            self.drain_packets()?;
        }
        self.timings.submit_receive += encode_started.elapsed();
        check(
            unsafe { ffi::av_write_trailer(self.format.as_ptr()) },
            "failed to finalize MP4 output",
        )
        .map_err(|error| {
            Error::pipeline(
                PipelineStage::Finalization,
                "write MP4 container trailer",
                error,
            )
        })?;
        self.finished = true;
        Ok(())
    }

    fn take_timings(&mut self) -> SinkTimings {
        std::mem::take(&mut self.timings)
    }
}
impl Drop for Encoder {
    fn drop(&mut self) {
        unsafe {
            let mut codec = self.codec.as_ptr();
            ffi::avcodec_free_context(&mut codec);
            if !(*self.format.as_ptr()).pb.is_null() {
                ffi::avio_closep(&mut (*self.format.as_ptr()).pb);
            }
            ffi::avformat_free_context(self.format.as_ptr());
        }
    }
}
unsafe impl Send for Encoder {}

fn require_vaapi_encoder(encoder: *const ffi::AVCodec) -> Result<()> {
    let mut index = 0;
    loop {
        let config = unsafe { ffi::avcodec_get_hw_config(encoder, index) };
        if config.is_null() {
            return Err(asciiflow_core::Error::Media(
                "h264_vaapi has no VAAPI hardware-frames configuration".into(),
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
