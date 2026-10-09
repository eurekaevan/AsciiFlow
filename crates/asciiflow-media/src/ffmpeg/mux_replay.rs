//! Mux-only diagnostics using native captured codec parameters and packet refs.
use super::*;
use std::{
    ffi::CString,
    fs::{self, OpenOptions},
    io::{BufWriter, Write},
    path::PathBuf,
};

#[path = "native_io.rs"]
pub(super) mod native_io;

struct Capture {
    format: NonNull<ffi::AVFormatContext>,
    packets: Vec<(usize, Packet)>,
}

impl Capture {
    fn read(path: &Path) -> Self {
        let name = CString::new(path.to_str().unwrap()).unwrap();
        let mut raw = ptr::null_mut();
        check(
            unsafe {
                ffi::avformat_open_input(&mut raw, name.as_ptr(), ptr::null_mut(), ptr::null_mut())
            },
            "open replay",
        )
        .unwrap();
        let mut capture = Self {
            format: NonNull::new(raw).unwrap(),
            packets: Vec::new(),
        };
        check(
            unsafe { ffi::avformat_find_stream_info(raw, ptr::null_mut()) },
            "probe replay",
        )
        .unwrap();
        loop {
            let mut packet = Packet::new().unwrap();
            let result = unsafe { ffi::av_read_frame(raw, packet.as_mut_ptr()) };
            if result == ffi::AVERROR_EOF {
                break;
            }
            check(result, "read replay packet").unwrap();
            capture.packets.push((
                unsafe { (*packet.as_mut_ptr()).stream_index } as usize,
                packet,
            ));
        }
        capture
    }

    fn stream(&self, index: usize) -> *mut ffi::AVStream {
        unsafe { *(*self.format.as_ptr()).streams.add(index) }
    }

    fn audio(&self, index: usize) -> bool {
        unsafe {
            (*(*self.stream(index)).codecpar).codec_type == ffi::AVMediaType::AVMEDIA_TYPE_AUDIO
        }
    }

    fn output(&self, path: &Path) -> MuxOutput {
        let name = CString::new(path.to_str().unwrap()).unwrap();
        let mut raw = ptr::null_mut();
        check(
            unsafe {
                ffi::avformat_alloc_output_context2(
                    &mut raw,
                    ptr::null(),
                    c"mp4".as_ptr(),
                    name.as_ptr(),
                )
            },
            "allocate replay",
        )
        .unwrap();
        let mut output = MuxOutput {
            format: NonNull::new(raw).unwrap(),
            video_stream_index: 0,
            video_time_base: ffi::AVRational { num: 1, den: 1 },
            audio_routes: Vec::new(),
        };
        for i in 0..unsafe { (*self.format.as_ptr()).nb_streams } as usize {
            unsafe {
                let source = self.stream(i);
                let target = ffi::avformat_new_stream(raw, ptr::null());
                assert!(!target.is_null());
                check(
                    ffi::avcodec_parameters_copy((*target).codecpar, (*source).codecpar),
                    "copy replay parameters",
                )
                .unwrap();
                (*(*target).codecpar).codec_tag = 0;
                (*target).time_base = (*source).time_base;
                (*target).disposition = (*source).disposition;
                check(
                    ffi::av_dict_copy(&mut (*target).metadata, (*source).metadata, 0),
                    "copy replay metadata",
                )
                .unwrap();
                if self.audio(i) {
                    output.audio_routes.push(AudioMuxRoute {
                        input_index: i,
                        output_index: i as i32,
                        output_time_base: (*source).time_base,
                    });
                } else {
                    assert_eq!(
                        (*(*source).codecpar).codec_type,
                        ffi::AVMediaType::AVMEDIA_TYPE_VIDEO
                    );
                    output.video_stream_index = i as i32;
                }
            }
        }
        unsafe {
            check(
                ffi::avio_open(&mut (*raw).pb, name.as_ptr(), ffi::AVIO_FLAG_WRITE),
                "open replay output",
            )
            .unwrap();
            check(
                ffi::avformat_write_header(raw, ptr::null_mut()),
                "write replay header",
            )
            .unwrap();
            output.video_time_base =
                (**(*raw).streams.add(output.video_stream_index as usize)).time_base;
            for route in &mut output.audio_routes {
                route.output_time_base =
                    (**(*raw).streams.add(route.output_index as usize)).time_base;
            }
        }
        output
    }

    fn order(&self, pattern: &str) -> Vec<usize> {
        let video: Vec<_> = (0..self.packets.len())
            .filter(|&i| !self.audio(self.packets[i].0))
            .collect();
        let audio: Vec<_> = (0..self.packets.len())
            .filter(|&i| self.audio(self.packets[i].0))
            .collect();
        match pattern {
            "fixed" => (0..self.packets.len()).collect(),
            "video-ahead" => video.into_iter().chain(audio).collect(),
            "audio-ahead" => audio.into_iter().chain(video).collect(),
            _ => {
                let burst = if pattern == "alternating" { 1 } else { 16 };
                let (mut v, mut a, mut out) = (0, 0, Vec::new());
                while v < video.len() || a < audio.len() {
                    for _ in 0..burst {
                        if v < video.len() {
                            out.push(video[v]);
                            v += 1;
                        }
                    }
                    for _ in 0..burst {
                        if a < audio.len() {
                            out.push(audio[a]);
                            a += 1;
                        }
                    }
                }
                out
            }
        }
    }

    fn replay(&self, path: &Path, pattern: &str, flush: bool, delta: Option<i64>) {
        let output = self.output(path);
        let (tx, rx) = bounded(MUX_CHANNEL_CAPACITY);
        std::thread::scope(|scope| {
            scope.spawn(move || {
                tx.send(MuxMessage::IntermediateFlush(flush)).unwrap();
                if let Some(delta) = delta {
                    tx.send(MuxMessage::InterleaveDelta(delta)).unwrap();
                }
                for i in self.order(pattern) {
                    let (index, original) = &self.packets[i];
                    let mut packet = Packet::new().unwrap();
                    // ref(), not a shallow AVPacket copy: the writer consumes
                    // an independent reference, leaving the capture intact.
                    check(
                        unsafe {
                            ffi::av_packet_ref(packet.as_mut_ptr(), original.pointer_for_replay())
                        },
                        "reference replay packet",
                    )
                    .unwrap();
                    tx.send(MuxMessage::Packet {
                        packet,
                        input_index: *index,
                        input_time_base: unsafe { (*self.stream(*index)).time_base },
                        audio: self.audio(*index),
                    })
                    .unwrap();
                }
                tx.send(MuxMessage::AudioDone).unwrap();
                tx.send(MuxMessage::Finish).unwrap();
            });
            run_mux_worker(
                output,
                mux_inbox::MuxInputs {
                    video: &rx,
                    audio: None,
                },
                Arc::new(Mutex::new(MuxStats::default())),
                Arc::new(Mutex::new(None)),
                CancellationToken::new(),
                CancellationToken::new(),
                None,
            )
            .unwrap();
        });
    }

    fn deterministic_replay(&self, path: &Path, schedule: usize, cycles: usize) {
        let output = self.output(path);
        let (video_tx, video_rx) = bounded(MUX_CHANNEL_CAPACITY);
        let (audio_tx, audio_rx) = bounded(MUX_CHANNEL_CAPACITY);
        let trace =
            super::super::mux_trace::MuxTrace::create(&path.with_extension("jsonl")).unwrap();
        std::thread::scope(|scope| {
            for (audio, tx) in [(false, video_tx), (true, audio_tx)] {
                let trace = trace.clone();
                scope.spawn(move || {
                    if (schedule % 2 == 0) == audio {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    for cycle in 0..cycles {
                        for (local, (index, original)) in self
                            .packets
                            .iter()
                            .filter(|(index, _)| self.audio(*index) == audio)
                            .enumerate()
                        {
                            let mut packet = Packet::new().unwrap();
                            check(
                                unsafe {
                                    ffi::av_packet_ref(
                                        packet.as_mut_ptr(),
                                        original.pointer_for_replay(),
                                    )
                                },
                                "reference replay packet",
                            )
                            .unwrap();
                            let tb = unsafe { (*self.stream(*index)).time_base };
                            let shift = unsafe {
                                ffi::av_rescale_q(
                                    (cycle * 3) as i64,
                                    ffi::AVRational { num: 1, den: 1 },
                                    tb,
                                )
                            };
                            unsafe {
                                (*packet.as_mut_ptr()).pts += shift;
                                (*packet.as_mut_ptr()).dts += shift;
                            }
                            trace
                                .packet("A-produced", &mut packet, *index, audio, tb)
                                .unwrap();
                            if cycles == 1 && local % 17 == schedule % 17 {
                                std::thread::sleep(Duration::from_millis(1));
                            } else {
                                std::thread::yield_now();
                            }
                            tx.send(MuxMessage::Packet {
                                packet,
                                input_index: *index,
                                input_time_base: tb,
                                audio,
                            })
                            .unwrap();
                        }
                    }
                    tx.send(if audio {
                        MuxMessage::AudioDone
                    } else {
                        MuxMessage::Finish
                    })
                    .unwrap();
                });
            }
            run_mux_worker(
                output,
                mux_inbox::MuxInputs {
                    video: &video_rx,
                    audio: Some(&audio_rx),
                },
                Arc::new(Mutex::new(MuxStats::default())),
                Arc::new(Mutex::new(None)),
                CancellationToken::new(),
                CancellationToken::new(),
                Some(trace),
            )
            .unwrap();
        });
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        let mut raw = self.format.as_ptr();
        unsafe { ffi::avformat_close_input(&mut raw) }
    }
}
// Native demux reads have finished. Each replay creates independent refs;
// the capture and its codec parameters are immutable throughout the test.
unsafe impl Sync for Capture {}

/// Stress replay without per-packet diagnostic I/O. Only independent AVPacket
/// references cross the same bounded producer channels used by other replays.
fn counted_replay(
    capture: &Capture,
    path: &Path,
    cycles: usize,
    cycle_ticks: &[i64],
) -> Vec<usize> {
    let output = capture.output(path);
    let streams = unsafe { (*capture.format.as_ptr()).nb_streams } as usize;
    let emitted: Vec<_> = (0..streams)
        .map(|_| std::sync::atomic::AtomicUsize::new(0))
        .collect();
    let (video_tx, video_rx) = bounded(MUX_CHANNEL_CAPACITY);
    let (audio_tx, audio_rx) = bounded(MUX_CHANNEL_CAPACITY);
    std::thread::scope(|scope| {
        for (audio, sender) in [(false, video_tx), (true, audio_tx)] {
            let emitted = &emitted;
            scope.spawn(move || {
                for cycle in 0..cycles {
                    for (index, original) in capture
                        .packets
                        .iter()
                        .filter(|(i, _)| capture.audio(*i) == audio)
                    {
                        let mut packet = Packet::new().unwrap();
                        check(
                            unsafe {
                                ffi::av_packet_ref(
                                    packet.as_mut_ptr(),
                                    original.pointer_for_replay(),
                                )
                            },
                            "reference stress packet",
                        )
                        .unwrap();
                        let tb = unsafe { (*capture.stream(*index)).time_base };
                        let shift = cycle as i64 * cycle_ticks[*index];
                        unsafe {
                            (*packet.as_mut_ptr()).pts += shift;
                            (*packet.as_mut_ptr()).dts += shift;
                        }
                        sender
                            .send_timeout(
                                MuxMessage::Packet {
                                    packet,
                                    input_index: *index,
                                    input_time_base: tb,
                                    audio,
                                },
                                Duration::from_secs(30),
                            )
                            .expect("stress producer backpressure watchdog");
                        emitted[*index].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                }
                sender
                    .send_timeout(
                        if audio {
                            MuxMessage::AudioDone
                        } else {
                            MuxMessage::Finish
                        },
                        Duration::from_secs(30),
                    )
                    .unwrap();
            });
        }
        run_mux_worker(
            output,
            mux_inbox::MuxInputs {
                video: &video_rx,
                audio: Some(&audio_rx),
            },
            Arc::new(Mutex::new(MuxStats::default())),
            Arc::new(Mutex::new(None)),
            CancellationToken::new(),
            CancellationToken::new(),
            None,
        )
        .unwrap();
    });
    emitted
        .iter()
        .map(|count| count.load(std::sync::atomic::Ordering::Relaxed))
        .collect()
}

/// Stream the output rather than retaining one million packet references.
/// Per-stream packet ordinal identifies the exact source packet and cycle.
fn verify_counted_replay(
    capture: &Capture,
    path: &Path,
    cycles: usize,
    cycle_ticks: &[i64],
) -> Vec<usize> {
    struct Input(*mut ffi::AVFormatContext);
    impl Drop for Input {
        fn drop(&mut self) {
            unsafe { ffi::avformat_close_input(&mut self.0) }
        }
    }
    let name = CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
    let mut input = Input(ptr::null_mut());
    check(
        unsafe {
            ffi::avformat_open_input(
                &mut input.0,
                name.as_ptr(),
                ptr::null_mut(),
                ptr::null_mut(),
            )
        },
        "open stress readback",
    )
    .unwrap();
    check(
        unsafe { ffi::avformat_find_stream_info(input.0, ptr::null_mut()) },
        "probe stress readback",
    )
    .unwrap();
    let stream_count = unsafe { (*capture.format.as_ptr()).nb_streams } as usize;
    assert_eq!(unsafe { (*input.0).nb_streams } as usize, stream_count);
    let source: Vec<Vec<_>> = (0..stream_count)
        .map(|stream| {
            capture
                .packets
                .iter()
                .filter(|(i, _)| *i == stream)
                .map(|(_, packet)| packet)
                .collect()
        })
        .collect();
    let mut counts = vec![0; stream_count];
    let mut last_dts = vec![None; stream_count];
    let mut packet = Packet::new().unwrap();
    loop {
        let result = unsafe { ffi::av_read_frame(input.0, packet.as_mut_ptr()) };
        if result == ffi::AVERROR_EOF {
            break;
        }
        check(result, "read stress packet").unwrap();
        let actual = unsafe { &*packet.as_mut_ptr() };
        let index = actual.stream_index as usize;
        assert!(index < stream_count);
        let cycle = counts[index] / source[index].len();
        assert!(cycle < cycles, "unexpected extra packet in stream {index}");
        let original =
            unsafe { &*source[index][counts[index] % source[index].len()].pointer_for_replay() };
        let source_tb = unsafe { (*capture.stream(index)).time_base };
        let output_tb = unsafe { (**(*input.0).streams.add(index)).time_base };
        let shift = cycle as i64 * cycle_ticks[index];
        for (label, expected, received) in [
            ("PTS", original.pts, actual.pts),
            ("DTS", original.dts, actual.dts),
        ] {
            assert_ne!(expected, ffi::AV_NOPTS_VALUE, "source {label} unavailable");
            let expected = unsafe { ffi::av_rescale_q(expected + shift, source_tb, output_tb) };
            assert_eq!(
                received, expected,
                "stream {index} packet {} {label}",
                counts[index]
            );
        }
        let duration = unsafe { ffi::av_rescale_q(original.duration, source_tb, output_tb) };
        assert_eq!(
            actual.duration, duration,
            "stream {index} packet {} duration",
            counts[index]
        );
        assert_eq!(
            actual.size, original.size,
            "stream {index} packet {} compressed size",
            counts[index]
        );
        assert_eq!(actual.flags, original.flags, "stream {index} packet flags");
        fn payload(packet: &ffi::AVPacket) -> &[u8] {
            assert!(packet.size >= 0, "negative packet size");
            if packet.size == 0 {
                &[][..]
            } else {
                assert!(!packet.data.is_null(), "nonempty packet has no data");
                unsafe { std::slice::from_raw_parts(packet.data, packet.size as usize) }
            }
        }
        assert_eq!(
            payload(actual),
            payload(original),
            "stream {index} packet {} compressed payload",
            counts[index]
        );
        if let Some(previous) = last_dts[index] {
            assert!(actual.dts > previous, "stream {index} DTS regression");
        }
        last_dts[index] = Some(actual.dts);
        counts[index] += 1;
        unsafe { ffi::av_packet_unref(packet.as_mut_ptr()) };
    }
    for index in 0..stream_count {
        assert_eq!(counts[index], source[index].len() * cycles);
    }
    counts
}

#[test]
#[ignore = "isolated release Path C packet replay; input/output directory environment required"]
fn d1b_path_c_mux_memory_replay() {
    assert!(!std::hint::black_box(cfg!(debug_assertions)));
    let source = PathBuf::from(std::env::var_os("ASCIIFLOW_D1B_REPLAY_SOURCE").unwrap());
    let directory = PathBuf::from(std::env::var_os("ASCIIFLOW_D1B_REPLAY_DIR").unwrap());
    fs::create_dir(&directory).expect("replay directory must be new");
    let before = fd_count();
    // Capture completes before any measured mux interval. All original packet
    // buffers remain immutable and retained, not a live demux/audio producer.
    let capture = Capture::read(&source);
    let streams = unsafe { (*capture.format.as_ptr()).nb_streams } as usize;
    assert_eq!(streams, 3);
    // MOV demux exposes the track title as "name", while its muxer consumes
    // "title". Preserve the actual production title across this replay only.
    for index in 0..streams {
        let stream = capture.stream(index);
        unsafe {
            let name = ffi::av_dict_get((*stream).metadata, c"name".as_ptr(), ptr::null(), 0);
            if !name.is_null() {
                let title = std::ffi::CStr::from_ptr((*name).value).to_owned();
                check(
                    ffi::av_dict_set(
                        &mut (*stream).metadata,
                        c"title".as_ptr(),
                        title.as_ptr(),
                        0,
                    ),
                    "restore replay track title",
                )
                .unwrap();
            }
        }
    }
    let counts: Vec<_> = (0..streams)
        .map(|i| capture.packets.iter().filter(|(s, _)| *s == i).count())
        .collect();
    assert_eq!(counts, [100_000, 93_751, 93_751]);
    let baseline = fd_count();
    fs::write(directory.join("capture-ready.json"), serde_json::to_vec(&serde_json::json!({
        "pid": std::process::id(), "source_counts": counts,
        "fd_before": before, "fd_with_fixed_capture": baseline,
        "reliability_measurement": cfg!(feature = "reliability-measurement"),
        "scope": "immutable captured actual output packets, same mux owner, no live demux/decode/Vulkan/VAAPI/audio producer",
    })).unwrap()).unwrap();
    let path = directory.join("output.mp4");
    let output = capture.output(&path);
    let (video_tx, video_rx) = bounded(MUX_CHANNEL_CAPACITY);
    let (audio_tx, audio_rx) = bounded(MUX_CHANNEL_CAPACITY);
    std::thread::scope(|scope| {
        for (audio, sender) in [(false, video_tx), (true, audio_tx)] {
            let capture = &capture;
            scope.spawn(move || {
                let mut video_packets = 0;
                for (index, original) in capture
                    .packets
                    .iter()
                    .filter(|(i, _)| capture.audio(*i) == audio)
                {
                    let mut packet = Packet::new().unwrap();
                    check(
                        unsafe {
                            ffi::av_packet_ref(packet.as_mut_ptr(), original.pointer_for_replay())
                        },
                        "reference captured Path C packet",
                    )
                    .unwrap();
                    sender
                        .send_timeout(
                            MuxMessage::Packet {
                                packet,
                                input_index: *index,
                                input_time_base: unsafe { (*capture.stream(*index)).time_base },
                                audio,
                            },
                            Duration::from_secs(30),
                        )
                        .unwrap();
                    if !audio {
                        video_packets += 1;
                        // Pacing gives the external observer several samples
                        // per checkpoint; no payload/timestamp or mux change.
                        if video_packets == 50_000 || video_packets == 99_940 {
                            std::thread::sleep(Duration::from_millis(300));
                        } else if video_packets % 1000 == 0 {
                            std::thread::sleep(Duration::from_millis(25));
                        }
                    }
                }
                sender
                    .send_timeout(
                        if audio {
                            MuxMessage::AudioDone
                        } else {
                            MuxMessage::Finish
                        },
                        Duration::from_secs(30),
                    )
                    .unwrap();
            });
        }
        run_mux_worker(
            output,
            mux_inbox::MuxInputs {
                video: &video_rx,
                audio: Some(&audio_rx),
            },
            Arc::new(Mutex::new(MuxStats::default())),
            Arc::new(Mutex::new(None)),
            CancellationToken::new(),
            CancellationToken::new(),
            None,
        )
        .unwrap();
    });
    fs::write(directory.join("mux-complete"), b"complete").unwrap();
    assert_eq!(fd_count(), baseline);
    assert_eq!(
        verify_counted_replay(&capture, &path, 1, &[0, 0, 0]),
        counts
    );
    assert_eq!(fd_count(), baseline);
    drop(capture);
    assert_eq!(fd_count(), before);
    fs::write(
        directory.join("result.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "result": "PASS", "source": source, "packets_per_stream": counts,
            "exact_payload_pts_dts_duration_flags": true,
            "fd_before": before, "fd_final": fd_count(),
            "production_code_changed": false,
        }))
        .unwrap(),
    )
    .unwrap();
}

#[test]
#[ignore = "release-only at least one million real native mux packets; ASCIIFLOW_D1A_MILLION_PACKET_DIR must be new; run alone"]
fn d1a_million_packet_native_mux_replay() {
    assert!(
        !std::hint::black_box(cfg!(debug_assertions)),
        "run with --release"
    );
    let directory = PathBuf::from(std::env::var_os("ASCIIFLOW_D1A_MILLION_PACKET_DIR").unwrap());
    fs::create_dir(&directory).expect("stress evidence directory must be new");
    // A deadlock must fail the isolated qualification process, including native
    // code that cannot be interrupted by a Rust receive timeout.
    let (finished, watchdog) = bounded(1);
    let watchdog = std::thread::spawn(move || {
        if matches!(
            watchdog.recv_timeout(Duration::from_secs(300)),
            Err(crossbeam_channel::RecvTimeoutError::Timeout)
        ) {
            eprintln!("million-packet native mux exceeded the 300-second watchdog");
            std::process::abort();
        }
    });
    let started = Instant::now();
    let source =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/media/multiple.mp4");
    let before_capture = fd_count();
    let capture = Capture::read(&source);
    let baseline = fd_count();
    let stream_count = unsafe { (*capture.format.as_ptr()).nb_streams } as usize;
    let source_counts: Vec<_> = (0..stream_count)
        .map(|stream| capture.packets.iter().filter(|(i, _)| *i == stream).count())
        .collect();
    assert_eq!(
        source_counts.len(),
        3,
        "fixture must contain video and dual AAC"
    );
    // Stitch each stream at its own exact DTS span, including priming/tail.
    // A shared wall-clock gap makes MP4 extend the previous sample duration.
    // Independent stream cycles preserve every packet duration but deliberately
    // do not qualify synchronized repeating-media or A/V alignment semantics.
    let cycle_ticks: Vec<_> = (0..stream_count)
        .map(|index| {
            let packets: Vec<_> = capture
                .packets
                .iter()
                .filter(|(i, _)| *i == index)
                .map(|(_, p)| unsafe { &*p.pointer_for_replay() })
                .collect();
            let first = packets.first().unwrap();
            let last = packets.last().unwrap();
            let ticks = last.dts - first.dts + last.duration;
            assert!(ticks > 0);
            ticks
        })
        .collect();
    let stream_clocks: Vec<_> = (0..stream_count).map(|index| {
        let tb = unsafe { (*capture.stream(index)).time_base };
        serde_json::json!({ "stream": index, "time_base_num": tb.num, "time_base_den": tb.den, "cycle_ticks": cycle_ticks[index] })
    }).collect();
    let mut records = Vec::new();
    let cycles = 1_000_000usize.div_ceil(capture.packets.len());
    for (label, cycles) in [("two-cycle-preflight", 2), ("million-packet", cycles)] {
        let output = directory.join(format!("{label}.mp4"));
        let emitted = counted_replay(&capture, &output, cycles, &cycle_ticks);
        assert_eq!(fd_count(), baseline, "mux FD cleanup");
        let readback = verify_counted_replay(&capture, &output, cycles, &cycle_ticks);
        assert_eq!(emitted, readback);
        assert_eq!(fd_count(), baseline, "readback FD cleanup");
        records.push(serde_json::json!({
            "label": label, "cycles": cycles, "source_packets_per_stream": source_counts,
            "emitted_packets_per_stream": emitted, "readback_packets_per_stream": readback,
            "total_packets": emitted.iter().sum::<usize>(), "exact_integer_pts_dts": true,
            "exact_compressed_payload_and_duration": true,
            "output_bytes": fs::metadata(output).unwrap().len(), "fd_after": fd_count(),
        }));
    }
    assert!(records[1]["total_packets"].as_u64().unwrap() >= 1_000_000);
    drop(capture);
    assert_eq!(fd_count(), before_capture, "capture FD cleanup");
    fs::write(directory.join("million-packet.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "schema": "asciiflow-d1a-million-packet-v1", "source": source,
        "stream_clocks": stream_clocks, "native_mux": true, "trace_enabled": false,
        "scope": "native mux-only per-stream cyclic packets; not synchronized repeating-media or A/V alignment parity",
        "fd_before_capture": before_capture, "fd_with_capture": baseline, "fd_final": fd_count(),
        "elapsed_ms": started.elapsed().as_millis(), "records": records,
    })).unwrap()).unwrap();
    finished.send(()).unwrap();
    watchdog.join().unwrap();
}

#[test]
#[ignore = "native mux cancellation while waiting for a head, under backpressure, and with buffered packets"]
fn deterministic_merge_cancellation_releases_fds_and_packets() {
    let source = PathBuf::from(std::env::var_os("ASCIIFLOW_MUX_REPLAY_SOURCE").unwrap());
    let directory = PathBuf::from(std::env::var_os("ASCIIFLOW_MUX_REPLAY_DIRECTORY").unwrap());
    std::fs::create_dir(&directory).unwrap();
    let capture = Capture::read(&source);
    let fd_count = || std::fs::read_dir("/proc/self/fd").unwrap().count();
    let baseline = fd_count();
    for scenario in [
        "waiting-audio-head",
        "audio-enqueue-backpressure",
        "buffered-interleaver",
    ] {
        let output = capture.output(&directory.join(format!("{scenario}.mp4")));
        let (vtx, vrx) = bounded(MUX_CHANNEL_CAPACITY);
        let (atx, arx) = bounded(MUX_CHANNEL_CAPACITY);
        let cancel = CancellationToken::new();
        let failure = Arc::new(Mutex::new(None));
        let stats = Arc::new(Mutex::new(MuxStats::default()));
        let sender = AudioPacketSender::new(atx.clone(), cancel.clone(), failure.clone());
        std::thread::scope(|scope| {
            let control = cancel.clone();
            let worker_stats = stats.clone();
            let worker_failure = failure.clone();
            let worker = scope.spawn(|| {
                run_mux_worker(
                    output,
                    mux_inbox::MuxInputs {
                        video: &vrx,
                        audio: Some(&arx),
                    },
                    worker_stats,
                    worker_failure,
                    CancellationToken::new(),
                    control,
                    None,
                )
            });
            let producer = if scenario == "audio-enqueue-backpressure" {
                Some(scope.spawn(|| {
                    for (index, original) in capture
                        .packets
                        .iter()
                        .filter(|(index, _)| capture.audio(*index))
                    {
                        let mut packet = Packet::new().unwrap();
                        check(
                            unsafe {
                                ffi::av_packet_ref(
                                    packet.as_mut_ptr(),
                                    original.pointer_for_replay(),
                                )
                            },
                            "copy cancelled replay audio",
                        )
                        .unwrap();
                        if let Err(error) = sender.send(packet, *index, unsafe {
                            (*capture.stream(*index)).time_base
                        }) {
                            return error;
                        }
                    }
                    panic!("producer should encounter bounded backpressure before EOF");
                }))
            } else {
                None
            };
            if scenario != "audio-enqueue-backpressure" {
                for audio in [false, true] {
                    if audio && scenario == "waiting-audio-head" {
                        continue;
                    }
                    for (index, original) in capture
                        .packets
                        .iter()
                        .filter(|(index, _)| capture.audio(*index) == audio)
                        .take(if scenario == "waiting-audio-head" {
                            1
                        } else {
                            10
                        })
                    {
                        let mut packet = Packet::new().unwrap();
                        check(
                            unsafe {
                                ffi::av_packet_ref(
                                    packet.as_mut_ptr(),
                                    original.pointer_for_replay(),
                                )
                            },
                            "copy cancelled replay packet",
                        )
                        .unwrap();
                        (if audio { &atx } else { &vtx })
                            .send(MuxMessage::Packet {
                                packet,
                                input_index: *index,
                                input_time_base: unsafe { (*capture.stream(*index)).time_base },
                                audio,
                            })
                            .unwrap();
                    }
                }
            }
            let deadline = Instant::now() + Duration::from_secs(2);
            let ready = || match scenario {
                "audio-enqueue-backpressure" => arx.is_full(),
                "buffered-interleaver" => stats.lock().unwrap().audio_packets > 0,
                _ => vrx.is_empty(),
            };
            while !ready() && Instant::now() < deadline {
                std::thread::yield_now();
            }
            assert!(
                ready(),
                "{scenario}: required cancellation state not reached"
            );
            cancel.cancel();
            assert!(
                worker.join().unwrap().unwrap_err().is_cancelled(),
                "{scenario}"
            );
            if let Some(producer) = producer {
                assert!(producer.join().unwrap().is_cancelled());
            }
            assert!(
                failure.lock().unwrap().is_none(),
                "cancellation must not invent a root failure"
            );
        });
        assert_eq!(fd_count(), baseline, "{scenario}: mux FD was not restored");
    }
}

#[test]
#[ignore = "100 independent-producer native mux repetitions and 30k-packet stress"]
fn deterministic_producer_merge_replay() {
    let source = PathBuf::from(std::env::var_os("ASCIIFLOW_MUX_REPLAY_SOURCE").unwrap());
    let directory = PathBuf::from(std::env::var_os("ASCIIFLOW_MUX_REPLAY_DIRECTORY").unwrap());
    std::fs::create_dir(&directory).unwrap();
    let capture = Capture::read(&source);
    for run in 0..100 {
        capture.deterministic_replay(&directory.join(format!("fixed-{run:03}.mp4")), run, 1);
    }
    capture.deterministic_replay(&directory.join("stress-30084.mp4"), 0, 218);
}

#[test]
#[ignore = "1000 deterministic dual-producer mux replays across dual-audio and early-audio-EOF fixtures"]
fn soak_dual_and_sparse_producer_merge_replay() {
    const RUNS_PER_FIXTURE: usize = 500;

    let directory = PathBuf::from(std::env::var_os("ASCIIFLOW_MUX_REPLAY_DIRECTORY").unwrap());
    fs::create_dir(&directory).expect("replay evidence directory must be new");
    let report_path = directory.join("soak.jsonl");
    let report = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&report_path)
        .expect("soak report must be new");
    let mut report = BufWriter::new(report);
    let mut completed = 0;

    for (fixture, source) in [
        (
            "dual-audio",
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/media/multiple.mp4"),
        ),
        (
            "early-audio-eof",
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/media/video-longer.mp4"),
        ),
    ] {
        let fd_before_capture = fd_count();
        let capture = Capture::read(&source);
        let initial_fd_count = fd_count();
        let reference_path = directory.join(format!("{fixture}-reference.mp4"));
        let reference_trace = reference_path.with_extension("jsonl");
        capture.deterministic_replay(&reference_path, 0, 1);
        let reference = fs::read(&reference_path).expect("read first replay reference");
        let fd_after_reference = fd_count();
        let rss_after_reference_kib = rss_kib();
        assert_eq!(
            fd_after_reference, initial_fd_count,
            "{fixture} reference replay must restore the post-capture FD count"
        );
        writeln!(
            report,
            "{}",
            serde_json::json!({
                "fixture": fixture,
                "run": 0,
                "reference": true,
                "fd_before_capture": fd_before_capture,
                "fd_before": initial_fd_count,
                "fd_after": fd_after_reference,
                "matched": true,
                "rss_kib": rss_after_reference_kib,
                "output_bytes": reference.len(),
            })
        )
        .unwrap();
        report.flush().unwrap();
        completed += 1;

        for run in 1..RUNS_PER_FIXTURE {
            let output_path = directory.join(format!("{fixture}-{run:03}.mp4"));
            let trace_path = output_path.with_extension("jsonl");
            capture.deterministic_replay(&output_path, run, 1);
            let output = fs::read(&output_path).expect("read replay output");
            let fd_after = fd_count();
            let rss_after_kib = rss_kib();
            let matched = output == reference;
            writeln!(
                report,
                "{}",
                serde_json::json!({
                    "fixture": fixture,
                    "run": run,
                    "reference": false,
                    "fd_before": initial_fd_count,
                    "fd_after": fd_after,
                    "matched": matched,
                    "rss_kib": rss_after_kib,
                    "output_bytes": output.len(),
                })
            )
            .unwrap();
            report.flush().unwrap();
            assert_eq!(
                fd_after, initial_fd_count,
                "{fixture} run {run} must restore the post-capture FD count"
            );
            assert!(
                matched,
                "{fixture} run {run} differs byte-for-byte from its first replay"
            );

            // Keep only each fixture's reference MP4 and trace; all other
            // outputs are deleted only after FD and byte-identity checks pass.
            fs::remove_file(output_path).unwrap();
            fs::remove_file(trace_path).unwrap();
            completed += 1;
        }
        assert!(reference_path.is_file());
        assert!(reference_trace.is_file());
        drop(capture);
        let fd_after_capture_drop = fd_count();
        writeln!(
            report,
            "{}",
            serde_json::json!({
                "fixture": fixture,
                "capture_dropped": true,
                "fd_before_capture": fd_before_capture,
                "fd_after_capture_drop": fd_after_capture_drop,
            })
        )
        .unwrap();
        report.flush().unwrap();
        assert_eq!(
            fd_after_capture_drop, fd_before_capture,
            "{fixture} capture drop must restore the pre-capture FD count"
        );
    }

    assert_eq!(completed, 2 * RUNS_PER_FIXTURE);
    writeln!(
        report,
        "{}",
        serde_json::json!({ "summary": true, "runs": completed })
    )
    .unwrap();
    report.flush().unwrap();
}

fn fd_count() -> usize {
    fs::read_dir("/proc/self/fd").unwrap().count()
}

fn rss_kib() -> u64 {
    fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find(|line| line.starts_with("VmRSS:"))
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap()
}

#[test]
#[ignore = "native fixed-sequence and arrival-perturbation replay"]
fn fixed_sequence_and_arrival_replay() {
    let source = PathBuf::from(std::env::var_os("ASCIIFLOW_MUX_REPLAY_SOURCE").unwrap());
    let directory = PathBuf::from(std::env::var_os("ASCIIFLOW_MUX_REPLAY_DIRECTORY").unwrap());
    std::fs::create_dir(&directory).unwrap();
    let capture = Capture::read(&source);
    for i in 0..30 {
        capture.replay(
            &directory.join(format!("fixed-{i:03}.mp4")),
            "fixed",
            true,
            None,
        );
    }
    for pattern in ["video-ahead", "audio-ahead", "alternating", "bursts"] {
        for flush in [true, false] {
            capture.replay(
                &directory.join(format!("{pattern}-flush-{flush}.mp4")),
                pattern,
                flush,
                None,
            );
        }
        for delta in [1_000_000, 10_000_000] {
            capture.replay(
                &directory.join(format!("{pattern}-delta-{delta}.mp4")),
                pattern,
                false,
                Some(delta),
            );
        }
    }
}
