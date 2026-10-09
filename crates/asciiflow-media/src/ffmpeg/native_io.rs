//! Test-only observations around real URL-backed AVIO and the native mux owner.
use super::*;
use std::{
    cell::Cell,
    collections::BTreeMap,
    ffi::c_void,
    sync::{
        OnceLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

thread_local! { static BOUNDARY: Cell<&'static str> = const { Cell::new("outside-marked-call") }; }
pub(crate) struct Boundary(&'static str);
impl Boundary {
    pub(crate) fn enter(name: &'static str) -> Self {
        Self(BOUNDARY.replace(name))
    }
}
impl Drop for Boundary {
    fn drop(&mut self) {
        BOUNDARY.set(self.0);
    }
}

type WriteCallback = unsafe extern "C" fn(*mut c_void, *const u8, i32) -> i32;
#[derive(Clone, Copy)]
enum Mode {
    Healthy,
    Fault(&'static str, i32),
    Slow(&'static str),
}
struct Trace {
    accepted: u64,
    calls: usize,
    gates: usize,
    events: Vec<serde_json::Value>,
}
struct Probe {
    original: WriteCallback,
    mode: Mode,
    trace: Mutex<Trace>,
    entered: Sender<&'static str>,
    permit: Receiver<()>,
}
fn probes() -> &'static Mutex<BTreeMap<usize, Arc<Probe>>> {
    static PROBES: OnceLock<Mutex<BTreeMap<usize, Arc<Probe>>>> = OnceLock::new();
    PROBES.get_or_init(Mutex::default)
}
struct Registration(usize);
impl Drop for Registration {
    fn drop(&mut self) {
        probes().lock().unwrap().remove(&self.0);
    }
}

unsafe extern "C" fn observed_write(opaque: *mut c_void, bytes: *const u8, size: i32) -> i32 {
    // Never unwind across C. The original opaque remains a URLContext and is
    // passed unchanged to FFmpeg's original callback and native close owner.
    std::panic::catch_unwind(|| {
        let probe = probes()
            .lock()
            .unwrap()
            .get(&(opaque as usize))
            .cloned()
            .unwrap();
        let boundary = BOUNDARY.get();
        let (sequence, accepted_before) = {
            let mut trace = probe.trace.lock().unwrap();
            if trace.events.len() >= 8192 {
                return ffi::AVERROR(libc::EOVERFLOW);
            }
            trace.calls += 1;
            (trace.calls, trace.accepted)
        };
        let fail = matches!(probe.mode, Mode::Fault(target, _) if target == boundary);
        let hold = match probe.mode {
            Mode::Slow("sustained") => boundary == "packet-write",
            Mode::Slow("bursty") => boundary == "packet-write" && sequence % 7 == 0,
            Mode::Slow("slow-flush") => boundary == "intermediate-flush",
            Mode::Slow("slow-finalization") => boundary == "trailer",
            _ => false,
        };
        let result = if fail {
            let Mode::Fault(_, errno) = probe.mode else {
                unreachable!()
            };
            ffi::AVERROR(errno)
        } else if hold
            && (probe
                .entered
                .send_timeout(boundary, Duration::from_secs(5))
                .is_err()
                || probe.permit.recv_timeout(Duration::from_secs(5)).is_err())
        {
            ffi::AVERROR(libc::ETIMEDOUT)
        } else {
            unsafe { (probe.original)(opaque, bytes, size) }
        };
        let mut trace = probe.trace.lock().unwrap();
        if hold {
            trace.gates += 1;
        }
        if result > 0 {
            trace.accepted += result as u64;
        }
        trace.events.push(serde_json::json!({
            "callback": sequence, "actual_boundary": boundary,
            "bytes_accepted_before": accepted_before, "attempted_bytes": size,
            "returned": result, "injected_fault": fail, "controlled_hold": hold,
        }));
        result
    })
    .unwrap_or_else(|_| ffi::AVERROR(libc::EIO))
}

struct ObservedOutput {
    output: Option<MuxOutput>,
    _registration: Registration,
    probe: Arc<Probe>,
    entered: Receiver<&'static str>,
    permit: Sender<()>,
}
impl Drop for ObservedOutput {
    fn drop(&mut self) {
        let _boundary = Boundary::enter("avio-close");
        drop(self.output.take());
    }
}
impl ObservedOutput {
    fn open(capture: &Capture, path: &Path, mode: Mode) -> Self {
        let native = CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        let mut raw = ptr::null_mut();
        check(
            unsafe {
                ffi::avformat_alloc_output_context2(
                    &mut raw,
                    ptr::null(),
                    c"mp4".as_ptr(),
                    native.as_ptr(),
                )
            },
            "allocate observed MP4",
        )
        .unwrap();
        let mut output = MuxOutput {
            format: NonNull::new(raw).unwrap(),
            video_stream_index: 0,
            video_time_base: ffi::AVRational { num: 1, den: 1 },
            audio_routes: Vec::new(),
        };
        for index in 0..unsafe { (*capture.format.as_ptr()).nb_streams } as usize {
            unsafe {
                let source = capture.stream(index);
                let target = ffi::avformat_new_stream(raw, ptr::null());
                assert!(!target.is_null());
                check(
                    ffi::avcodec_parameters_copy((*target).codecpar, (*source).codecpar),
                    "copy observed codec parameters",
                )
                .unwrap();
                (*(*target).codecpar).codec_tag = 0;
                (*target).time_base = (*source).time_base;
                (*target).disposition = (*source).disposition;
                check(
                    ffi::av_dict_copy(&mut (*target).metadata, (*source).metadata, 0),
                    "copy observed stream metadata",
                )
                .unwrap();
                if capture.audio(index) {
                    output.audio_routes.push(AudioMuxRoute {
                        input_index: index,
                        output_index: index as i32,
                        output_time_base: (*source).time_base,
                    });
                } else {
                    output.video_stream_index = index as i32;
                }
            }
        }
        check(
            unsafe { ffi::avio_open(&mut (*raw).pb, native.as_ptr(), ffi::AVIO_FLAG_WRITE) },
            "open observed native AVIO",
        )
        .unwrap();
        let pb = unsafe { &mut *(*raw).pb };
        let (entered_tx, entered) = bounded(1);
        let (permit, permit_rx) = bounded(1);
        let probe = Arc::new(Probe {
            original: pb.write_packet.unwrap(),
            mode,
            trace: Mutex::new(Trace {
                accepted: 0,
                calls: 0,
                gates: 0,
                events: Vec::new(),
            }),
            entered: entered_tx,
            permit: permit_rx,
        });
        let key = pb.opaque as usize;
        assert!(
            probes()
                .lock()
                .unwrap()
                .insert(key, probe.clone())
                .is_none()
        );
        pb.write_packet = Some(observed_write);
        // Direct AVIO writes make the requested boundaries observable without
        // substituting an outer error for an actual native callback failure.
        pb.direct = 1;
        Self {
            output: Some(output),
            _registration: Registration(key),
            probe,
            entered,
            permit,
        }
    }

    fn header(&mut self) -> Result<()> {
        let output = self.output.as_mut().unwrap();
        let _boundary = Boundary::enter("header");
        let written =
            unsafe { ffi::avformat_write_header(output.format.as_ptr(), ptr::null_mut()) };
        let io_error = unsafe { (*(*output.format.as_ptr()).pb).error };
        check(written.min(io_error), "write observed MP4 header").map_err(|error| {
            Error::pipeline(
                PipelineStage::MuxInitialization,
                "write MP4 container header",
                error,
            )
        })?;
        unsafe {
            output.video_time_base = (**(*output.format.as_ptr())
                .streams
                .add(output.video_stream_index as usize))
            .time_base;
            for route in &mut output.audio_routes {
                route.output_time_base = (**(*output.format.as_ptr())
                    .streams
                    .add(route.output_index as usize))
                .time_base;
            }
        }
        Ok(())
    }

    fn trace(&self) -> serde_json::Value {
        let trace = self.probe.trace.lock().unwrap();
        serde_json::json!({"callbacks": trace.calls, "bytes_accepted": trace.accepted,
            "controlled_holds": trace.gates, "events": trace.events})
    }
}

struct DriveOutcome {
    result: Result<()>,
    counts: Vec<usize>,
    peaks: [usize; 2],
    backpressure: [usize; 2],
}
fn drive(capture: &Capture, io: &mut ObservedOutput) -> DriveOutcome {
    let output = io.output.take().unwrap();
    let (vtx, vrx) = bounded(MUX_CHANNEL_CAPACITY);
    let (atx, arx) = bounded(MUX_CHANNEL_CAPACITY);
    let video_observer = vtx.clone();
    let audio_observer = atx.clone();
    assert_eq!(video_observer.capacity(), Some(16));
    assert_eq!(audio_observer.capacity(), Some(16));
    let video_done = AtomicBool::new(false);
    let audio_done = AtomicBool::new(false);
    let blocked = [AtomicBool::new(false), AtomicBool::new(false)];
    let backpressure = [AtomicUsize::new(0), AtomicUsize::new(0)];
    let (completed, completion) = bounded(1);
    let mut peaks = [0; 2];
    std::thread::scope(|scope| {
        let mut producers = Vec::new();
        for (lane, (audio, sender, done)) in [(false, vtx, &video_done), (true, atx, &audio_done)]
            .into_iter()
            .enumerate()
        {
            let blocked = &blocked[lane];
            let backpressure = &backpressure[lane];
            producers.push(scope.spawn(move || {
                let submit = |message| match sender.try_send(message) {
                    Ok(()) => true,
                    Err(crossbeam_channel::TrySendError::Full(message)) => {
                        backpressure.fetch_add(1, Ordering::Relaxed);
                        blocked.store(true, Ordering::Release);
                        let sent = sender.send_timeout(message, Duration::from_secs(5)).is_ok();
                        blocked.store(false, Ordering::Release);
                        sent
                    }
                    Err(crossbeam_channel::TrySendError::Disconnected(_)) => false,
                };
                let mut sent = 0;
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
                        "reference observed packet",
                    )
                    .unwrap();
                    if !submit(MuxMessage::Packet {
                        packet,
                        input_index: *index,
                        input_time_base: unsafe { (*capture.stream(*index)).time_base },
                        audio,
                    }) {
                        break;
                    }
                    sent += 1;
                }
                let _ = submit(if audio {
                    MuxMessage::AudioDone
                } else {
                    MuxMessage::Finish
                });
                done.store(true, Ordering::Release);
                sent
            }));
        }
        let worker = scope.spawn(move || {
            let result = run_mux_worker(
                output,
                mux_inbox::MuxInputs {
                    video: &vrx,
                    audio: Some(&arx),
                },
                Arc::new(Mutex::new(MuxStats::default())),
                Arc::new(Mutex::new(None)),
                CancellationToken::new(),
                CancellationToken::new(),
                None,
            )
            .map_err(|error| {
                Error::pipeline(PipelineStage::MuxRuntime, "write interleaved packet", error)
            });
            completed.send(result).unwrap();
        });
        let result = loop {
            crossbeam_channel::select! {
                recv(completion) -> result => break result.unwrap(),
                recv(io.entered) -> boundary => {
                    let boundary = boundary.unwrap();
                    if boundary != "trailer" {
                        let deadline = Instant::now() + Duration::from_secs(2);
                        while ((!video_observer.is_full() || !blocked[0].load(Ordering::Acquire)) && !video_done.load(Ordering::Acquire))
                            || ((!audio_observer.is_full() || !blocked[1].load(Ordering::Acquire)) && !audio_done.load(Ordering::Acquire)) {
                            assert!(Instant::now() < deadline, "producers did not reach bounded native-I/O backpressure");
                            std::thread::yield_now();
                        }
                    }
                    peaks[0] = peaks[0].max(video_observer.len());
                    peaks[1] = peaks[1].max(audio_observer.len());
                    io.permit.send_timeout((), Duration::from_secs(2)).unwrap();
                },
                default(Duration::from_secs(10)) => panic!("native mux controlled-I/O completion watchdog"),
            }
        };
        worker.join().unwrap();
        let counts = producers
            .into_iter()
            .map(|producer| producer.join().unwrap())
            .collect();
        DriveOutcome {
            result,
            counts,
            peaks,
            backpressure: backpressure
                .each_ref()
                .map(|count| count.load(Ordering::Relaxed)),
        }
    })
}

#[test]
#[ignore = "native AVIO phase/slow-consumer matrix; release and ASCIIFLOW_NATIVE_IO_DIR=new directory; run alone"]
fn native_avio_phase_faults_and_controlled_slow_sink() {
    assert!(
        !std::hint::black_box(cfg!(debug_assertions)),
        "run with --release"
    );
    let directory = PathBuf::from(std::env::var_os("ASCIIFLOW_NATIVE_IO_DIR").unwrap());
    fs::create_dir(&directory).unwrap();
    let source =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/media/multiple.mp4");
    let before_capture = fd_count();
    let capture = Capture::read(&source);
    let baseline = fd_count();
    let reference_path = directory.join("healthy-reference.mp4");
    let mut reference = ObservedOutput::open(&capture, &reference_path, Mode::Healthy);
    reference.header().unwrap();
    drive(&capture, &mut reference).result.unwrap();
    drop(reference);
    let expected = fs::read(&reference_path).unwrap();
    assert_eq!(fd_count(), baseline);
    let mut faults = Vec::new();
    for boundary in ["header", "packet-write", "intermediate-flush", "trailer"] {
        for errno in [libc::ENOSPC, libc::EIO] {
            let path = directory.join(format!("{boundary}-{errno}.mp4"));
            let mut io = ObservedOutput::open(&capture, &path, Mode::Fault(boundary, errno));
            let error = match io.header() {
                Err(error) => error,
                Ok(()) => drive(&capture, &mut io)
                    .result
                    .expect_err("native fault callback must fail"),
            };
            let trace = io.trace();
            let failures: Vec<_> = trace["events"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|event| event["injected_fault"] == true)
                .collect();
            assert!(
                !failures.is_empty(),
                "no actual {boundary} callback failure"
            );
            assert!(
                failures
                    .iter()
                    .all(|event| event["actual_boundary"] == boundary)
            );
            assert!(
                error.to_string().contains(&format!("({})", -errno)),
                "{error}"
            );
            let expected_stage = match boundary {
                "header" => PipelineStage::MuxInitialization,
                "trailer" => PipelineStage::Finalization,
                _ => PipelineStage::MuxRuntime,
            };
            assert_eq!(error.stage(), Some(expected_stage));
            drop(io);
            assert_eq!(fd_count(), baseline, "native fault FD cleanup");
            let recovered_path = directory.join(format!("recovered-{boundary}-{errno}.mp4"));
            let mut recovered = ObservedOutput::open(&capture, &recovered_path, Mode::Healthy);
            recovered.header().unwrap();
            drive(&capture, &mut recovered).result.unwrap();
            drop(recovered);
            assert_eq!(fs::read(&recovered_path).unwrap(), expected);
            verify_counted_replay(&capture, &recovered_path, 1, &[0, 0, 0]);
            assert_eq!(fd_count(), baseline, "native fault recovery FD cleanup");
            faults.push(serde_json::json!({"requested_boundary": boundary, "errno": errno,
                "classification": "SimulatedOnly", "error_surfaced_at": format!("{:?}", error.stage()),
                "root_cause_category": "MediaNativeErrno",
                "root": error.to_string(), "trace": trace, "fd_after": fd_count(),
                "immediate_healthy_recovery_exact_bytes_payload_duration_pts_dts": true}));
        }
    }
    let mut slow = Vec::new();
    for mode in ["sustained", "bursty", "slow-flush", "slow-finalization"] {
        let path = directory.join(format!("{mode}.mp4"));
        let mut io = ObservedOutput::open(&capture, &path, Mode::Slow(mode));
        io.header().unwrap();
        let DriveOutcome {
            result,
            counts,
            peaks,
            backpressure,
        } = drive(&capture, &mut io);
        result.unwrap();
        let trace = io.trace();
        assert!(
            trace["controlled_holds"].as_u64().unwrap() > 0,
            "{mode} never reached actual controlled native write"
        );
        if mode != "slow-finalization" {
            assert_eq!(peaks, [16, 16]);
            assert!(backpressure.iter().all(|count| *count > 0));
        }
        assert_eq!(counts.iter().sum::<usize>(), capture.packets.len());
        assert_eq!(fs::read(&path).unwrap(), expected, "{mode} mux determinism");
        let stream_counts = verify_counted_replay(&capture, &path, 1, &[0, 0, 0]);
        drop(io);
        assert_eq!(fd_count(), baseline);
        slow.push(
            serde_json::json!({"mode": mode, "classification": "NativePass",
            "control": "event-controlled holds inside actual native AVIO callback",
            "project_mux_capacities": [16, 16], "witnessed_full_depths": peaks,
            "producer_full_then_blocking_send_events": backpressure,
            "producer_counts_video_audio": counts, "readback_counts_per_stream": stream_counts,
            "exact_mp4_bytes": true, "exact_payload_duration_pts_dts": true,
            "trace": trace, "fd_after": fd_count()}),
        );
    }
    // /dev/full gives real kernel write ENOSPC without filling any filesystem.
    // This device control does not qualify quota/filesystem exhaustion.
    let full_device = if Path::new("/dev/full").exists() {
        let mut io = ObservedOutput::open(&capture, Path::new("/dev/full"), Mode::Healthy);
        let error = io.header().expect_err("kernel /dev/full write must fail");
        assert_eq!(error.stage(), Some(PipelineStage::MuxInitialization));
        assert!(error.to_string().contains("(-28)"));
        let trace = io.trace();
        assert!(trace["events"].as_array().unwrap().iter().any(|event| event["returned"] == -libc::ENOSPC && event["injected_fault"] == false));
        drop(io);
        assert_eq!(fd_count(), baseline);
        serde_json::json!({"classification": "NativePass", "kind": "kernel-character-device-write-control",
            "path": "/dev/full", "errno": libc::ENOSPC, "root": error.to_string(), "trace": trace,
            "qualification": "real native file-descriptor write error; not filesystem quota/disk exhaustion"})
    } else {
        serde_json::json!({"classification": "UnavailableSafely", "reason": "/dev/full unavailable"})
    };
    drop(capture);
    assert_eq!(fd_count(), before_capture);
    fs::write(directory.join("native-io.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "schema": "asciiflow-native-io-closure-v1", "faults": faults, "slow_consumers": slow,
        "native_kernel_enospc_control": full_device,
        "avio_direct": true,
        "avio_mode_scope": "test-configured direct native AVIO for controllable write boundaries; production buffering policy unchanged",
        "queue_peak_scope": "full channel capacity witnessed while native AVIO callback held; not periodic-sample inference or a total-memory census",
        "fd_before_capture": before_capture, "fd_final": fd_count(),
        "remaining": ["real isolated filesystem ENOSPC", "pipeline queue capacity-three native slow-IO integration", "CLI mux write plus staging-removal composite"],
    })).unwrap()).unwrap();
}
