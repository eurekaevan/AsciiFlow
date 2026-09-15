use super::{codec::check, ffi, packet::Packet};
use asciiflow_core::{
    AudioPlan, AudioStreamInfo, CancellationToken, Error, PipelineStage, Rational, Result,
};
use crossbeam_channel::{SendTimeoutError, Sender};
use std::{
    ffi::{CStr, CString},
    ptr::{self, NonNull},
    sync::{Arc, Mutex},
    time::Duration,
};

const SEND_POLL: Duration = Duration::from_millis(20);

pub(crate) struct AudioInputStream {
    pub info: AudioStreamInfo,
    parameters: CodecParameters,
    pub time_base: ffi::AVRational,
    pub disposition: i32,
}

impl AudioInputStream {
    pub(crate) fn output_template(&self) -> Result<AudioOutputTemplate> {
        Ok(AudioOutputTemplate {
            input_index: self.info.input_index,
            parameters: self.parameters.try_clone()?,
            input_time_base: self.time_base,
            disposition: self.disposition,
            language: self.info.language.clone(),
        })
    }
}

pub struct AudioOutputTemplate {
    pub(crate) input_index: usize,
    pub(crate) parameters: CodecParameters,
    pub(crate) input_time_base: ffi::AVRational,
    pub(crate) disposition: i32,
    pub(crate) language: Option<String>,
}

pub(crate) struct CodecParameters(NonNull<ffi::AVCodecParameters>);

impl CodecParameters {
    fn copy_from(source: *const ffi::AVCodecParameters) -> Result<Self> {
        let pointer = NonNull::new(unsafe { ffi::avcodec_parameters_alloc() })
            .ok_or_else(|| Error::Media("failed to allocate audio codec parameters".into()))?;
        if let Err(error) = check(
            unsafe { ffi::avcodec_parameters_copy(pointer.as_ptr(), source) },
            "failed to copy audio codec parameters",
        ) {
            let mut raw = pointer.as_ptr();
            unsafe { ffi::avcodec_parameters_free(&mut raw) };
            return Err(error);
        }
        Ok(Self(pointer))
    }

    fn try_clone(&self) -> Result<Self> {
        Self::copy_from(self.0.as_ptr())
    }

    pub(crate) fn as_ptr(&self) -> *const ffi::AVCodecParameters {
        self.0.as_ptr()
    }
}

impl Drop for CodecParameters {
    fn drop(&mut self) {
        let mut pointer = self.0.as_ptr();
        unsafe { ffi::avcodec_parameters_free(&mut pointer) };
    }
}

unsafe impl Send for CodecParameters {}

pub(crate) fn discover_audio_streams(
    format: NonNull<ffi::AVFormatContext>,
) -> Result<Vec<AudioInputStream>> {
    let mp4 = CString::new("mp4").expect("static MP4 name contains no NUL");
    let output_format = unsafe { ffi::av_guess_format(mp4.as_ptr(), ptr::null(), ptr::null()) };
    if output_format.is_null() {
        return Err(Error::Media(
            "FFmpeg exposes no MP4 muxer for audio compatibility probing".into(),
        ));
    }

    let stream_count = unsafe { (*format.as_ptr()).nb_streams as usize };
    let mut result = Vec::new();
    for input_index in 0..stream_count {
        let stream = unsafe { *(*format.as_ptr()).streams.add(input_index) };
        let parameters = unsafe { (*stream).codecpar };
        if parameters.is_null()
            || unsafe { (*parameters).codec_type } != ffi::AVMediaType::AVMEDIA_TYPE_AUDIO
        {
            continue;
        }
        let codec_id = unsafe { (*parameters).codec_id };
        let codec = unsafe { CStr::from_ptr(ffi::avcodec_get_name(codec_id)) }
            .to_string_lossy()
            .into_owned();
        let profile = unsafe {
            let value = ffi::avcodec_profile_name(codec_id, (*parameters).profile);
            (!value.is_null()).then(|| CStr::from_ptr(value).to_string_lossy().into_owned())
        };
        let compatibility = unsafe {
            ffi::avformat_query_codec(output_format, codec_id, ffi::FF_COMPLIANCE_NORMAL)
        };
        let mp4_compatible = compatibility > 0;
        let language = dictionary_value(unsafe { (*stream).metadata }, "language");
        let time_base = unsafe { (*stream).time_base };
        let sample_rate =
            unsafe { ((*parameters).sample_rate > 0).then_some((*parameters).sample_rate as u32) };
        let channels = unsafe {
            ((*parameters).ch_layout.nb_channels > 0)
                .then_some((*parameters).ch_layout.nb_channels as u32)
        };
        result.push(AudioInputStream {
            info: AudioStreamInfo {
                input_index,
                codec_id: codec_id as i32,
                codec: codec.clone(),
                profile,
                time_base: Rational::new(time_base.num, time_base.den).map_err(|error| {
                    Error::pipeline(
                        PipelineStage::InputProbe,
                        "validate audio stream time base",
                        error,
                    )
                })?,
                sample_rate,
                channels,
                bit_rate: unsafe {
                    ((*parameters).bit_rate > 0).then_some((*parameters).bit_rate as u64)
                },
                language,
                start_time: unsafe {
                    ((*stream).start_time != ffi::AV_NOPTS_VALUE).then_some((*stream).start_time)
                },
                default: unsafe { (*stream).disposition & ffi::AV_DISPOSITION_DEFAULT != 0 },
                forced: unsafe { (*stream).disposition & ffi::AV_DISPOSITION_FORCED != 0 },
                mp4_compatible,
                compatibility_reason: (!mp4_compatible).then(|| {
                    format!("FFmpeg MP4 muxer compatibility query returned {compatibility}")
                }),
            },
            parameters: CodecParameters::copy_from(parameters)?,
            time_base,
            disposition: unsafe { (*stream).disposition },
        });
    }
    Ok(result)
}

fn dictionary_value(dictionary: *mut ffi::AVDictionary, key: &str) -> Option<String> {
    let key = CString::new(key).expect("static metadata key contains no NUL");
    let entry = unsafe { ffi::av_dict_get(dictionary, key.as_ptr(), ptr::null(), 0) };
    if entry.is_null() || unsafe { (*entry).value }.is_null() {
        None
    } else {
        Some(
            unsafe { CStr::from_ptr((*entry).value) }
                .to_string_lossy()
                .into_owned(),
        )
    }
}

pub(crate) enum MuxMessage {
    #[cfg(test)]
    FailAudioAfter(u64),
    Packet {
        packet: Packet,
        input_index: usize,
        input_time_base: ffi::AVRational,
        audio: bool,
    },
    Finish,
    AudioDone,
    VideoOrigin {
        pts: i64,
        time_base: ffi::AVRational,
    },
    Abort,
}

#[derive(Clone)]
pub struct AudioPacketSender {
    sender: Sender<MuxMessage>,
    cancellation: CancellationToken,
    failure: Arc<Mutex<Option<String>>>,
}

impl AudioPacketSender {
    pub(crate) fn check_active(&self) -> Result<()> {
        if self
            .failure
            .lock()
            .expect("mux failure lock poisoned")
            .is_some()
        {
            return Err(self.mux_failure());
        }
        if self.cancellation.is_cancelled() {
            return Err(Error::Cancelled);
        }
        Ok(())
    }
    pub(crate) fn finish(&self) -> Result<()> {
        self.send_message(MuxMessage::AudioDone)
    }

    pub(crate) fn video_origin(&self, pts: i64, time_base: ffi::AVRational) -> Result<()> {
        self.send_message(MuxMessage::VideoOrigin { pts, time_base })
    }
    pub(crate) fn new(
        sender: Sender<MuxMessage>,
        cancellation: CancellationToken,
        failure: Arc<Mutex<Option<String>>>,
    ) -> Self {
        Self {
            sender,
            cancellation,
            failure,
        }
    }

    pub(crate) fn send(
        &self,
        mut packet: Packet,
        input_index: usize,
        input_time_base: ffi::AVRational,
    ) -> Result<()> {
        if packet.size() > 16 * 1024 * 1024 {
            return Err(Error::pipeline_message(
                PipelineStage::MuxRuntime,
                "validate audio packet size",
                "compressed packet exceeds the 16 MiB mux limit",
            ));
        }
        let native = unsafe { &*packet.as_mut_ptr() };
        if native.pts == ffi::AV_NOPTS_VALUE || native.dts == ffi::AV_NOPTS_VALUE {
            return Err(Error::pipeline_message(
                PipelineStage::MuxRuntime,
                "validate passthrough audio timestamps",
                format!("audio stream #{input_index} packet has missing PTS or DTS"),
            ));
        }
        self.send_message(MuxMessage::Packet {
            packet,
            input_index,
            input_time_base,
            audio: true,
        })
    }

    fn send_message(&self, mut message: MuxMessage) -> Result<()> {
        loop {
            self.check_active()?;
            match self.sender.send_timeout(message, SEND_POLL) {
                Ok(()) => return Ok(()),
                Err(SendTimeoutError::Timeout(pending)) => message = pending,
                Err(SendTimeoutError::Disconnected(_)) => return Err(self.mux_failure()),
            }
        }
    }

    fn mux_failure(&self) -> Error {
        let message = self
            .failure
            .lock()
            .expect("mux failure lock poisoned")
            .clone()
            .unwrap_or_else(|| "mux worker stopped before accepting the packet".into());
        Error::pipeline_message(
            PipelineStage::MuxRuntime,
            "write interleaved packet",
            message,
        )
    }
}

pub(crate) fn selected_templates(
    streams: &[AudioInputStream],
    plan: &AudioPlan,
) -> Result<Vec<AudioOutputTemplate>> {
    plan.selected
        .iter()
        .map(|selected| {
            streams
                .iter()
                .find(|stream| stream.info.input_index == selected.input_index)
                .ok_or_else(|| {
                    Error::Media(format!(
                        "selected audio stream #{} disappeared while reopening the input",
                        selected.input_index
                    ))
                })?
                .output_template()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::bounded;

    fn packet(pts: i64, dts: i64) -> Packet {
        let mut packet = Packet::new().unwrap();
        unsafe {
            (*packet.as_mut_ptr()).pts = pts;
            (*packet.as_mut_ptr()).dts = dts;
        }
        packet
    }

    #[test]
    fn missing_audio_timestamps_fail_without_guessing_or_enqueueing() {
        let (tx, rx) = bounded(1);
        let sender =
            AudioPacketSender::new(tx, CancellationToken::new(), Arc::new(Mutex::new(None)));
        for (pts, dts) in [(ffi::AV_NOPTS_VALUE, 0), (0, ffi::AV_NOPTS_VALUE)] {
            let error = sender
                .send(packet(pts, dts), 7, ffi::AVRational { num: 1, den: 48000 })
                .unwrap_err();
            assert_eq!(error.stage(), Some(PipelineStage::MuxRuntime));
            assert!(error.to_string().contains("stream #7"));
            assert!(rx.is_empty());
        }
    }

    #[test]
    fn negative_audio_timestamps_are_not_clamped() {
        let (tx, rx) = bounded(1);
        let sender =
            AudioPacketSender::new(tx, CancellationToken::new(), Arc::new(Mutex::new(None)));
        sender
            .send(
                packet(-1024, -1024),
                1,
                ffi::AVRational { num: 1, den: 48000 },
            )
            .unwrap();
        let MuxMessage::Packet { mut packet, .. } = rx.recv().unwrap() else {
            panic!("expected packet")
        };
        unsafe {
            assert_eq!((*packet.as_mut_ptr()).pts, -1024);
            assert_eq!((*packet.as_mut_ptr()).dts, -1024);
        }
    }

    #[test]
    fn audio_queue_backpressure_is_bounded_and_cancellable() {
        let (tx, rx) = bounded(2);
        let observe = tx.clone();
        let cancellation = CancellationToken::new();
        let sender = AudioPacketSender::new(tx, cancellation.clone(), Arc::new(Mutex::new(None)));
        let worker = std::thread::spawn(move || {
            for index in 0..10000 {
                sender.send(
                    packet(index, index),
                    1,
                    ffi::AVRational { num: 1, den: 48000 },
                )?;
            }
            Ok::<_, Error>(())
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while observe.len() < 2 && std::time::Instant::now() < deadline {
            std::thread::yield_now();
        }
        let high_water = observe.len();
        cancellation.cancel();
        assert!(worker.join().unwrap().unwrap_err().is_cancelled());
        assert_eq!(high_water, observe.capacity().unwrap());
        assert_eq!(rx.len(), 2);
    }

    #[test]
    fn audio_rescale_preserves_media_time_within_destination_tick() {
        let mut packet = packet(1234567, 1234567);
        let source = ffi::AVRational { num: 1, den: 90000 };
        let destination = ffi::AVRational { num: 1, den: 48000 };
        unsafe {
            ffi::av_packet_rescale_ts(packet.as_mut_ptr(), source, destination);
            let scaled = (*packet.as_mut_ptr()).pts;
            assert!((scaled as i128 * 90000 - 1234567_i128 * 48000).abs() <= 90000);
        }
    }
}
