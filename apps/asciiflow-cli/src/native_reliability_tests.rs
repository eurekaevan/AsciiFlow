//! Same-process production-pipeline lifecycle qualification on a qualified
//! VAAPI/Vulkan host. Kept ignored and feature-gated so normal tests stay fast.
use super::{Args, DecodeMode, Decoder, FrameSource, VaapiOptions};
use crate::args::{OutputBitDepthArg, OutputCodecArg};
use asciiflow_core::{CancellationToken, ColorSpace, VideoCodec, reliability::Session};
use clap::Parser;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    fs,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const FRAMES_PER_JOB: u64 = 3;
const MIXED_SEQUENCES: usize = 20;

#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[path = "memory_plateau.rs"]
mod memory_plateau;

#[derive(Clone, Copy, Debug)]
enum InputKind {
    Sdr,
    Pq,
}

#[derive(Clone, Copy, Debug)]
enum OutputKind {
    Sdr8H264,
    Pq10Hevc,
    PqToSdr8H264,
    PqToSdr10Hevc,
}

#[derive(Clone, Copy, Debug)]
enum AudioKind {
    None,
    SingleAac,
    DualAac,
}

#[derive(Clone, Copy, Debug)]
enum HookAction {
    None,
    Cancel(&'static str),
    Panic {
        phase: &'static str,
        cancel_on_phase: bool,
    },
    InjectFailure {
        phase: &'static str,
        cancel_on_phase: bool,
    },
    HoldMuxUntilPipelinesFull,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OutcomeKind {
    Success,
    Cancelled,
    InjectedFailure,
}

#[derive(Debug)]
struct Case {
    label: String,
    input: InputKind,
    output: OutputKind,
    font: PathBuf,
    color: bool,
    audio: AudioKind,
    action: HookAction,
    expected: OutcomeKind,
    frame_limit: u64,
}

struct Workspace(PathBuf);

impl Workspace {
    fn new(root: &Path, index: usize) -> Self {
        let path = root.join(format!("job-{index:03}"));
        fs::create_dir(&path).expect("create per-job output workspace");
        Self(path)
    }

    fn output(&self) -> PathBuf {
        self.0.join("output.mp4")
    }

    fn no_staging(&self) {
        for item in fs::read_dir(&self.0).expect("read job workspace") {
            let item = item.expect("read job entry");
            assert!(
                !item
                    .file_name()
                    .to_string_lossy()
                    .contains("asciiflow-part"),
                "staging leak at {}",
                item.path().display()
            );
        }
    }
}

fn fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(relative)
}

fn unique_directory() -> PathBuf {
    let configured = std::env::var_os("ASCIIFLOW_NATIVE_RELIABILITY_EVIDENCE_DIR")
        .expect("set ASCIIFLOW_NATIVE_RELIABILITY_EVIDENCE_DIR to a fresh evidence directory");
    let root = PathBuf::from(configured);
    fs::create_dir(&root).expect("evidence directory must not exist already");
    root
}

fn selected_input(case: &Case, pq_audio: &[PathBuf; 3]) -> PathBuf {
    match (case.input, case.audio) {
        (InputKind::Sdr, AudioKind::None) => fixture("media/no-audio.mp4"),
        (InputKind::Sdr, AudioKind::SingleAac) => fixture("media/single.mp4"),
        (InputKind::Sdr, AudioKind::DualAac) => fixture("media/multiple.mp4"),
        (InputKind::Pq, AudioKind::None) => fixture("codecs/hevc-main10-pq-c3-legal-v1.mp4"),
        (InputKind::Pq, AudioKind::SingleAac) => pq_audio[1].clone(),
        (InputKind::Pq, AudioKind::DualAac) => pq_audio[2].clone(),
    }
}

fn pq_audio_inputs(root: &Path) -> [PathBuf; 3] {
    let source = fixture("codecs/hevc-main10-pq-c3-legal-v1.mp4");
    let no_audio = source.clone();
    let single = root.join("pq-single-aac.mp4");
    let dual = root.join("pq-dual-aac.mp4");
    for (audio, destination, streams) in [
        (fixture("media/single.mp4"), &single, 1usize),
        (fixture("media/multiple.mp4"), &dual, 2usize),
    ] {
        let mut command = fixture_ffmpeg();
        command
            .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-i"])
            .arg(&source)
            .args(["-i"])
            .arg(&audio)
            .args(["-map", "0:v:0"]);
        for index in 0..streams {
            command.args(["-map", &format!("1:a:{index}")]);
        }
        command.args(["-c", "copy"]).arg(destination);
        let result = command.output().expect("run pinned FFmpeg fixture remux");
        assert!(
            result.status.success(),
            "PQ audio fixture remux failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    [no_audio, single, dual]
}

fn fixture_ffmpeg() -> Command {
    Command::new(std::env::var_os("ASCIIFLOW_FIXTURE_FFMPEG").unwrap_or_else(|| "ffmpeg".into()))
}

fn font(index: usize) -> PathBuf {
    if index.is_multiple_of(2) {
        PathBuf::from("builtin-8x8")
    } else {
        fixture("fonts/Inconsolata-Regular.ttf")
    }
}

fn case(index: usize, output: OutputKind, action: HookAction) -> Case {
    let audio = match index % 3 {
        0 => AudioKind::None,
        1 => AudioKind::SingleAac,
        _ => AudioKind::DualAac,
    };
    let input = if matches!(output, OutputKind::Sdr8H264) {
        InputKind::Sdr
    } else {
        InputKind::Pq
    };
    let mut case = Case {
        label: String::new(),
        input,
        output,
        font: font(index),
        color: (index / 2).is_multiple_of(2),
        audio,
        action,
        expected: match action {
            HookAction::None => OutcomeKind::Success,
            HookAction::Cancel(_) => OutcomeKind::Cancelled,
            HookAction::Panic { .. } => OutcomeKind::InjectedFailure,
            HookAction::InjectFailure { .. } => OutcomeKind::InjectedFailure,
            HookAction::HoldMuxUntilPipelinesFull => OutcomeKind::Success,
        },
        frame_limit: FRAMES_PER_JOB,
    };
    refresh_label(&mut case);
    case
}

fn refresh_label(case: &mut Case) {
    let font = if case.font.as_path() == Path::new("builtin-8x8") {
        "builtin"
    } else {
        "freetype"
    };
    case.label = format!(
        "{:?}-{font}-color{}-{:?}-frames{}",
        case.output, case.color, case.audio, case.frame_limit
    );
}

fn output_args(output: OutputKind) -> (OutputCodecArg, OutputBitDepthArg, &'static str) {
    match output {
        OutputKind::Sdr8H264 | OutputKind::PqToSdr8H264 => {
            (OutputCodecArg::H264, OutputBitDepthArg::Eight, "sdr")
        }
        OutputKind::Pq10Hevc => (OutputCodecArg::Hevc, OutputBitDepthArg::Ten, "preserve"),
        OutputKind::PqToSdr10Hevc => (OutputCodecArg::Hevc, OutputBitDepthArg::Ten, "sdr"),
    }
}

fn args_for(case: &Case, input: &Path, output: &Path, diagnostic: &Path) -> Args {
    let (codec, depth, dynamic_range) = output_args(case.output);
    let device = std::env::var_os("ASCIIFLOW_VAAPI_DEVICE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/dev/dri/renderD128"));
    let audio = match case.audio {
        AudioKind::None => "none",
        AudioKind::SingleAac | AudioKind::DualAac => "copy",
    };
    let font = case.font.to_string_lossy();
    let argv = [
        "asciiflow-native-reliability".to_owned(),
        input.to_string_lossy().into_owned(),
        output.to_string_lossy().into_owned(),
        "--backend".into(),
        "vulkan".into(),
        "--decode".into(),
        "vaapi".into(),
        "--encode".into(),
        "vaapi".into(),
        "--output-codec".into(),
        format!("{codec:?}").to_ascii_lowercase(),
        "--output-bit-depth".into(),
        match depth {
            OutputBitDepthArg::Eight => "8".into(),
            OutputBitDepthArg::Ten => "10".into(),
        },
        "--output-dynamic-range".into(),
        dynamic_range.into(),
        "--vaapi-vulkan-input-interop".into(),
        "on".into(),
        "--vaapi-vulkan-output-interop".into(),
        "on".into(),
        "--hw-device".into(),
        device.to_string_lossy().into_owned(),
        "--audio".into(),
        audio.into(),
        "--width".into(),
        if matches!(case.input, InputKind::Sdr) {
            "8"
        } else {
            "80"
        }
        .into(),
        "--font".into(),
        font.into_owned(),
        "--color".into(),
        case.color.to_string(),
        "--max-frames".into(),
        case.frame_limit.to_string(),
        "--no-progress".into(),
        "--diagnostic-report".into(),
        diagnostic.to_string_lossy().into_owned(),
    ];
    Args::try_parse_from(argv).expect("valid production CLI test args")
}

fn phase_counts(phases: &[String]) -> Value {
    let mut counts = serde_json::Map::new();
    for phase in phases {
        let count = counts
            .entry(phase.clone())
            .or_insert_with(|| Value::from(0));
        *count = Value::from(count.as_u64().unwrap_or_default() + 1);
    }
    Value::Object(counts)
}

fn proc_count(path: &str) -> Option<usize> {
    fs::read_dir(path).ok().map(|entries| entries.count())
}

fn sha256_file(path: &Path) -> String {
    let output = Command::new("sha256sum")
        .arg(path)
        .output()
        .expect("run sha256sum for evidence identity");
    assert!(
        output.status.success(),
        "sha256sum failed for {}",
        path.display()
    );
    String::from_utf8(output.stdout)
        .expect("sha256sum output is UTF-8")
        .split_whitespace()
        .next()
        .expect("sha256sum emitted a digest")
        .to_owned()
}

fn fixture_ffmpeg_version() -> String {
    let output = fixture_ffmpeg()
        .arg("-version")
        .output()
        .expect("query fixture FFmpeg version");
    assert!(output.status.success(), "fixture FFmpeg -version failed");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or("unknown")
        .to_owned()
}

#[derive(Default)]
struct PipelineGate {
    mux_held: bool,
    decoded_full: bool,
    processed_full: bool,
    release_mux: bool,
    timed_out: bool,
    run_finished: bool,
}

fn controller_release_after_pipeline_saturation(gate: Arc<(Mutex<PipelineGate>, Condvar)>) -> bool {
    let deadline = Instant::now() + Duration::from_secs(45);
    let (lock, changed) = &*gate;
    let mut state = lock.lock().unwrap();
    while !state.run_finished && !(state.mux_held && state.decoded_full && state.processed_full) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            state.timed_out = true;
            state.release_mux = true;
            changed.notify_all();
            return false;
        }
        let (next, timeout) = changed.wait_timeout(state, remaining).unwrap();
        state = next;
        if timeout.timed_out() && !(state.mux_held && state.decoded_full && state.processed_full) {
            state.timed_out = true;
            state.release_mux = true;
            changed.notify_all();
            return false;
        }
    }
    let passed = state.mux_held && state.decoded_full && state.processed_full;
    state.release_mux = true;
    changed.notify_all();
    passed
}

fn active_resources(rows: &[Value]) -> bool {
    rows.last()
        .and_then(|row| row.get("resources"))
        .and_then(Value::as_object)
        .is_some_and(|resources| {
            !resources.is_empty()
                && resources
                    .values()
                    .all(|resource| resource["active_count"].as_u64() == Some(0))
        })
}

#[test]
fn missing_resource_accounting_is_not_a_cleanup_pass() {
    assert!(!active_resources(&[]));
    assert!(!active_resources(&[json!({"resources": {}})]));
    assert!(!active_resources(&[
        json!({"resources": {"binding": {"active_count": 1}}})
    ]));
    assert!(active_resources(&[
        json!({"resources": {"binding": {"active_count": 0}}})
    ]));
}

fn sampled_queue_peak(rows: &[Value], name: &str) -> Option<u64> {
    rows.iter()
        .filter_map(|row| row["queues"][name].as_object())
        .flat_map(|queue| {
            ["peak_depth", "pre_operation_peak_depth"]
                .into_iter()
                .filter_map(|key| queue.get(key).and_then(Value::as_u64))
        })
        .max()
}

fn cleanup_rss_kib(record: &Value) -> Option<u64> {
    record["resource_samples"]
        .as_array()?
        .iter()
        .find(|sample| sample["phase"] == "post-cleanup")?["rss_kib"]
        .as_u64()
}

fn median(values: &mut [u64]) -> u64 {
    values.sort_unstable();
    values[values.len() / 2]
}

fn rss_trend(records: &[Value], jobs_per_sequence: usize) -> Value {
    let sequence_rss: Vec<u64> = records
        .iter()
        .take(MIXED_SEQUENCES * jobs_per_sequence)
        .collect::<Vec<_>>()
        .chunks(jobs_per_sequence)
        .map(|jobs| {
            let mut values: Vec<_> = jobs
                .iter()
                .filter_map(|record| cleanup_rss_kib(record))
                .collect();
            assert_eq!(
                values.len(),
                jobs_per_sequence,
                "missing post-cleanup RSS sample"
            );
            median(&mut values)
        })
        .collect();
    let steady = &sequence_rss[1..]; // Exclude one full mixed sequence as warmup.
    let first = steady[0];
    let last = *steady.last().unwrap();
    let minimum = *steady.iter().min().unwrap();
    let maximum = *steady.iter().max().unwrap();
    let alarm_threshold_kib = 64 * 1024;
    let range_kib = maximum - minimum;
    json!({
        "sampling": "per-job post-cleanup /proc VmRSS, median of four jobs per sequence",
        "warmup_sequences_excluded": 1,
        "sequence_medians_kib": sequence_rss,
        "first_to_last_steady_delta_kib": last as i64 - first as i64,
        "steady_range_kib": range_kib,
        "growth_alarm_threshold_kib": alarm_threshold_kib,
        "large_growth_alarm": range_kib > alarm_threshold_kib,
        "classification": if range_kib > alarm_threshold_kib { "ReviewRequired" } else { "NoLargePostWarmupGrowthObserved" },
        "interpretation": "coarse 20-sequence observation only; allocator and intentional driver caches are possible; not a leak verdict",
    })
}

fn output_oracle(path: &Path, case: &Case) -> Value {
    let (codec, depth, hdr) = match case.output {
        OutputKind::Sdr8H264 | OutputKind::PqToSdr8H264 => (VideoCodec::H264, 8, false),
        OutputKind::Pq10Hevc => (VideoCodec::Hevc, 10, true),
        OutputKind::PqToSdr10Hevc => (VideoCodec::Hevc, 10, false),
    };
    let mut decoder = if hdr {
        Decoder::open_with_pq_preserve(path, DecodeMode::Software, VaapiOptions::default())
    } else {
        Decoder::open(path)
    }
    .expect("open output with independent software decoder");
    let info = decoder.info().clone();
    assert_eq!(info.requirements.codec, codec, "{} codec", case.label);
    assert_eq!(
        info.requirements.bit_depth,
        Some(depth),
        "{} depth",
        case.label
    );
    assert_eq!(
        info.requirements.color_space,
        if hdr {
            ColorSpace::pq_bt2020()
        } else {
            ColorSpace::default()
        },
        "{} color",
        case.label
    );
    let mut frames = 0;
    while decoder.next_frame().expect("decode output frame").is_some() {
        frames += 1;
    }
    decoder.finish().expect("finish independent output decoder");
    assert_eq!(
        frames, case.frame_limit,
        "{} decoded frame count",
        case.label
    );
    let expected_audio = match case.audio {
        AudioKind::None => 0,
        AudioKind::SingleAac => 1,
        AudioKind::DualAac => 2,
    };
    assert_eq!(
        info.audio_streams.len(),
        expected_audio,
        "{} audio count",
        case.label
    );
    json!({
        "codec": format!("{:?}", info.requirements.codec),
        "bit_depth": info.requirements.bit_depth,
        "color_space": format!("{:?}", info.requirements.color_space),
        "decoded_frames": frames,
        "audio_streams": info.audio_streams.len(),
        "oracle": "independent FFmpeg software decode plus media metadata",
    })
}

fn execute_case(
    case: &Case,
    index: usize,
    evidence: &Path,
    pq_audio: &[PathBuf; 3],
    byte_references: &mut HashMap<String, Vec<u8>>,
) -> Value {
    let workspace = Workspace::new(evidence, index);
    let input = selected_input(case, pq_audio);
    let output = workspace.output();
    let diagnostic = workspace.0.join("diagnostic.json");
    let measurement = workspace.0.join("resources.jsonl");
    let cancellation = CancellationToken::new();
    let hook_cancel = cancellation.clone();
    let action = case.action;
    let action_fired = Arc::new(AtomicBool::new(false));
    let action_guard = action_fired.clone();
    let phases = Arc::new(Mutex::new(Vec::<String>::new()));
    let observer_phases = phases.clone();
    let pipeline_gate = Arc::new((Mutex::new(PipelineGate::default()), Condvar::new()));
    let controller = if matches!(action, HookAction::HoldMuxUntilPipelinesFull) {
        let controller_gate = pipeline_gate.clone();
        Some(thread::spawn(move || {
            controller_release_after_pipeline_saturation(controller_gate)
        }))
    } else {
        None
    };
    let before_fds = proc_count("/proc/self/fd");
    let before_threads = proc_count("/proc/self/task");
    let session = Session::start(&measurement).expect("start per-job resource measurement");
    let callback_gate = pipeline_gate.clone();
    let observer = asciiflow_core::reliability_hooks::Observation::install(move |phase| {
        {
            let mut seen = observer_phases.lock().unwrap();
            if seen.len() < 8192 {
                seen.push(phase.to_owned());
            }
        }
        if matches!(action, HookAction::HoldMuxUntilPipelinesFull) {
            let (lock, changed) = &*callback_gate;
            let mut state = lock.lock().unwrap();
            match phase {
                "PipelineDecodedFull" if state.mux_held && !state.release_mux => {
                    state.decoded_full = true
                }
                "PipelineProcessedFull" if state.mux_held && !state.release_mux => {
                    state.processed_full = true
                }
                "MuxPacketWrite" if !state.mux_held => {
                    state.mux_held = true;
                    state.decoded_full = false;
                    state.processed_full = false;
                    changed.notify_all();
                    let deadline = Instant::now() + Duration::from_secs(45);
                    while !state.release_mux {
                        let remaining = deadline.saturating_duration_since(Instant::now());
                        if remaining.is_zero() {
                            state.timed_out = true;
                            state.release_mux = true;
                            changed.notify_all();
                            break;
                        }
                        let (next, timeout) = changed.wait_timeout(state, remaining).unwrap();
                        state = next;
                        if timeout.timed_out() && !state.release_mux {
                            state.timed_out = true;
                            state.release_mux = true;
                            changed.notify_all();
                        }
                    }
                }
                _ => {}
            }
            changed.notify_all();
            if phase == "MuxPacketWrite" {
                action_guard.store(true, Ordering::Release);
            }
            return;
        }
        let target = match action {
            HookAction::None => false,
            HookAction::Cancel(target) => phase == target,
            HookAction::Panic {
                phase: target,
                cancel_on_phase,
            } => {
                if cancel_on_phase && phase == target {
                    hook_cancel.cancel();
                }
                phase == target
            }
            HookAction::InjectFailure {
                phase: target,
                cancel_on_phase,
            } => {
                if cancel_on_phase && phase == target {
                    hook_cancel.cancel();
                }
                phase == target
            }
            HookAction::HoldMuxUntilPipelinesFull => unreachable!(),
        };
        if target && !action_guard.swap(true, Ordering::AcqRel) {
            match action {
                HookAction::Cancel(_) => hook_cancel.cancel(),
                HookAction::Panic {
                    cancel_on_phase: true,
                    ..
                } => panic!("D1A injected worker panic with cancellation at {phase}"),
                HookAction::Panic { .. } => panic!("D1A injected worker panic at {phase}"),
                HookAction::InjectFailure { .. } => {}
                HookAction::HoldMuxUntilPipelinesFull | HookAction::None => unreachable!(),
            }
        }
    });
    let injected_failure = match action {
        HookAction::InjectFailure { phase, .. } => Some(
            asciiflow_core::reliability_hooks::InjectedFailure::at(phase),
        ),
        _ => None,
    };
    let args = args_for(case, &input, &output, &diagnostic);
    let result = super::run(args, cancellation);
    drop(injected_failure);
    {
        let (lock, changed) = &*pipeline_gate;
        let mut state = lock.lock().unwrap();
        state.run_finished = true;
        if !state.mux_held {
            state.release_mux = true;
        }
        changed.notify_all();
    }
    let backpressure_gate_passed = controller.map(|thread| thread.join().unwrap());
    let gate_snapshot = {
        let (lock, _) = &*pipeline_gate;
        let state = lock.lock().unwrap();
        json!({
            "mux_held_before_native_write": state.mux_held,
            "decoded_queue_full_observed": state.decoded_full,
            "processed_queue_full_observed": state.processed_full,
            "watchdog_timed_out": state.timed_out,
            "run_returned": state.run_finished,
        })
    };
    drop(observer);
    session
        .finish()
        .expect("resource accounting and report close");
    let after_fds = proc_count("/proc/self/fd");
    let after_threads = proc_count("/proc/self/task");
    let phase_snapshot = phases.lock().unwrap().clone();
    let report_rows: Vec<Value> = fs::read_to_string(&measurement)
        .expect("read resource report")
        .lines()
        .map(|line| serde_json::from_str(line).expect("parse resource sample"))
        .collect();
    let diagnostic_value: Value =
        serde_json::from_slice(&fs::read(&diagnostic).expect("read production plan diagnostic"))
            .expect("parse production plan diagnostic");
    let output_state = if output.exists() { "present" } else { "absent" };
    workspace.no_staging();
    assert_eq!(
        action_fired.load(Ordering::Acquire),
        !matches!(case.action, HookAction::None),
        "{} hook phase was not observed: {:?}",
        case.label,
        case.action
    );
    let (observed_outcome, error_detail) = match result {
        Ok(()) => (OutcomeKind::Success, None),
        Err(error)
            if error.chain().any(|cause| {
                cause
                    .downcast_ref::<asciiflow_core::Error>()
                    .is_some_and(asciiflow_core::Error::is_cancelled)
            }) =>
        {
            (OutcomeKind::Cancelled, Some(error.to_string()))
        }
        Err(error) => (OutcomeKind::InjectedFailure, Some(format!("{error:#}"))),
    };
    assert_eq!(
        observed_outcome, case.expected,
        "{} unexpected outcome: {error_detail:?}",
        case.label
    );
    if let HookAction::InjectFailure { phase, .. } = case.action {
        assert!(
            error_detail.as_deref().is_some_and(
                |error| error.contains(&format!("qualification injected {phase} error"))
            ),
            "{} did not preserve injected media failure over cancellation: {error_detail:?}",
            case.label
        );
    }
    if let HookAction::Panic { phase, .. } = case.action {
        let expected_root = if phase == "MuxQueueOccupied" {
            "mux worker panicked"
        } else {
            "worker thread panicked"
        };
        assert!(
            error_detail
                .as_deref()
                .is_some_and(|error| error.contains(expected_root)),
            "{} did not preserve {expected_root} as primary cause: {error_detail:?}",
            case.label
        );
    }
    assert_eq!(
        output_state,
        if observed_outcome == OutcomeKind::Success {
            "present"
        } else {
            "absent"
        },
        "{} output state",
        case.label
    );
    if observed_outcome == OutcomeKind::Success {
        let plan = &diagnostic_value["selected_plan"];
        assert_eq!(plan["backend"], "Vulkan", "{} backend", case.label);
        assert_eq!(plan["buffer_capacity"], 3, "{} queue capacity", case.label);
        assert_eq!(plan["decode"], "Hardware", "{} decode", case.label);
        assert_eq!(plan["encode"], "Hardware", "{} encode", case.label);
        assert_eq!(
            plan["hardware_input_interop"], true,
            "{} input interop",
            case.label
        );
        assert_eq!(
            plan["hardware_output_interop"], true,
            "{} output interop",
            case.label
        );
    }
    assert!(
        active_resources(&report_rows),
        "{} outstanding resource tokens: {:?}",
        case.label,
        report_rows.last()
    );
    assert_eq!(before_fds, after_fds, "{} process FD baseline", case.label);
    if matches!(case.action, HookAction::HoldMuxUntilPipelinesFull) {
        assert_eq!(
            backpressure_gate_passed,
            Some(true),
            "mux/pipeline gate did not release after both full signals: {gate_snapshot}"
        );
        assert_eq!(
            sampled_queue_peak(&report_rows, "pipeline_decoded"),
            Some(3),
            "decoded queue capacity-three peak missing"
        );
        assert_eq!(
            sampled_queue_peak(&report_rows, "pipeline_processed"),
            Some(3),
            "processed queue capacity-three peak missing"
        );
    }
    let (output_oracle, byte_exact, output_sha256) = if observed_outcome == OutcomeKind::Success {
        let oracle = output_oracle(&output, case);
        let bytes = fs::read(&output).expect("read oracle-validated output bytes");
        let digest = sha256_file(&output);
        let configuration = case.label.clone();
        let byte_exact = match byte_references.entry(configuration) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(bytes);
                true
            }
            std::collections::hash_map::Entry::Occupied(entry) => *entry.get() == bytes,
        };
        fs::remove_file(&output).expect("remove validated per-job output");
        (Some(oracle), Some(byte_exact), Some(digest))
    } else {
        (None, None, None)
    };
    let record = json!({
        "index": index,
        "case": case.label,
        "input": input,
        "output_path": output,
        "outcome": format!("{observed_outcome:?}"),
        "expected_outcome": format!("{:?}", case.expected),
        "phases": phase_counts(&phase_snapshot),
        "phase_sequence": phase_snapshot,
        "plan": diagnostic_value["selected_plan"],
        "report_phases": report_rows.iter().map(|row| row["phase"].clone()).collect::<Vec<_>>(),
        "post_cleanup_resources_zero": active_resources(&report_rows),
        "resource_samples": report_rows,
        "fd_count_before_session": before_fds,
        "fd_count_after_session": after_fds,
        "thread_count_before": before_threads,
        "thread_count_after": after_threads,
        "worker_thread_count_is_diagnostic_only": true,
        "backpressure_gate_passed": backpressure_gate_passed,
        "backpressure_gate": gate_snapshot,
        "output_state": output_state,
        "error": error_detail,
        "input_sha256": sha256_file(&input),
        "output_sha256": output_sha256,
        "job_entry_returned_after_worker_join_boundary": true,
        "oracle": output_oracle,
        "byte_exact_with_first_output_for_configuration": byte_exact,
        "injected_failure_kind": match case.action {
            HookAction::Panic { phase: "EncoderBusy", .. } => Some("test-hook encoder-worker panic; not a native encoder fault"),
            HookAction::Panic { phase: "MuxQueueOccupied", .. } => Some("test-hook mux-worker panic; not native mux I/O failure"),
            HookAction::InjectFailure { phase: "EncoderBusy", .. } => Some("injected encoder media error at production hook"),
            HookAction::InjectFailure { phase: "MuxPacketWrite", .. } => Some("injected mux packet media error at production hook"),
            _ => None,
        },
    });
    fs::remove_file(measurement).expect("remove consumed per-job reliability report");
    fs::remove_file(diagnostic).expect("remove consumed per-job diagnostic");
    fs::remove_dir(&workspace.0).expect("remove completed per-job workspace");
    record
}

#[test]
#[ignore = "requires ASCIIFLOW_D1A_NATIVE_GPU=1, ASCIIFLOW_C4B_PRODUCTION=1, and qualified Intel VAAPI/Vulkan hardware"]
fn same_process_mixed_gpu_sequences_recover_after_injected_failure_and_cancellation() {
    assert_eq!(
        std::env::var("ASCIIFLOW_D1A_NATIVE_GPU").as_deref(),
        Ok("1"),
        "set ASCIIFLOW_D1A_NATIVE_GPU=1 only on qualified Intel hardware"
    );
    assert_eq!(
        std::env::var("ASCIIFLOW_C4B_PRODUCTION").as_deref(),
        Ok("1"),
        "set ASCIIFLOW_C4B_PRODUCTION=1 only on qualified Intel hardware"
    );
    let evidence = unique_directory();
    let pq_audio = pq_audio_inputs(&evidence);
    let cases_path = [
        OutputKind::Sdr8H264,
        OutputKind::Pq10Hevc,
        OutputKind::PqToSdr8H264,
        OutputKind::PqToSdr10Hevc,
    ];
    let mut records = Vec::with_capacity(MIXED_SEQUENCES * cases_path.len() + 16);
    let mut byte_references = HashMap::<String, Vec<u8>>::new();
    let mut index = 0usize;
    for sequence in 0..MIXED_SEQUENCES {
        for (slot, output) in cases_path.into_iter().enumerate() {
            let mut case = case(sequence + slot, output, HookAction::None);
            // Cover every audio policy within each sequence without a full
            // Cartesian product of codec, font, color, and audio settings.
            case.audio = match (sequence + slot) % 3 {
                0 => AudioKind::None,
                1 => AudioKind::SingleAac,
                _ => AudioKind::DualAac,
            };
            refresh_label(&mut case);
            records.push(execute_case(
                &case,
                index,
                &evidence,
                &pq_audio,
                &mut byte_references,
            ));
            index += 1;
        }
    }

    // Each fault is bracketed by healthy jobs; cancellation is requested at
    // the same checkpoint as the failure to test primary-cause precedence.
    for (output, action) in [
        (OutputKind::Pq10Hevc, HookAction::None),
        (
            OutputKind::PqToSdr10Hevc,
            HookAction::Panic {
                phase: "EncoderBusy",
                cancel_on_phase: true,
            },
        ),
        (OutputKind::Sdr8H264, HookAction::None),
        (OutputKind::Pq10Hevc, HookAction::None),
        (
            OutputKind::PqToSdr10Hevc,
            HookAction::Panic {
                phase: "MuxQueueOccupied",
                cancel_on_phase: true,
            },
        ),
        (OutputKind::Sdr8H264, HookAction::None),
        (OutputKind::Pq10Hevc, HookAction::None),
        (
            OutputKind::PqToSdr10Hevc,
            HookAction::InjectFailure {
                phase: "EncoderBusy",
                cancel_on_phase: true,
            },
        ),
        (OutputKind::Sdr8H264, HookAction::None),
        (OutputKind::Pq10Hevc, HookAction::None),
        (
            OutputKind::PqToSdr10Hevc,
            HookAction::InjectFailure {
                phase: "MuxPacketWrite",
                cancel_on_phase: true,
            },
        ),
        (OutputKind::Sdr8H264, HookAction::None),
        (OutputKind::PqToSdr8H264, HookAction::None),
        (
            OutputKind::PqToSdr10Hevc,
            HookAction::Cancel("VulkanInFlight"),
        ),
        (OutputKind::PqToSdr10Hevc, HookAction::None),
    ] {
        let case = case(index, output, action);
        records.push(execute_case(
            &case,
            index,
            &evidence,
            &pq_audio,
            &mut byte_references,
        ));
        index += 1;
    }

    let mut slow = case(
        index,
        OutputKind::PqToSdr10Hevc,
        HookAction::HoldMuxUntilPipelinesFull,
    );
    slow.audio = AudioKind::None;
    slow.frame_limit = 120;
    refresh_label(&mut slow);
    records.push(execute_case(
        &slow,
        index,
        &evidence,
        &pq_audio,
        &mut byte_references,
    ));

    let document = json!({
        "schema": "asciiflow-d1a-same-process-gpu-v1",
        "scope": "same-process private production CLI job entry; real VAAPI/Vulkan required",
        "sequences": MIXED_SEQUENCES,
        "jobs_in_mixed_sequences": MIXED_SEQUENCES * cases_path.len(),
        "recovery_jobs": records.len() - MIXED_SEQUENCES * cases_path.len() - 1,
        "frame_limit_per_job": FRAMES_PER_JOB,
        "sigint_qualification": "not covered here; process-global signal behavior remains in child-process tests",
        "injected_fault_semantics": "worker/mux test-hook panics and qualification media errors at EncoderBusy/MuxPacketWrite; no native AVIO EIO/ENOSPC claim",
        "slow_pipeline_job": records.last(),
        "rss_trend": rss_trend(&records, cases_path.len()),
        "input_generators": {
            "checked_in_c3_generator_sha256": sha256_file(&fixture("codecs/generate-c3-pq-legal-v1.sh")),
            "checked_in_media_generator_sha256": sha256_file(&fixture("media/generate.sh")),
            "runtime_audio_remux_ffmpeg_version": fixture_ffmpeg_version(),
            "runtime_audio_remux": "ffmpeg -i hevc-main10-pq-c3-legal-v1.mp4 -i single.mp4|multiple.mp4 -map 0:v:0 -map 1:a:0[,1:a:1] -c copy",
        },
        "process_scoped_driver_caches": "thread count recorded, not treated as job-owned leak",
        "byte_exact_configuration_comparisons": records.iter().filter(|record| record["byte_exact_with_first_output_for_configuration"] == true).count(),
        "byte_variances": records.iter().filter(|record| record["byte_exact_with_first_output_for_configuration"] == false).count(),
        "records": records,
    });
    fs::write(
        evidence.join("same-process-gpu.json"),
        serde_json::to_vec_pretty(&document).unwrap(),
    )
    .expect("write same-process evidence");
    assert_eq!(
        document["byte_variances"].as_u64(),
        Some(0),
        "output bytes changed for a repeated configuration; evidence retained"
    );
    assert_eq!(
        document["rss_trend"]["large_growth_alarm"], false,
        "post-warmup RSS range exceeded the 64 MiB review alarm; evidence retained"
    );
}

/// Linux process accounting, sampled only after a complete job/cycle has
/// released its owners. Missing kernel fields remain unavailable, never zero.
fn process_memory() -> Value {
    let mut fields = serde_json::Map::new();
    for (path, prefix) in [
        ("/proc/self/smaps_rollup", "smaps_"),
        ("/proc/self/status", "status_"),
    ] {
        let text = fs::read_to_string(path).expect("read Linux process accounting");
        for line in text.lines() {
            let Some((name, value)) = line.split_once(':') else {
                continue;
            };
            if matches!(
                name,
                "Rss"
                    | "Pss"
                    | "Private_Clean"
                    | "Private_Dirty"
                    | "Anonymous"
                    | "Swap"
                    | "Shared_Clean"
                    | "Shared_Dirty"
                    | "VmRSS"
                    | "RssAnon"
            ) {
                let kib = value
                    .split_whitespace()
                    .next()
                    .expect("accounting value")
                    .parse::<u64>()
                    .expect("accounting KiB integer");
                fields.insert(format!("{prefix}{name}_kib"), json!(kib));
            }
        }
    }
    fields.insert("fd_count".into(), json!(proc_count("/proc/self/fd")));
    fields.insert("thread_count".into(), json!(proc_count("/proc/self/task")));
    Value::Object(fields)
}

#[test]
#[ignore = "default-allocator RSS observation on qualified Intel hardware; explicit evidence directory required"]
fn default_allocator_fixed_mixed_cycles_observation() {
    for key in ["ASCIIFLOW_D1A_NATIVE_GPU", "ASCIIFLOW_C4B_PRODUCTION"] {
        assert_eq!(std::env::var(key).as_deref(), Ok("1"), "{key}");
    }
    // These experiments must not silently become allocator-tuned evidence.
    for (key, _) in std::env::vars_os() {
        let key = key.to_string_lossy();
        assert!(
            !key.starts_with("MALLOC_") && key != "GLIBC_TUNABLES" && key != "LD_PRELOAD",
            "allocator diagnostic environment is forbidden: {key}"
        );
    }
    let cycles = match std::env::var("ASCIIFLOW_NATIVE_RSS_MODE").as_deref() {
        Ok("fresh-process-control") => 1,
        Ok("repeated-cycles") => 50,
        other => panic!("explicit fixed RSS observation mode required: {other:?}"),
    };
    let evidence = unique_directory();
    let pq_audio = pq_audio_inputs(&evidence);
    let mut jobs = BufWriter::new(
        fs::File::create(evidence.join("jobs.jsonl")).expect("create streamed job evidence"),
    );
    let mut cycle_samples = BufWriter::new(
        fs::File::create(evidence.join("cycles.jsonl")).expect("create streamed cycle evidence"),
    );
    // The observer's evidence FDs are process-scoped and held for the whole
    // campaign. Include them in the baseline, not in job-owned FD balance.
    let initial = process_memory();
    // Four fixed configurations, not an ever-growing Cartesian reference set.
    let mut references = HashMap::<String, Vec<u8>>::new();
    let outputs = [
        OutputKind::Sdr8H264,
        OutputKind::Pq10Hevc,
        OutputKind::PqToSdr8H264,
        OutputKind::PqToSdr10Hevc,
    ];
    serde_json::to_writer(&mut cycle_samples, &json!({"cycle": 0, "memory": initial}))
        .expect("stream initial process sample");
    writeln!(cycle_samples).unwrap();
    let mut index = 0;
    for cycle in 1..=cycles {
        for (slot, output) in outputs.into_iter().enumerate() {
            let mut job = case(slot, output, HookAction::None);
            job.audio = match slot % 3 {
                0 => AudioKind::None,
                1 => AudioKind::SingleAac,
                _ => AudioKind::DualAac,
            };
            refresh_label(&mut job);
            let record = execute_case(&job, index, &evidence, &pq_audio, &mut references);
            let resources = record["resource_samples"].as_array().unwrap();
            let final_resources = resources.last().unwrap()["resources"].as_object().unwrap();
            assert!(
                !final_resources.is_empty(),
                "resource observation cannot be vacuous"
            );
            assert_eq!(record["thread_count_after"], initial["thread_count"]);
            assert_eq!(record["fd_count_after_session"], initial["fd_count"]);
            assert_eq!(
                record["byte_exact_with_first_output_for_configuration"],
                true
            );
            serde_json::to_writer(&mut jobs, &record).expect("stream job evidence");
            writeln!(jobs).unwrap();
            index += 1;
        }
        // Fixed, declared recovery schedule; later identical cycles measure
        // whether cancellation/error adds a persistent per-event staircase.
        if cycles > 1 && matches!(cycle, 10 | 20 | 30 | 40) {
            let action = if cycle % 20 == 0 {
                HookAction::InjectFailure {
                    phase: "EncoderBusy",
                    cancel_on_phase: true,
                }
            } else {
                HookAction::Cancel("VulkanInFlight")
            };
            let job = case(0, OutputKind::PqToSdr10Hevc, action);
            let record = execute_case(&job, index, &evidence, &pq_audio, &mut references);
            assert_eq!(record["thread_count_after"], initial["thread_count"]);
            assert_eq!(record["fd_count_after_session"], initial["fd_count"]);
            serde_json::to_writer(&mut jobs, &record).expect("stream recovery evidence");
            writeln!(jobs).unwrap();
            index += 1;
        }
        jobs.flush().expect("flush cycle evidence");
        serde_json::to_writer(
            &mut cycle_samples,
            &json!({"cycle": cycle, "memory": process_memory()}),
        )
        .expect("stream process sample after cycle");
        writeln!(cycle_samples).unwrap();
        cycle_samples.flush().expect("flush cycle accounting");
    }
    // Assemble the convenient summary only after the last measured cycle.
    // Measurement itself retains neither jobs nor an expanding sample vector.
    cycle_samples.flush().unwrap();
    let samples: Vec<Value> = fs::read_to_string(evidence.join("cycles.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let document = json!({
        "schema": "asciiflow-default-rss-cycles-v1",
        "qualification": "ObservationOnlyPendingTrendAndFreshProcessReview",
        "cycles": cycles,
        "jobs": index,
        "fixed_jobs_per_cycle": 4,
        "cycles_after_final_failure": if cycles > 1 { 10 } else { 0 },
        "samples": samples,
        "reference_configurations": references.len(),
        "retained_reference_bytes": references.values().map(Vec::len).sum::<usize>(),
        "records_policy": "Jobs and cycle samples streamed and dropped; only four fixed first-output references retained during measurement; summary assembled after last sample",
        "queue_cleanup_scope": "Per-job run returned after all channel owners and worker joins; resource balance/staging checked. Last-boundary queue depths in reports are not cleanup-time depths.",
        "historical_64mib_alarm": "Preserved in the original driver and historical receipt, not silently changed; this test collects evidence and does not itself seal RSS",
    });
    fs::write(
        evidence.join("rss-cycles.json"),
        serde_json::to_vec_pretty(&document).unwrap(),
    )
    .expect("write RSS cycle evidence");
}
