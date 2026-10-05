//! Mux-only diagnostics using native captured codec parameters and packet refs.
use super::*;
use std::{ffi::CString, path::PathBuf};

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
