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
                    #[cfg(feature = "mux-qualification")]
                    if let Some(receipt) = std::env::var_os("ASCIIFLOW_D1B_REPLAY_AAC") {
                        return replay_captured_aac(&path, &streams, &sender, &receipt);
                    }
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

// Qualification-only control: retain an immutable capture before the measured
// interval, then use the unchanged sender, channels and mux ownership path.
#[cfg(feature = "mux-qualification")]
fn replay_captured_aac(
    path: &CString,
    streams: &[AudioStreamInfo],
    sender: &AudioPacketSender,
    receipt: &std::ffi::OsStr,
) -> Result<()> {
    let mut input = Input(ptr::null_mut());
    check(
        unsafe {
            ffi::avformat_open_input(&mut input.0, path.as_ptr(), ptr::null(), ptr::null_mut())
        },
        "open AAC capture input",
    )?;
    check(
        unsafe { ffi::avformat_find_stream_info(input.0, ptr::null_mut()) },
        "probe AAC capture input",
    )?;
    let mut packets = Vec::new();
    loop {
        sender.check_active()?;
        let mut packet = Packet::new()?;
        let result = unsafe { ffi::av_read_frame(input.0, packet.as_mut_ptr()) };
        if result == ffi::AVERROR_EOF {
            break;
        }
        check(result, "capture original AAC packet")?;
        let index = unsafe { (*packet.as_mut_ptr()).stream_index };
        if let Some(stream) = streams
            .iter()
            .find(|stream| stream.input_index as i32 == index)
        {
            packets.push((
                stream.input_index,
                ffi::AVRational {
                    num: stream.time_base.numerator,
                    den: stream.time_base.denominator,
                },
                packet,
            ));
        }
    }
    drop(input);
    std::fs::write(
        receipt,
        format!(
            "{{\"captured_packets\":{},\"immutable_capture\":true,\"native_input_closed\":true}}\n",
            packets.len()
        ),
    )
    .map_err(|e| Error::Media(e.to_string()))?;
    for (index, time_base, original) in &mut packets {
        let mut packet = Packet::new()?;
        check(
            unsafe { ffi::av_packet_ref(packet.as_mut_ptr(), original.as_mut_ptr()) },
            "reference captured AAC packet",
        )?;
        sender.send(packet, *index, *time_base)?;
    }
    sender.finish()?;
    std::fs::write(
        std::path::Path::new(receipt).with_extension("complete"),
        b"replay producer complete; capture drop follows",
    )
    .map_err(|e| Error::Media(e.to_string()))?;
    Ok(())
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
            #[cfg(feature = "mux-qualification")]
            if packet_lifetime::enabled() {
                packet_lifetime::sample("before_close_input")?;
                drop(input);
                packet_lifetime::sample("after_close_input")?;
                return sender.finish();
            }
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
            #[cfg(feature = "mux-qualification")]
            packet_lifetime::track(
                &mut packet,
                stream.input_index,
                ffi::AVRational {
                    num: stream.time_base.numerator,
                    den: stream.time_base.denominator,
                },
            )?;
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

/// Qualification-only, fixed two-track payload census. The wrapper follows
/// native AVBuffer references too, not just Rust AVPacket wrapper lifetimes.
#[cfg(feature = "mux-qualification")]
pub(crate) mod packet_lifetime {
    use super::*;
    use std::{
        fs::File,
        io::Write,
        sync::{Mutex, OnceLock},
    };

    struct State {
        file: File,
        current: [u64; 2],
        bytes: [u64; 2],
        peak: [u64; 2],
        peak_bytes: [u64; 2],
        read: [u64; 2],
        consumed: [u64; 2],
        side_bytes: [u64; 2],
        checkpoint: usize,
        queue_peak: usize,
        accounting_error: bool,
    }
    static STATE: OnceLock<Option<Mutex<State>>> = OnceLock::new();
    fn state() -> Option<&'static Mutex<State>> {
        STATE
            .get_or_init(|| {
                std::env::var_os("ASCIIFLOW_D1B_AUDIO_LIFETIME").map(|path| {
                    Mutex::new(State {
                        file: File::options()
                            .write(true)
                            .create_new(true)
                            .open(path)
                            .expect("create audio lifetime diagnostic"),
                        current: [0; 2],
                        bytes: [0; 2],
                        peak: [0; 2],
                        peak_bytes: [0; 2],
                        read: [0; 2],
                        consumed: [0; 2],
                        side_bytes: [0; 2],
                        checkpoint: 0,
                        queue_peak: 0,
                        accounting_error: false,
                    })
                })
            })
            .as_ref()
    }
    pub(super) fn enabled() -> bool {
        state().is_some()
    }
    struct Payload {
        original: *mut ffi::AVBufferRef,
        track: usize,
        bytes: u64,
    }
    unsafe extern "C" fn released(opaque: *mut std::ffi::c_void, _data: *mut u8) {
        let mut payload = unsafe { Box::from_raw(opaque.cast::<Payload>()) };
        unsafe { ffi::av_buffer_unref(&mut payload.original) };
        let mut state = state().unwrap().lock().unwrap_or_else(|e| e.into_inner());
        if state.current[payload.track] == 0 || state.bytes[payload.track] < payload.bytes {
            state.accounting_error = true;
        } else {
            state.current[payload.track] -= 1;
            state.bytes[payload.track] -= payload.bytes;
        }
    }
    pub(super) fn track(
        packet: &mut Packet,
        index: usize,
        time_base: ffi::AVRational,
    ) -> Result<()> {
        let Some(census) = state() else {
            return Ok(());
        };
        if !(1..=2).contains(&index) {
            return Err(Error::Media(
                "audio lifetime fixture requires streams1,2".into(),
            ));
        }
        let track = index - 1;
        let native = unsafe { &mut *packet.as_mut_ptr() };
        if native.buf.is_null() {
            return Err(Error::Media(
                "audio lifetime requires refcounted input packet".into(),
            ));
        }
        let bytes = native.size.max(0) as u64;
        let payload = Box::into_raw(Box::new(Payload {
            original: native.buf,
            track,
            bytes,
        }));
        let buffer = unsafe {
            ffi::av_buffer_create(
                (*native.buf).data,
                (*native.buf).size,
                Some(released),
                payload.cast(),
                ffi::AV_BUFFER_FLAG_READONLY,
            )
        };
        if buffer.is_null() {
            drop(unsafe { Box::from_raw(payload) });
            return Err(Error::Media(
                "allocate audio lifetime buffer reference".into(),
            ));
        }
        native.buf = buffer;
        let equivalent = unsafe {
            ffi::av_rescale_q(native.pts, time_base, ffi::AVRational { num: 1, den: 50 })
        };
        let mut state = census.lock().expect("audio lifetime lock");
        state.current[track] += 1;
        state.bytes[track] += bytes;
        state.read[track] += 1;
        state.peak[track] = state.peak[track].max(state.current[track]);
        state.peak_bytes[track] = state.peak_bytes[track].max(state.bytes[track]);
        for i in 0..native.side_data_elems as usize {
            state.side_bytes[track] += unsafe { (*native.side_data.add(i)).size as u64 };
        }
        let checkpoints = [10_000, 25_000, 50_000, 75_000, 99_940];
        let record =
            state.checkpoint < checkpoints.len() && equivalent >= checkpoints[state.checkpoint];
        if record {
            state.checkpoint += 1;
        }
        drop(state);
        if record {
            sample("checkpoint")?;
        }
        Ok(())
    }
    pub(crate) fn consumed(index: usize) {
        if let Some(census) = state() {
            let mut state = census.lock().expect("audio lifetime lock");
            if (1..=2).contains(&index) {
                state.consumed[index - 1] += 1;
            } else {
                state.accounting_error = true;
            }
        }
    }
    pub(crate) fn queue_observed(len: usize, capacity: Option<usize>) {
        if let Some(census) = state() {
            let mut state = census.lock().expect("audio lifetime lock");
            state.queue_peak = state.queue_peak.max(len);
            state.accounting_error |= capacity != Some(16) || len > 16;
        }
    }
    pub(crate) fn sample(phase: &str) -> Result<()> {
        let Some(census) = state() else {
            return Ok(());
        };
        let rollup = std::fs::read_to_string("/proc/self/smaps_rollup")
            .map_err(|e| Error::Media(e.to_string()))?;
        let value = |key: &str| -> u64 {
            rollup
                .lines()
                .find_map(|line| line.strip_prefix(key))
                .and_then(|s| s.split_whitespace().next())
                .and_then(|s| s.parse().ok())
                .unwrap_or(0)
        };
        let mut state = census.lock().expect("audio lifetime lock");
        let row = format!(
            "{{\"phase\":\"{phase}\",\"rss_kib\":{},\"pss_kib\":{},\"anonymous_kib\":{},\"private_dirty_kib\":{},\"current_packets\":{:?},\"current_bytes\":{:?},\"peak_packets\":{:?},\"peak_bytes\":{:?},\"read\":{:?},\"mux_consumed\":{:?},\"total_side_data_bytes_read\":{:?},\"queue_peak\":{},\"accounting_error\":{}}}\n",
            value("Rss:"),
            value("Pss:"),
            value("Anonymous:"),
            value("Private_Dirty:"),
            state.current,
            state.bytes,
            state.peak,
            state.peak_bytes,
            state.read,
            state.consumed,
            state.side_bytes,
            state.queue_peak,
            state.accounting_error
        );
        state
            .file
            .write_all(row.as_bytes())
            .and_then(|()| state.file.flush())
            .map_err(|e| Error::Media(e.to_string()))
    }
    pub(crate) fn finish() -> Result<()> {
        let Some(census) = state() else {
            return Ok(());
        };
        sample("after_job_teardown")?;
        {
            let state = census.lock().expect("audio lifetime lock");
            if state.accounting_error
                || state.current != [0; 2]
                || state.bytes != [0; 2]
                || state.read != state.consumed
            {
                return Err(Error::Media(
                    "audio lifetime diagnostic: outstanding payloads or unmatched mux progress"
                        .into(),
                ));
            }
        }
        // One diagnosis-only trim, after all job-owned objects have been dropped.
        // Never compiled into a default production build.
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        unsafe {
            libc::malloc_trim(0);
        }
        sample("after_diagnostic_trim")
    }

    #[cfg(test)]
    #[test]
    #[ignore = "isolated native payload lifetime check; explicit diagnostic file required"]
    fn d1b_payload_census_follows_last_native_reference() {
        assert!(enabled());
        let mut packet = Packet::new().unwrap();
        check(
            unsafe { ffi::av_new_packet(packet.as_mut_ptr(), 256) },
            "allocate census test packet",
        )
        .unwrap();
        let original = unsafe { ffi::av_buffer_ref((*packet.as_mut_ptr()).buf) };
        assert!(!original.is_null());
        track(
            &mut packet,
            1,
            ffi::AVRational {
                num: 1,
                den: 48_000,
            },
        )
        .unwrap();
        let mut clone = Packet::new().unwrap();
        check(
            unsafe { ffi::av_packet_ref(clone.as_mut_ptr(), packet.as_mut_ptr()) },
            "clone census packet",
        )
        .unwrap();
        drop(packet);
        assert_eq!(state().unwrap().lock().unwrap().current, [1, 0]);
        assert_eq!(state().unwrap().lock().unwrap().bytes, [256, 0]);
        drop(clone);
        let census = state().unwrap().lock().unwrap();
        assert_eq!(census.current, [0, 0]);
        assert_eq!(census.bytes, [0, 0]);
        assert!(!census.accounting_error);
        assert_eq!(unsafe { ffi::av_buffer_get_ref_count(original) }, 1);
        let mut original = original;
        unsafe { ffi::av_buffer_unref(&mut original) };
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
