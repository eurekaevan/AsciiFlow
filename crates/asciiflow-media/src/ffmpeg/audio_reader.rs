//! Independent demux progress prevents audio/video head waits from blocking
//! the video decoder behind its own passthrough audio packets.
use super::{audio::AudioPacketSender, codec::check, ffi, packet::Packet};
use asciiflow_core::{AudioStreamInfo, CancellationToken, Error, PipelineStage, Result};
use std::{ffi::CString, ptr, thread::JoinHandle};

pub(crate) struct AudioReader {
    stop: CancellationToken,
    thread: Option<JoinHandle<Result<()>>>,
}

impl AudioReader {
    pub(crate) fn start(
        path: CString,
        streams: Vec<AudioStreamInfo>,
        mut sender: AudioPacketSender,
    ) -> Result<Self> {
        let stop = CancellationToken::new();
        sender.stop = Some(stop.clone());
        let thread = std::thread::Builder::new()
            .name("asciiflow-audio-demux".into())
            .spawn(move || {
                // Publish before dropping the sender: a disconnected head wait must
                // see the originating error rather than inventing a mux failure.
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    read(&path, &streams, &sender)
                }))
                .unwrap_or_else(|_| {
                    Err(Error::pipeline_message(
                        PipelineStage::DecodeRuntime,
                        "read passthrough audio",
                        "audio demux worker panicked",
                    ))
                });
                let result = result.map_err(|error| {
                    if error.is_cancelled() {
                        error
                    } else {
                        Error::pipeline(
                            PipelineStage::DecodeRuntime,
                            "read passthrough audio",
                            error,
                        )
                    }
                });
                if let Err(error) = &result {
                    sender.publish_failure(error);
                }
                result
            })
            .map_err(|error| {
                Error::pipeline_message(
                    PipelineStage::MuxInitialization,
                    "start audio demux worker",
                    error.to_string(),
                )
            })?;
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }

    pub(crate) fn finish(&mut self) -> Result<()> {
        match self.thread.take() {
            Some(thread) => thread.join().map_err(|_| {
                Error::pipeline_message(
                    PipelineStage::DecodeRuntime,
                    "join audio demux worker",
                    "audio demux worker panicked",
                )
            })?,
            None => Ok(()),
        }
    }
}

impl Drop for AudioReader {
    fn drop(&mut self) {
        self.stop.cancel();
        let _ = self.finish();
    }
}

unsafe extern "C" fn interrupted(opaque: *mut std::ffi::c_void) -> i32 {
    // The borrowed sender outlives Input, including avformat_close_input.
    let sender = unsafe { &*opaque.cast::<AudioPacketSender>() };
    i32::from(sender.check_active().is_err())
}

struct Input(*mut ffi::AVFormatContext);
impl Drop for Input {
    fn drop(&mut self) {
        unsafe { ffi::avformat_close_input(&mut self.0) };
    }
}

fn read(path: &CString, streams: &[AudioStreamInfo], sender: &AudioPacketSender) -> Result<()> {
    let mut input = Input(unsafe { ffi::avformat_alloc_context() });
    if input.0.is_null() {
        return Err(Error::Media("allocate audio input context".into()));
    }
    unsafe {
        (*input.0).interrupt_callback = ffi::AVIOInterruptCB {
            callback: Some(interrupted),
            opaque: (sender as *const AudioPacketSender).cast_mut().cast(),
        };
    }
    let open = unsafe {
        ffi::avformat_open_input(&mut input.0, path.as_ptr(), ptr::null(), ptr::null_mut())
    };
    sender.check_active()?;
    check(open, "reopen passthrough audio input")?;
    let probe = unsafe { ffi::avformat_find_stream_info(input.0, ptr::null_mut()) };
    sender.check_active()?;
    check(probe, "probe passthrough audio input")?;
    for selected in streams {
        if selected.input_index >= unsafe { (*input.0).nb_streams as usize } {
            return Err(Error::Media(
                "selected audio stream disappeared while reopening input".into(),
            ));
        }
        let stream = unsafe { &**(*input.0).streams.add(selected.input_index) };
        let parameters = unsafe { stream.codecpar.as_ref() }
            .ok_or_else(|| Error::Media("missing reopened audio parameters".into()))?;
        if parameters.codec_type != ffi::AVMediaType::AVMEDIA_TYPE_AUDIO
            || parameters.codec_id as i32 != selected.codec_id
            || stream.time_base.num != selected.time_base.numerator
            || stream.time_base.den != selected.time_base.denominator
        {
            return Err(Error::Media(
                "selected audio descriptor changed while reopening input".into(),
            ));
        }
    }
    loop {
        sender.check_active()?;
        let mut packet = Packet::new()?;
        let result = unsafe { ffi::av_read_frame(input.0, packet.as_mut_ptr()) };
        sender.check_active()?;
        if result == ffi::AVERROR_EOF {
            return sender.finish();
        }
        check(result, "read passthrough audio packet").map_err(|error| {
            Error::pipeline(
                PipelineStage::DecodeRuntime,
                "read passthrough audio",
                error,
            )
        })?;
        let index = unsafe { (*packet.as_mut_ptr()).stream_index };
        if let Some(stream) = streams
            .iter()
            .find(|stream| stream.input_index as i32 == index)
        {
            sender.send(
                packet,
                stream.input_index,
                ffi::AVRational {
                    num: stream.time_base.numerator,
                    den: stream.time_base.denominator,
                },
            )?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn reopen_failure_publishes_decode_root_before_disconnect() {
        let (tx, _rx) = crossbeam_channel::bounded(1);
        let failure = Arc::new(Mutex::new(None));
        let sender = AudioPacketSender::new(tx, CancellationToken::new(), failure.clone());
        let mut reader = AudioReader::start(
            CString::new("/nonexistent/asciiflow-audio-input.mp4").unwrap(),
            Vec::new(),
            sender,
        )
        .unwrap();
        let error = reader.finish().unwrap_err();
        assert_eq!(error.stage(), Some(PipelineStage::DecodeRuntime));
        assert!(error.to_string().contains("reopen passthrough audio input"));
        assert_eq!(
            failure
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .error()
                .to_string(),
            error.to_string()
        );
    }

    #[test]
    fn drop_interrupts_a_reader_blocked_on_bounded_audio_enqueue() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/media/single.mp4");
        let decoder = super::super::decoder::Decoder::open(&path).unwrap();
        let streams = decoder.info().audio_streams.clone();
        let (tx, rx) = crossbeam_channel::bounded(1);
        let failure = Arc::new(Mutex::new(None));
        let sender = AudioPacketSender::new(tx, CancellationToken::new(), failure.clone());
        let reader = AudioReader::start(
            CString::new(path.as_os_str().as_encoded_bytes()).unwrap(),
            streams,
            sender,
        )
        .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !rx.is_full() && std::time::Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert!(rx.is_full());
        drop(reader);
        assert!(std::time::Instant::now() < deadline);
        assert!(failure.lock().unwrap().is_none());
    }
}
