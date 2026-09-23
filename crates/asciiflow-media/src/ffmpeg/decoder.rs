use super::{
    audio::{
        AudioInputStream, AudioOutputTemplate, AudioPacketSender, discover_audio_streams,
        selected_templates,
    },
    codec::{again, check, ffmpeg_error},
    ffi,
    frame::Frame,
    hwdevice::HardwareDevice,
    hwframes::{supports_download_nv12, supports_download_p010},
    packet::Packet,
    vaapi::{DecodeMode, VaapiOptions},
};
use asciiflow_core::{
    AudioPlan, AudioStreamInfo, ChromaLocation, ChromaSubsampling, ColorMatrix, ColorPrimaries,
    ColorRange, ColorSpace, FrameDesc, FrameSource, HostFrame, InputRequirements, PixelFormat,
    Rational, Result, SourceTimings, TransferCharacteristic, VideoCodec, VideoFrame, VideoProfile,
};
use std::{
    ffi::{CStr, CString},
    path::Path,
    ptr::{self, NonNull},
    time::Instant,
};

#[derive(Clone, Debug)]
pub struct MediaInfo {
    pub frame_desc: FrameDesc,
    pub frame_rate: Rational,
    pub frame_count: Option<u64>,
    pub requirements: InputRequirements,
    pub audio_streams: Vec<AudioStreamInfo>,
}

pub struct Decoder {
    format: NonNull<ffi::AVFormatContext>,
    codec: NonNull<ffi::AVCodecContext>,
    scaler: Option<NonNull<ffi::SwsContext>>,
    stream_index: i32,
    source_frame: Frame,
    download_frame: Option<Frame>,
    packet: Packet,
    _hardware_device: Option<HardwareDevice>,
    mode: DecodeMode,
    timings: SourceTimings,
    source_matrix: ColorMatrix,
    source_range: ColorRange,
    info: MediaInfo,
    input_eof: bool,
    drain_sent: bool,
    packet_pending: bool,
    download_format_checked: bool,
    format_locked: bool,
    audio_streams: Vec<AudioInputStream>,
    selected_audio_streams: Vec<usize>,
    audio_sender: Option<AudioPacketSender>,
    audio_video_origin: Option<i64>,
    audio_video_frames: i64,
}

/// An owned reference to a decoded VAAPI surface.
///
/// Dropping this value releases the FFmpeg frame reference. Consumers must
/// retain it until all external users have finished reading the surface.
pub struct VaapiDecodedFrame {
    frame: Frame,
    desc: FrameDesc,
    pts: Option<i64>,
}

impl VaapiDecodedFrame {
    pub fn desc(&self) -> &FrameDesc {
        &self.desc
    }

    pub fn pts(&self) -> Option<i64> {
        self.pts
    }

    /// Returns the native frame for the narrowly-scoped interop layer.
    ///
    /// # Safety
    ///
    /// The pointer is borrowed from `self`, must not be freed or mutated, and
    /// must not be used after `self` is dropped.
    pub unsafe fn as_raw_ptr(&self) -> *const ffi::AVFrame {
        self.frame.as_ptr()
    }
}

unsafe impl Send for VaapiDecodedFrame {}

impl Decoder {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with(path, DecodeMode::Software, VaapiOptions::default())
    }

    pub fn open_with(
        path: impl AsRef<Path>,
        mode: DecodeMode,
        vaapi: VaapiOptions,
    ) -> Result<Self> {
        let path = path.as_ref();
        let native = CString::new(path.as_os_str().as_encoded_bytes())
            .map_err(|_| asciiflow_core::Error::Media("input path contains a NUL byte".into()))?;
        let mut format = ptr::null_mut();
        check(
            unsafe {
                ffi::avformat_open_input(&mut format, native.as_ptr(), ptr::null(), ptr::null_mut())
            },
            &format!("failed to open input {}", path.display()),
        )?;
        let format =
            NonNull::new(format).expect("FFmpeg returned success with null format context");
        let mut guard = FormatGuard(Some(format));
        check(
            unsafe { ffi::avformat_find_stream_info(format.as_ptr(), ptr::null_mut()) },
            "failed to read stream information",
        )?;
        let stream_index = unsafe {
            ffi::av_find_best_stream(
                format.as_ptr(),
                ffi::AVMediaType::AVMEDIA_TYPE_VIDEO,
                -1,
                -1,
                ptr::null_mut(),
                0,
            )
        };
        if stream_index < 0 {
            return Err(ffmpeg_error(
                "input contains no decodable video stream",
                stream_index,
            ));
        }
        let stream = unsafe { *(*format.as_ptr()).streams.add(stream_index as usize) };
        let parameters = unsafe { (*stream).codecpar };
        let decoder = super::vaapi::select_decoder(unsafe { (*parameters).codec_id }, mode)?;
        if decoder.is_null() {
            return Err(asciiflow_core::Error::Media(
                "no decoder is available for the input video codec".into(),
            ));
        }
        let codec =
            NonNull::new(unsafe { ffi::avcodec_alloc_context3(decoder) }).ok_or_else(|| {
                asciiflow_core::Error::Media("failed to allocate decoder context".into())
            })?;
        let mut codec_guard = CodecGuard(Some(codec));
        check(
            unsafe { ffi::avcodec_parameters_to_context(codec.as_ptr(), parameters) },
            "failed to configure decoder from stream",
        )?;
        let hardware_device = if mode == DecodeMode::Vaapi {
            require_vaapi_decoder(decoder)?;
            let device = vaapi.create_device()?;
            unsafe {
                (*codec.as_ptr()).get_format = Some(select_vaapi_format);
                (*codec.as_ptr()).hw_device_ctx = device.try_clone_ref()?;
            }
            Some(device)
        } else {
            None
        };
        check(
            unsafe { ffi::avcodec_open2(codec.as_ptr(), decoder, ptr::null_mut()) },
            if mode == DecodeMode::Vaapi {
                "failed to open VAAPI hardware decoder"
            } else {
                "failed to open software decoder"
            },
        )?;
        let source_width = unsafe { (*codec.as_ptr()).width };
        let source_height = unsafe { (*codec.as_ptr()).height };
        if source_width <= 0 || source_height <= 0 {
            return Err(asciiflow_core::Error::Media(
                "decoder reported invalid video dimensions".into(),
            ));
        }
        let width = ((source_width as u32) + 1) & !1;
        let height = ((source_height as u32) + 1) & !1;
        let source_matrix = match unsafe { (*codec.as_ptr()).colorspace } {
            ffi::AVColorSpace::AVCOL_SPC_BT709 => ColorMatrix::Bt709,
            ffi::AVColorSpace::AVCOL_SPC_BT470BG | ffi::AVColorSpace::AVCOL_SPC_SMPTE170M => {
                ColorMatrix::Bt601
            }
            _ => ColorMatrix::Unspecified,
        };
        let source_range = match unsafe { (*codec.as_ptr()).color_range } {
            ffi::AVColorRange::AVCOL_RANGE_JPEG => ColorRange::Full,
            ffi::AVColorRange::AVCOL_RANGE_MPEG => ColorRange::Limited,
            _ => ColorRange::Unspecified,
        };
        let color_space = ColorSpace {
            matrix: ColorMatrix::Bt709,
            range: ColorRange::Limited,
            primaries: ColorPrimaries::Bt709,
            transfer: TransferCharacteristic::Bt709,
            chroma_location: ChromaLocation::Left,
        };
        let guessed = unsafe { ffi::av_guess_frame_rate(format.as_ptr(), stream, ptr::null_mut()) };
        let frame_rate = if guessed.num > 0 && guessed.den > 0 {
            Rational::new(guessed.num, guessed.den)?
        } else {
            Rational::new(30, 1)?
        };
        let frame_count = unsafe {
            if (*stream).nb_frames > 0 {
                Some((*stream).nb_frames as u64)
            } else {
                None
            }
        };
        let requirements = input_requirements(
            parameters,
            source_width as u32,
            source_height as u32,
            frame_rate,
            ColorSpace {
                matrix: source_matrix,
                range: source_range,
                primaries: map_primaries(unsafe { (*codec.as_ptr()).color_primaries }),
                transfer: map_transfer(unsafe { (*codec.as_ptr()).color_trc }),
                chroma_location: map_chroma_location(unsafe {
                    (*codec.as_ptr()).chroma_sample_location
                }),
            },
        );
        if has_hdr_stream_side_data(parameters)? {
            return Err(asciiflow_core::Error::UnsupportedFrame(
                "HDR mastering/display stream metadata is not supported".into(),
            ));
        }
        let frame_desc = if requirements.bit_depth == Some(10) {
            FrameDesc::host_p010_le(width, height, requirements.color_space)?
        } else {
            FrameDesc::host_nv12(width, height, color_space)?
        };
        let audio_streams = discover_audio_streams(format)?;
        let audio_info = audio_streams
            .iter()
            .map(|stream| stream.info.clone())
            .collect();
        let decoder_pixel_format = unsafe { (*codec.as_ptr()).pix_fmt };
        let scaler = if frame_desc.format == PixelFormat::Nv12
            && mode == DecodeMode::Software
            && decoder_pixel_format != ffi::AVPixelFormat::AV_PIX_FMT_NONE
        {
            Some(create_scaler(
                source_width,
                source_height,
                decoder_pixel_format,
                width,
                height,
                source_matrix,
                source_range,
            )?)
        } else {
            None
        };
        let mut scaler_guard = ScalerGuard(scaler);
        let source_frame = Frame::new()?;
        let download_frame = (mode == DecodeMode::Vaapi).then(Frame::new).transpose()?;
        let packet = Packet::new()?;
        tracing::debug!(
            input = %path.display(),
            source_width,
            source_height,
            nv12_width = width,
            nv12_height = height,
            fps_numerator = frame_rate.numerator,
            fps_denominator = frame_rate.denominator,
            mode = ?mode,
            vaapi_device = vaapi.display_device(),
            "opened decoder"
        );
        let format = guard.take();
        let codec = codec_guard.take();
        Ok(Self {
            format,
            codec,
            scaler: scaler_guard.take_optional(),
            stream_index,
            source_frame,
            download_frame,
            packet,
            _hardware_device: hardware_device,
            mode,
            timings: SourceTimings::default(),
            source_matrix,
            source_range,
            info: MediaInfo {
                frame_desc,
                frame_rate,
                frame_count,
                requirements,
                audio_streams: audio_info,
            },
            input_eof: false,
            drain_sent: false,
            packet_pending: false,
            download_format_checked: false,
            format_locked: false,
            audio_streams,
            selected_audio_streams: Vec::new(),
            audio_sender: None,
            audio_video_origin: None,
            audio_video_frames: 0,
        })
    }
    pub fn info(&self) -> &MediaInfo {
        &self.info
    }
    pub fn audio_output_templates(&self, plan: &AudioPlan) -> Result<Vec<AudioOutputTemplate>> {
        selected_templates(&self.audio_streams, plan)
    }

    pub fn attach_audio_passthrough(&mut self, plan: &AudioPlan, sender: AudioPacketSender) {
        self.selected_audio_streams = plan
            .selected
            .iter()
            .map(|stream| stream.input_index)
            .collect();
        self.audio_sender = (!self.selected_audio_streams.is_empty()).then_some(sender);
    }
    fn receive_native(&mut self) -> Result<ReceiveResult> {
        let receive_started = Instant::now();
        let result = unsafe {
            ffi::avcodec_receive_frame(self.codec.as_ptr(), self.source_frame.as_mut_ptr())
        };
        self.timings.frame_receive += receive_started.elapsed();
        if result == 0 {
            let native = unsafe { &*self.source_frame.as_mut_ptr() };
            let pts = if native.best_effort_timestamp == ffi::AV_NOPTS_VALUE {
                None
            } else {
                Some(native.best_effort_timestamp)
            };
            if self.mode == DecodeMode::Vaapi
                && (native.format != ffi::AVPixelFormat::AV_PIX_FMT_VAAPI as i32
                    || native.hw_frames_ctx.is_null())
            {
                self.source_frame.unref();
                return Err(asciiflow_core::Error::Media(
                    "VAAPI decode was requested, but the decoder returned a software frame".into(),
                ));
            }
            let format = if self.mode == DecodeMode::Vaapi {
                if native.hw_frames_ctx.is_null() {
                    return Err(asciiflow_core::Error::Media(
                        "VAAPI decoded frame has no hardware frames context".into(),
                    ));
                }
                let frames = unsafe {
                    &*((*native.hw_frames_ctx)
                        .data
                        .cast::<ffi::AVHWFramesContext>())
                };
                frames.sw_format as i32
            } else {
                native.format
            };
            if self.mode == DecodeMode::Vaapi
                && self.info.requirements.bit_depth == Some(10)
                && format != ffi::AVPixelFormat::AV_PIX_FMT_P010LE as i32
            {
                return Err(asciiflow_core::Error::UnsupportedFrame(format!(
                    "VAAPI 10-bit frames context uses software format {format}; expected P010LE"
                )));
            }
            let pixel = if (0..ffi::AVPixelFormat::AV_PIX_FMT_NB as i32).contains(&format) {
                unsafe {
                    ffi::av_pix_fmt_desc_get(std::mem::transmute::<i32, ffi::AVPixelFormat>(format))
                        .as_ref()
                }
            } else {
                None
            };
            let mut actual = self.info.requirements.clone();
            actual.bit_depth = pixel.map(|p| p.comp[0].depth as u8);
            actual.pixel_format = pixel.map(|p| {
                unsafe { CStr::from_ptr(p.name) }
                    .to_string_lossy()
                    .into_owned()
            });
            actual.chroma_subsampling = pixel.map_or(ChromaSubsampling::Unknown, |p| {
                if p.nb_components == 3 && p.log2_chroma_w == 1 && p.log2_chroma_h == 1 {
                    ChromaSubsampling::Yuv420
                } else {
                    ChromaSubsampling::Other
                }
            });
            let frame_color = ColorSpace {
                matrix: map_matrix(native.colorspace),
                range: map_range(native.color_range),
                primaries: map_primaries(native.color_primaries),
                transfer: map_transfer(native.color_trc),
                chroma_location: map_chroma_location(native.chroma_location),
            };
            if actual.bit_depth == Some(10) {
                merge_frame_color(&mut actual.color_space, frame_color)?;
            }
            if matches!(frame_color.primaries, ColorPrimaries::Bt2020)
                || matches!(
                    frame_color.transfer,
                    TransferCharacteristic::Pq | TransferCharacteristic::Hlg
                )
            {
                return Err(asciiflow_core::Error::UnsupportedFrame(
                    "HDR/BT.2020 input is not supported by the SDR processing pipeline".into(),
                ));
            }
            let has_hdr_side_data = unsafe {
                !ffi::av_frame_get_side_data(
                    self.source_frame.as_mut_ptr(),
                    ffi::AVFrameSideDataType::AV_FRAME_DATA_MASTERING_DISPLAY_METADATA,
                )
                .is_null()
                    || !ffi::av_frame_get_side_data(
                        self.source_frame.as_mut_ptr(),
                        ffi::AVFrameSideDataType::AV_FRAME_DATA_CONTENT_LIGHT_LEVEL,
                    )
                    .is_null()
            };
            if has_hdr_side_data {
                return Err(asciiflow_core::Error::UnsupportedFrame(
                    "HDR mastering/display frame metadata is not supported".into(),
                ));
            }
            let working_format = actual.validate_processing_input().map_err(|e| {
                asciiflow_core::Error::Media(format!(
                    "decoded {:?} profile {:?}, {:?}-bit {:?}, requested {:?}: {e}",
                    actual.codec,
                    actual.profile,
                    actual.bit_depth,
                    actual.chroma_subsampling,
                    self.mode
                ))
            })?;
            if self.format_locked && self.info.frame_desc.format != working_format {
                return Err(asciiflow_core::Error::UnsupportedFrame(
                    "decoded pixel format changed after the first frame".into(),
                ));
            }
            if !self.format_locked {
                if working_format == PixelFormat::P010Le
                    && let Some(scaler) = self.scaler.take()
                {
                    unsafe { ffi::sws_freeContext(scaler.as_ptr()) };
                }
                self.info.frame_desc = match working_format {
                    PixelFormat::Nv12 => FrameDesc::host_nv12(
                        self.info.frame_desc.width,
                        self.info.frame_desc.height,
                        ColorSpace::default(),
                    )?,
                    PixelFormat::P010Le => FrameDesc::host_p010_le(
                        self.info.frame_desc.width,
                        self.info.frame_desc.height,
                        actual.color_space,
                    )?,
                };
                self.format_locked = true;
            }
            self.info.requirements = actual;
            return Ok(ReceiveResult::Frame(pts));
        }
        if result == ffi::AVERROR_EOF {
            Ok(ReceiveResult::Eof)
        } else if again(result) {
            Ok(ReceiveResult::Again)
        } else {
            Err(ffmpeg_error("failed to receive decoded frame", result))
        }
    }

    fn next_native_frame(&mut self) -> Result<Option<Option<i64>>> {
        loop {
            if let Some(sender) = &self.audio_sender {
                sender.check_active()?;
            }
            match self.receive_native()? {
                ReceiveResult::Frame(pts) => {
                    if let Some(sender) = &self.audio_sender {
                        let pts = pts.ok_or_else(|| {
                            asciiflow_core::Error::Media(
                                "audio passthrough requires video presentation timestamps".into(),
                            )
                        })?;
                        let stream = unsafe {
                            *(*self.format.as_ptr())
                                .streams
                                .add(self.stream_index as usize)
                        };
                        let time_base = unsafe { (*stream).time_base };
                        let origin = *self.audio_video_origin.get_or_insert(pts);
                        if self.audio_video_frames == 0 {
                            sender.video_origin(origin, time_base)?;
                        }
                        let expected_delta = unsafe {
                            ffi::av_rescale_q(
                                self.audio_video_frames,
                                ffi::AVRational {
                                    num: self.info.frame_rate.denominator,
                                    den: self.info.frame_rate.numerator,
                                },
                                time_base,
                            )
                        };
                        let expected = origin.checked_add(expected_delta).ok_or_else(|| {
                            asciiflow_core::Error::Media("video timeline overflow".into())
                        })?;
                        if pts.abs_diff(expected) > 1 {
                            return Err(asciiflow_core::Error::Media(format!(
                                "audio passthrough requires the existing CFR video timeline: frame {} has PTS {pts}, expected {expected}; variable-rate or discontinuous video needs a future timeline policy (use --audio none for video-only CFR output)",
                                self.audio_video_frames
                            )));
                        }
                        self.audio_video_frames =
                            self.audio_video_frames.checked_add(1).ok_or_else(|| {
                                asciiflow_core::Error::Media("video frame count overflow".into())
                            })?;
                    }
                    return Ok(Some(pts));
                }
                ReceiveResult::Eof => return Ok(None),
                ReceiveResult::Again => {}
            }
            if self.packet_pending {
                let submit_started = Instant::now();
                let sent = unsafe {
                    ffi::avcodec_send_packet(self.codec.as_ptr(), self.packet.as_mut_ptr())
                };
                self.timings.packet_submit += submit_started.elapsed();
                if sent == 0 {
                    self.packet.unref();
                    self.packet_pending = false;
                    continue;
                }
                if again(sent) {
                    return Err(asciiflow_core::Error::Media(
                        "decoder rejected a pending packet after all available frames were received"
                            .into(),
                    ));
                }
                return Err(ffmpeg_error(
                    "failed to send compressed packet to decoder",
                    sent,
                ));
            }
            if self.drain_sent {
                return Err(asciiflow_core::Error::Media(
                    "decoder requested more input after its drain packet".into(),
                ));
            }
            if self.input_eof {
                let submit_started = Instant::now();
                let result = unsafe { ffi::avcodec_send_packet(self.codec.as_ptr(), ptr::null()) };
                self.timings.packet_submit += submit_started.elapsed();
                if result < 0 && result != ffi::AVERROR_EOF {
                    return Err(ffmpeg_error("failed to drain decoder", result));
                }
                self.drain_sent = true;
                continue;
            }
            let result =
                unsafe { ffi::av_read_frame(self.format.as_ptr(), self.packet.as_mut_ptr()) };
            if result == ffi::AVERROR_EOF {
                self.input_eof = true;
                continue;
            }
            if result < 0 {
                return Err(ffmpeg_error(
                    "failed while reading compressed input",
                    result,
                ));
            }
            let packet_stream_index = unsafe { (*self.packet.as_mut_ptr()).stream_index };
            let is_video = packet_stream_index == self.stream_index;
            if is_video {
                self.packet_pending = true;
            } else {
                self.route_audio_packet(packet_stream_index)?;
            }
        }
    }

    fn route_audio_packet(&mut self, packet_stream_index: i32) -> Result<()> {
        if packet_stream_index >= 0
            && self
                .selected_audio_streams
                .contains(&(packet_stream_index as usize))
        {
            let input_index = packet_stream_index as usize;
            let time_base = self
                .audio_streams
                .iter()
                .find(|stream| stream.info.input_index == input_index)
                .map(|stream| stream.time_base)
                .ok_or_else(|| {
                    asciiflow_core::Error::Media(format!(
                        "selected audio stream #{input_index} has no demux descriptor"
                    ))
                })?;
            let mut packet = Packet::new()?;
            packet.take_from(&mut self.packet);
            self.audio_sender
                .as_ref()
                .expect("selected audio streams require a mux sender")
                .send(packet, input_index, time_base)?;
        } else {
            self.packet.unref();
        }
        Ok(())
    }

    fn convert_current_to_host(&mut self, pts: Option<i64>) -> Result<VideoFrame> {
        let desc = self.info.frame_desc.clone();
        let native = unsafe { &*self.source_frame.as_ptr() };
        let source_ptr = if self.mode == DecodeMode::Vaapi {
            if !self.download_format_checked {
                let can_download = match desc.format {
                    PixelFormat::Nv12 => supports_download_nv12(native.hw_frames_ctx)?,
                    PixelFormat::P010Le => supports_download_p010(native.hw_frames_ctx)?,
                };
                if !can_download {
                    self.source_frame.unref();
                    return Err(asciiflow_core::Error::UnsupportedFrame(format!(
                        "VAAPI decoded frame cannot be downloaded as {:?}",
                        desc.format
                    )));
                }
                self.download_format_checked = true;
            }
            let download = self
                .download_frame
                .as_mut()
                .expect("VAAPI decoder download frame missing");
            download.unref();
            let download_format = match desc.format {
                PixelFormat::Nv12 => ffi::AVPixelFormat::AV_PIX_FMT_NV12,
                PixelFormat::P010Le => ffi::AVPixelFormat::AV_PIX_FMT_P010LE,
            };
            unsafe { (*download.as_mut_ptr()).format = download_format as i32 };
            let transfer_started = Instant::now();
            let transferred = unsafe {
                ffi::av_hwframe_transfer_data(
                    download.as_mut_ptr(),
                    self.source_frame.as_mut_ptr(),
                    0,
                )
            };
            self.timings.hardware_download += transfer_started.elapsed();
            check(transferred, "failed to download VAAPI frame to Host memory")?;
            let downloaded = unsafe { &*download.as_mut_ptr() };
            if downloaded.format != download_format as i32 {
                return Err(asciiflow_core::Error::UnsupportedFrame(format!(
                    "VAAPI download produced pixel format {}; expected {download_format:?}",
                    downloaded.format
                )));
            }
            download.as_mut_ptr()
        } else {
            self.source_frame.as_mut_ptr()
        };
        let source = unsafe { &*source_ptr };
        if source.width <= 0 || source.height <= 0 {
            self.source_frame.unref();
            return Err(asciiflow_core::Error::UnsupportedFrame(format!(
                "decoder returned invalid frame dimensions {}x{}",
                source.width, source.height
            )));
        }
        if desc.format == PixelFormat::P010Le {
            let result = p010_from_native(source, &desc, pts);
            if let Some(frame) = &mut self.download_frame {
                frame.unref();
            }
            self.source_frame.unref();
            return result;
        }
        if self.scaler.is_none() {
            let source_format = if self.mode == DecodeMode::Vaapi {
                ffi::AVPixelFormat::AV_PIX_FMT_NV12
            } else {
                unsafe { (*self.codec.as_ptr()).pix_fmt }
            };
            if source_format == ffi::AVPixelFormat::AV_PIX_FMT_NONE {
                self.source_frame.unref();
                return Err(asciiflow_core::Error::UnsupportedFrame(
                    "decoder could not determine a pixel format for the damaged input".into(),
                ));
            }
            self.scaler = Some(create_scaler(
                source.width,
                source.height,
                source_format,
                self.info.frame_desc.width,
                self.info.frame_desc.height,
                self.source_matrix,
                self.source_range,
            )?);
        }
        let mut storage = HostFrame::new_zeroed(&desc);
        let width = desc.width as usize;
        let height = desc.height as usize;
        let (y, uv) = storage.planes_mut(&desc);
        let mut destination = [ptr::null_mut(); 4];
        destination[0] = y.as_mut_ptr();
        destination[1] = uv.as_mut_ptr();
        let strides = [width as i32, width as i32, 0, 0];
        let scaled = unsafe {
            ffi::sws_scale(
                self.scaler.expect("decoder scaler missing").as_ptr(),
                source.data.as_ptr() as *const *const u8,
                source.linesize.as_ptr(),
                0,
                source.height,
                destination.as_mut_ptr(),
                strides.as_ptr(),
            )
        };
        if let Some(frame) = &mut self.download_frame {
            frame.unref();
        }
        self.source_frame.unref();
        if scaled != height as i32 {
            return Err(asciiflow_core::Error::Media(format!(
                "NV12 conversion produced {scaled} rows; expected {height}"
            )));
        }
        VideoFrame::new_host(desc, pts, storage)
    }

    pub fn next_vaapi_frame(&mut self) -> Result<Option<VaapiDecodedFrame>> {
        if self.mode != DecodeMode::Vaapi {
            return Err(asciiflow_core::Error::Media(
                "hardware-frame output requires a VAAPI decoder".into(),
            ));
        }
        let Some(pts) = self.next_native_frame()? else {
            return Ok(None);
        };
        let frame = self.source_frame.try_clone()?;
        self.source_frame.unref();
        Ok(Some(VaapiDecodedFrame {
            frame,
            desc: self.info.frame_desc.clone(),
            pts,
        }))
    }
}

fn input_requirements(
    parameters: *const ffi::AVCodecParameters,
    width: u32,
    height: u32,
    frame_rate: Rational,
    color_space: ColorSpace,
) -> InputRequirements {
    let codec_id = unsafe { (*parameters).codec_id };
    let codec = if codec_id == ffi::AVCodecID::AV_CODEC_ID_H264 {
        VideoCodec::H264
    } else if codec_id == ffi::AVCodecID::AV_CODEC_ID_HEVC {
        VideoCodec::Hevc
    } else if codec_id == ffi::AVCodecID::AV_CODEC_ID_AV1 {
        VideoCodec::Av1
    } else {
        let name = unsafe { CStr::from_ptr(ffi::avcodec_get_name(codec_id)) }
            .to_string_lossy()
            .into_owned();
        VideoCodec::Other(name)
    };
    let profile = unsafe {
        let name = ffi::avcodec_profile_name(codec_id, (*parameters).profile);
        (!name.is_null()).then(|| {
            let name = CStr::from_ptr(name).to_string_lossy();
            match (&codec, name.as_ref()) {
                (VideoCodec::Hevc, "Main") => VideoProfile::HevcMain,
                (VideoCodec::Hevc, "Main 10") => VideoProfile::HevcMain10,
                (VideoCodec::Av1, "Main") => VideoProfile::Av1Main,
                (VideoCodec::H264, "Main") => VideoProfile::H264Main,
                _ => VideoProfile::from(name.as_ref()),
            }
        })
    };
    let format_value = unsafe { (*parameters).format };
    let descriptor = if (-1..ffi::AVPixelFormat::AV_PIX_FMT_NB as i32).contains(&format_value) {
        let format: ffi::AVPixelFormat = unsafe { std::mem::transmute(format_value) };
        unsafe { ffi::av_pix_fmt_desc_get(format).as_ref() }
    } else {
        None
    };
    let pixel_format = descriptor.map(|value| {
        unsafe { CStr::from_ptr(value.name) }
            .to_string_lossy()
            .into_owned()
    });
    let bit_depth = descriptor
        .map(|value| value.comp[0].depth as u8)
        .or_else(|| {
            let bits = unsafe { (*parameters).bits_per_raw_sample };
            (bits > 0).then_some(bits as u8)
        });
    let chroma_subsampling = descriptor.map_or(ChromaSubsampling::Unknown, |value| {
        if value.log2_chroma_w == 1 && value.log2_chroma_h == 1 {
            ChromaSubsampling::Yuv420
        } else {
            ChromaSubsampling::Other
        }
    });
    InputRequirements {
        codec,
        profile,
        pixel_format,
        bit_depth,
        chroma_subsampling,
        width,
        height,
        frame_rate,
        color_space,
    }
}

fn has_hdr_stream_side_data(parameters: *const ffi::AVCodecParameters) -> Result<bool> {
    let count = unsafe { (*parameters).nb_coded_side_data };
    if !(0..=1024).contains(&count) {
        return Err(asciiflow_core::Error::Media(
            "FFmpeg reported an invalid stream side-data count".into(),
        ));
    }
    if count == 0 {
        return Ok(false);
    }
    let pointer = unsafe { (*parameters).coded_side_data };
    if pointer.is_null() {
        return Err(asciiflow_core::Error::Media(
            "FFmpeg reported stream side data without storage".into(),
        ));
    }
    let side_data = unsafe { std::slice::from_raw_parts(pointer, count as usize) };
    Ok(side_data.iter().any(|entry| {
        matches!(
            entry.type_,
            ffi::AVPacketSideDataType::AV_PKT_DATA_MASTERING_DISPLAY_METADATA
                | ffi::AVPacketSideDataType::AV_PKT_DATA_CONTENT_LIGHT_LEVEL
        )
    }))
}

fn p010_from_native(
    source: &ffi::AVFrame,
    desc: &FrameDesc,
    pts: Option<i64>,
) -> Result<VideoFrame> {
    let width = desc.width as usize;
    let height = desc.height as usize;
    if source.width != desc.width as i32 || source.height != desc.height as i32 {
        return Err(asciiflow_core::Error::UnsupportedFrame(format!(
            "10-bit frame geometry changed: {}x{}; expected {}x{}",
            source.width, source.height, desc.width, desc.height
        )));
    }
    let mut storage = HostFrame::try_new_zeroed(desc)?;
    let (y, uv) = storage.planes_mut(desc);
    match source.format {
        value if value == ffi::AVPixelFormat::AV_PIX_FMT_P010LE as i32 => {
            for row in 0..height {
                let input = native_row(source, 0, row, width * 2)?;
                validate_p010_words(input)?;
                y[row * width * 2..(row + 1) * width * 2].copy_from_slice(input);
            }
            for row in 0..height / 2 {
                let input = native_row(source, 1, row, width * 2)?;
                validate_p010_words(input)?;
                uv[row * width * 2..(row + 1) * width * 2].copy_from_slice(input);
            }
        }
        value if value == ffi::AVPixelFormat::AV_PIX_FMT_YUV420P10LE as i32 => {
            for row in 0..height {
                let input = native_row(source, 0, row, width * 2)?;
                let output = &mut y[row * width * 2..(row + 1) * width * 2];
                repack_planar_row(input, output)?;
            }
            for row in 0..height / 2 {
                let u = native_row(source, 1, row, width)?;
                let v = native_row(source, 2, row, width)?;
                let output = &mut uv[row * width * 2..(row + 1) * width * 2];
                for sample in 0..width / 2 {
                    let u_code = planar_code(&u[sample * 2..sample * 2 + 2])?;
                    let v_code = planar_code(&v[sample * 2..sample * 2 + 2])?;
                    output[sample * 4..sample * 4 + 2]
                        .copy_from_slice(&(u_code << 6).to_le_bytes());
                    output[sample * 4 + 2..sample * 4 + 4]
                        .copy_from_slice(&(v_code << 6).to_le_bytes());
                }
            }
        }
        other => {
            return Err(asciiflow_core::Error::UnsupportedFrame(format!(
                "10-bit decoder returned pixel format {other}; expected P010LE or YUV420P10LE"
            )));
        }
    }
    VideoFrame::new_host(desc.clone(), pts, storage)
}

fn native_row(source: &ffi::AVFrame, plane: usize, row: usize, row_bytes: usize) -> Result<&[u8]> {
    let data = source.data[plane];
    let stride = source.linesize[plane] as isize;
    if data.is_null() || stride.unsigned_abs() < row_bytes {
        return Err(asciiflow_core::Error::UnsupportedFrame(format!(
            "10-bit plane {plane} has null data or insufficient stride"
        )));
    }
    let offset = isize::try_from(row)
        .ok()
        .and_then(|value| value.checked_mul(stride))
        .ok_or_else(|| {
            asciiflow_core::Error::UnsupportedFrame("10-bit row offset overflows isize".into())
        })?;
    // FFmpeg owns this plane and its signed stride. The caller keeps the AVFrame
    // referenced while using this row; width/stride are checked above.
    Ok(unsafe { std::slice::from_raw_parts(data.offset(offset), row_bytes) })
}

fn planar_code(bytes: &[u8]) -> Result<u16> {
    let word = u16::from_le_bytes([bytes[0], bytes[1]]);
    if word & !0x03ff != 0 {
        return Err(asciiflow_core::Error::UnsupportedFrame(
            "YUV420P10LE sample has non-zero high padding bits".into(),
        ));
    }
    Ok(word)
}

fn repack_planar_row(input: &[u8], output: &mut [u8]) -> Result<()> {
    for (source, destination) in input.chunks_exact(2).zip(output.chunks_exact_mut(2)) {
        destination.copy_from_slice(&(planar_code(source)? << 6).to_le_bytes());
    }
    Ok(())
}

fn validate_p010_words(input: &[u8]) -> Result<()> {
    if input.chunks_exact(2).any(|sample| sample[0] & 0x3f != 0) {
        return Err(asciiflow_core::Error::UnsupportedFrame(
            "P010LE sample has non-zero low padding bits".into(),
        ));
    }
    Ok(())
}

fn map_primaries(value: ffi::AVColorPrimaries) -> ColorPrimaries {
    match value {
        ffi::AVColorPrimaries::AVCOL_PRI_BT709 => ColorPrimaries::Bt709,
        ffi::AVColorPrimaries::AVCOL_PRI_BT2020 => ColorPrimaries::Bt2020,
        ffi::AVColorPrimaries::AVCOL_PRI_UNSPECIFIED => ColorPrimaries::Unspecified,
        _ => ColorPrimaries::Other,
    }
}

fn map_transfer(value: ffi::AVColorTransferCharacteristic) -> TransferCharacteristic {
    match value {
        ffi::AVColorTransferCharacteristic::AVCOL_TRC_BT709 => TransferCharacteristic::Bt709,
        ffi::AVColorTransferCharacteristic::AVCOL_TRC_SMPTE2084 => TransferCharacteristic::Pq,
        ffi::AVColorTransferCharacteristic::AVCOL_TRC_ARIB_STD_B67 => TransferCharacteristic::Hlg,
        ffi::AVColorTransferCharacteristic::AVCOL_TRC_UNSPECIFIED => {
            TransferCharacteristic::Unspecified
        }
        _ => TransferCharacteristic::Other,
    }
}

fn map_matrix(value: ffi::AVColorSpace) -> ColorMatrix {
    match value {
        ffi::AVColorSpace::AVCOL_SPC_BT709 => ColorMatrix::Bt709,
        ffi::AVColorSpace::AVCOL_SPC_BT470BG | ffi::AVColorSpace::AVCOL_SPC_SMPTE170M => {
            ColorMatrix::Bt601
        }
        ffi::AVColorSpace::AVCOL_SPC_BT2020_NCL | ffi::AVColorSpace::AVCOL_SPC_BT2020_CL => {
            ColorMatrix::Bt2020
        }
        ffi::AVColorSpace::AVCOL_SPC_UNSPECIFIED => ColorMatrix::Unspecified,
        _ => ColorMatrix::Other,
    }
}

fn map_range(value: ffi::AVColorRange) -> ColorRange {
    match value {
        ffi::AVColorRange::AVCOL_RANGE_JPEG => ColorRange::Full,
        ffi::AVColorRange::AVCOL_RANGE_MPEG => ColorRange::Limited,
        _ => ColorRange::Unspecified,
    }
}

fn merge_frame_color(stream: &mut ColorSpace, frame: ColorSpace) -> Result<()> {
    fn merge<T: Copy + PartialEq + std::fmt::Debug>(
        stream: &mut T,
        frame: T,
        unspecified: T,
        name: &str,
    ) -> Result<()> {
        if frame != unspecified {
            if *stream != unspecified && *stream != frame {
                return Err(asciiflow_core::Error::UnsupportedFrame(format!(
                    "stream/frame {name} metadata disagree: {:?} vs {:?}",
                    stream, frame
                )));
            }
            *stream = frame;
        }
        Ok(())
    }
    merge(
        &mut stream.matrix,
        frame.matrix,
        ColorMatrix::Unspecified,
        "matrix",
    )?;
    merge(
        &mut stream.range,
        frame.range,
        ColorRange::Unspecified,
        "range",
    )?;
    merge(
        &mut stream.primaries,
        frame.primaries,
        ColorPrimaries::Unspecified,
        "primaries",
    )?;
    merge(
        &mut stream.transfer,
        frame.transfer,
        TransferCharacteristic::Unspecified,
        "transfer",
    )?;
    merge(
        &mut stream.chroma_location,
        frame.chroma_location,
        ChromaLocation::Unspecified,
        "chroma location",
    )
}

fn map_chroma_location(value: ffi::AVChromaLocation) -> ChromaLocation {
    match value {
        ffi::AVChromaLocation::AVCHROMA_LOC_LEFT => ChromaLocation::Left,
        ffi::AVChromaLocation::AVCHROMA_LOC_CENTER => ChromaLocation::Center,
        _ => ChromaLocation::Unspecified,
    }
}

impl FrameSource for Decoder {
    fn finish(&mut self) -> Result<()> {
        if self.audio_sender.is_none() {
            return Ok(());
        }
        // A video frame limit does not trim the independent audio timeline.
        self.packet.unref();
        while !self.input_eof {
            if let Some(sender) = &self.audio_sender {
                sender.check_active()?;
            }
            let result =
                unsafe { ffi::av_read_frame(self.format.as_ptr(), self.packet.as_mut_ptr()) };
            if result == ffi::AVERROR_EOF {
                self.input_eof = true;
                break;
            }
            check(result, "read remaining passthrough audio")?;
            let index = unsafe { (*self.packet.as_mut_ptr()).stream_index };
            self.route_audio_packet(index)?;
        }
        if let Some(sender) = self.audio_sender.take() {
            sender.finish()?;
        }
        Ok(())
    }
    fn next_frame(&mut self) -> Result<Option<VideoFrame>> {
        let Some(pts) = self.next_native_frame()? else {
            return Ok(None);
        };
        self.convert_current_to_host(pts).map(Some)
    }

    fn take_timings(&mut self) -> SourceTimings {
        std::mem::take(&mut self.timings)
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        unsafe {
            if let Some(scaler) = self.scaler {
                ffi::sws_freeContext(scaler.as_ptr());
            }
            let mut codec = self.codec.as_ptr();
            ffi::avcodec_free_context(&mut codec);
            let mut format = self.format.as_ptr();
            ffi::avformat_close_input(&mut format);
        }
    }
}
unsafe impl Send for Decoder {}

enum ReceiveResult {
    Frame(Option<i64>),
    Again,
    Eof,
}

struct FormatGuard(Option<NonNull<ffi::AVFormatContext>>);
impl FormatGuard {
    fn take(&mut self) -> NonNull<ffi::AVFormatContext> {
        self.0.take().expect("format guard already empty")
    }
}
impl Drop for FormatGuard {
    fn drop(&mut self) {
        if let Some(pointer) = self.0 {
            let mut p = pointer.as_ptr();
            unsafe { ffi::avformat_close_input(&mut p) }
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
struct ScalerGuard(Option<NonNull<ffi::SwsContext>>);
impl ScalerGuard {
    fn take_optional(&mut self) -> Option<NonNull<ffi::SwsContext>> {
        self.0.take()
    }
}

fn create_scaler(
    source_width: i32,
    source_height: i32,
    source_format: ffi::AVPixelFormat,
    width: u32,
    height: u32,
    source_matrix: ColorMatrix,
    source_range: ColorRange,
) -> Result<NonNull<ffi::SwsContext>> {
    let scaler = NonNull::new(unsafe {
        ffi::sws_getContext(
            source_width,
            source_height,
            source_format,
            width as i32,
            height as i32,
            ffi::AVPixelFormat::AV_PIX_FMT_NV12,
            ffi::SwsFlags::SWS_BILINEAR as i32,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null(),
        )
    })
    .ok_or_else(|| {
        asciiflow_core::Error::Media("failed to create source-to-NV12 converter".into())
    })?;
    let source_coefficients = unsafe {
        ffi::sws_getCoefficients(match source_matrix {
            ColorMatrix::Bt709 => ffi::SWS_CS_ITU709,
            _ => ffi::SWS_CS_ITU601,
        })
    };
    let target_coefficients = unsafe { ffi::sws_getCoefficients(ffi::SWS_CS_ITU709) };
    if let Err(error) = check(
        unsafe {
            ffi::sws_setColorspaceDetails(
                scaler.as_ptr(),
                source_coefficients,
                (source_range == ColorRange::Full) as i32,
                target_coefficients,
                0,
                0,
                1 << 16,
                1 << 16,
            )
        },
        "failed to configure BT.709 limited-range NV12 conversion",
    ) {
        unsafe { ffi::sws_freeContext(scaler.as_ptr()) };
        return Err(error);
    }
    Ok(scaler)
}

fn require_vaapi_decoder(decoder: *const ffi::AVCodec) -> Result<()> {
    let mut index = 0;
    loop {
        let config = unsafe { ffi::avcodec_get_hw_config(decoder, index) };
        if config.is_null() {
            return Err(asciiflow_core::Error::Media(
                "input codec has no VAAPI hardware decoder configuration".into(),
            ));
        }
        let config = unsafe { &*config };
        if config.device_type == ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI
            && config.pix_fmt == ffi::AVPixelFormat::AV_PIX_FMT_VAAPI
            && config.methods & ffi::AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX as i32 != 0
        {
            return Ok(());
        }
        index += 1;
    }
}

unsafe extern "C" fn select_vaapi_format(
    _context: *mut ffi::AVCodecContext,
    formats: *const ffi::AVPixelFormat,
) -> ffi::AVPixelFormat {
    if formats.is_null() {
        return ffi::AVPixelFormat::AV_PIX_FMT_NONE;
    }
    let mut cursor = formats;
    while unsafe { *cursor } != ffi::AVPixelFormat::AV_PIX_FMT_NONE {
        if unsafe { *cursor } == ffi::AVPixelFormat::AV_PIX_FMT_VAAPI {
            return ffi::AVPixelFormat::AV_PIX_FMT_VAAPI;
        }
        cursor = unsafe { cursor.add(1) };
    }
    ffi::AVPixelFormat::AV_PIX_FMT_NONE
}
impl Drop for ScalerGuard {
    fn drop(&mut self) {
        if let Some(pointer) = self.0 {
            unsafe { ffi::sws_freeContext(pointer.as_ptr()) }
        }
    }
}

#[cfg(test)]
mod p010_tests {
    use super::*;

    #[test]
    fn planar_repack_respects_padding_stride_and_preserves_all_channels() {
        let desc = FrameDesc::host_p010_le(2, 2, ColorSpace::default()).unwrap();
        let mut y = [0u8; 16];
        for (offset, code) in [(0, 1u16), (2, 513), (8, 1023), (10, 2)] {
            y[offset..offset + 2].copy_from_slice(&code.to_le_bytes());
        }
        let mut u = [0u8; 4];
        let mut v = [0u8; 4];
        u[..2].copy_from_slice(&514u16.to_le_bytes());
        v[..2].copy_from_slice(&515u16.to_le_bytes());
        let mut frame = Frame::new().unwrap();
        let native = unsafe { &mut *frame.as_mut_ptr() };
        native.format = ffi::AVPixelFormat::AV_PIX_FMT_YUV420P10LE as i32;
        native.width = 2;
        native.height = 2;
        native.data[0] = y.as_mut_ptr();
        native.data[1] = u.as_mut_ptr();
        native.data[2] = v.as_mut_ptr();
        native.linesize[0] = 8;
        native.linesize[1] = 4;
        native.linesize[2] = 4;
        let output = p010_from_native(native, &desc, Some(37)).unwrap();
        assert_eq!(output.pts(), Some(37));
        let codes: Vec<u16> = output
            .host()
            .as_slice()
            .chunks_exact(2)
            .map(|word| u16::from_le_bytes([word[0], word[1]]) >> 6)
            .collect();
        assert_eq!(codes, [1, 513, 1023, 2, 514, 515]);
        assert!(
            output
                .host()
                .as_slice()
                .chunks_exact(2)
                .all(|word| word[0] & 0x3f == 0)
        );
    }

    #[test]
    fn invalid_planar_and_p010_padding_are_not_silently_masked() {
        assert!(planar_code(&0x0400u16.to_le_bytes()).is_err());
        assert!(validate_p010_words(&0x0041u16.to_le_bytes()).is_err());
        assert!(validate_p010_words(&0xffc0u16.to_le_bytes()).is_ok());
    }
}
