//! Bounded, opt-in CLI transaction and process-lifetime qualification.
//! These child-process checks do not establish in-process GPU resource cleanup.
#![cfg(target_os = "linux")]

#[allow(dead_code)]
mod audio_support;

use audio_support::{Process, Workspace, command, fixture, success};
use serde_json::{Value, json};
use std::{fs, os::unix::process::CommandExt, path::Path, process::Command, time::Instant};

fn conversion(input: &Path, output: &Path) -> Command {
    let mut cmd = command(input, output, "none");
    cmd.args(["--max-frames", "3"]);
    cmd
}

fn run(cmd: &mut Command, kind: &str, records: &mut Vec<Value>) -> std::process::Output {
    let started = Instant::now();
    let result = Process::start(cmd).finish();
    records.push(json!({
        "kind": kind,
        "success": result.status.success(),
        "exit_code": result.status.code(),
        "elapsed_ms": started.elapsed().as_millis(),
        "stderr": String::from_utf8_lossy(&result.stderr),
    }));
    result
}

#[cfg(feature = "reliability-measurement")]
#[test]
fn measurement_preserves_output_identity_and_retains_failure_cleanup() {
    let ws = Workspace::new();
    let input = fixture("no-audio.mp4");
    let plain = ws.0.join("plain.mp4");
    let measured = ws.0.join("measured.mp4");
    let report = ws.0.join("resources.jsonl");
    success(&Process::start(&mut conversion(&input, &plain)).finish());
    let mut cmd = conversion(&input, &measured);
    cmd.env("ASCIIFLOW_RELIABILITY_REPORT", &report);
    success(&Process::start(&mut cmd).finish());
    assert_eq!(fs::read(&plain).unwrap(), fs::read(&measured).unwrap());
    let rows: Vec<Value> = fs::read_to_string(&report)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.first().unwrap()["phase"], "initial");
    assert_eq!(rows.last().unwrap()["phase"], "post-cleanup");
    assert_eq!(rows.last().unwrap()["frames_processed"], 3);
    assert_eq!(rows.last().unwrap()["packets_processed"], 3);
    assert_eq!(
        rows.first().unwrap()["fd_count"],
        rows.last().unwrap()["fd_count"]
    );

    let aliased = ws.0.join("aliased.mp4");
    let mut cmd = conversion(&input, &aliased);
    cmd.env("ASCIIFLOW_RELIABILITY_REPORT", ws.0.join("./aliased.mp4"));
    let result = Process::start(&mut cmd).finish();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("must not alias"));
    assert!(!aliased.exists());

    let destination = ws.0.join("preserved.mp4");
    fs::write(&destination, b"sentinel").unwrap();
    let failed_report = ws.0.join("failed.jsonl");
    let mut cmd = conversion(&ws.0.join("missing-input.mp4"), &destination);
    cmd.env("ASCIIFLOW_RELIABILITY_REPORT", &failed_report);
    let result = Process::start(&mut cmd).finish();
    assert!(!result.status.success());
    // Missing files are rejected before FFmpeg opens native input; telemetry
    // must still record cleanup while preserving the original OS diagnostic.
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("failed to inspect input media"), "{stderr}");
    assert!(stderr.contains("No such file or directory"), "{stderr}");
    assert_eq!(fs::read(destination).unwrap(), b"sentinel");
    let rows: Vec<Value> = fs::read_to_string(failed_report)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.last().unwrap()["phase"], "post-cleanup");
    assert_eq!(rows.last().unwrap()["frames_processed"], 0);
    assert_eq!(
        rows.first().unwrap()["fd_count"],
        rows.last().unwrap()["fd_count"]
    );
    ws.assert_no_staging();
}

#[test]
#[ignore = "release-only: set ASCIIFLOW_D1A_EVIDENCE_DIR to a new directory; runs 150 bounded CLI jobs"]
fn repeated_jobs_and_output_failures_preserve_transactions_and_recover_exactly() {
    assert!(
        !std::hint::black_box(cfg!(debug_assertions)),
        "run this qualification with --release"
    );
    let evidence = std::env::var_os("ASCIIFLOW_D1A_EVIDENCE_DIR")
        .map(std::path::PathBuf::from)
        .expect("ASCIIFLOW_D1A_EVIDENCE_DIR must name a new evidence directory");
    fs::create_dir(&evidence).expect("evidence directory must not already exist");
    let input = fixture("no-audio.mp4");
    let mut records = Vec::with_capacity(150);
    let reference = Workspace::new();
    success(&run(
        &mut conversion(&input, &reference.output()),
        "baseline",
        &mut records,
    ));
    let expected = fs::read(reference.output()).unwrap();
    assert!(!expected.is_empty());
    reference.assert_no_staging();

    for index in 0..50 {
        let ws = Workspace::new();
        let sentinel = b"D1A-existing-destination-sentinel";
        let (mut cmd, kind, preserved) = match index % 3 {
            0 => {
                // An existing file as a parent prevents staging reservation.
                let blocker = ws.0.join("blocked-parent");
                fs::write(&blocker, sentinel).unwrap();
                (
                    conversion(&input, &blocker.join("output.mp4")),
                    "staging-parent",
                    blocker,
                )
            }
            1 => {
                // A nonempty directory rejects the final atomic rename after
                // the encoder has drained and finalized successfully.
                fs::create_dir(ws.output()).unwrap();
                let preserved = ws.output().join("sentinel");
                fs::write(&preserved, sentinel).unwrap();
                (conversion(&input, &ws.output()), "commit-rename", preserved)
            }
            _ => {
                fs::write(ws.output(), sentinel).unwrap();
                let mut cmd = conversion(&input, &ws.output());
                // CPU/software-only invocation: no GPU cache or memfd can
                // consume this limit before the intended native output write.
                unsafe {
                    cmd.pre_exec(|| {
                        libc::signal(libc::SIGXFSZ, libc::SIG_IGN);
                        let limit = libc::rlimit {
                            rlim_cur: 1,
                            rlim_max: 1,
                        };
                        if libc::setrlimit(libc::RLIMIT_FSIZE, &limit) != 0 {
                            return Err(std::io::Error::last_os_error());
                        }
                        Ok(())
                    });
                }
                (cmd, "native-write-efbig", ws.output())
            }
        };
        let failed = run(&mut cmd, kind, &mut records);
        assert!(!failed.status.success(), "{kind} unexpectedly succeeded");
        let diagnostic = String::from_utf8_lossy(&failed.stderr);
        let expected_diagnostic = match kind {
            "staging-parent" => "failed to create output directory",
            "commit-rename" => "failed to atomically commit output",
            "native-write-efbig" => "File too large",
            _ => unreachable!(),
        };
        assert!(
            diagnostic.contains(expected_diagnostic),
            "{kind}: {diagnostic}"
        );
        assert!(!diagnostic.contains("channel disconnected"), "{diagnostic}");
        assert_eq!(fs::read(&preserved).unwrap(), sentinel);
        ws.assert_no_staging();

        let recovered = ws.0.join("recovered.mp4");
        success(&run(
            &mut conversion(&input, &recovered),
            "recovery",
            &mut records,
        ));
        assert_eq!(fs::read(recovered).unwrap(), expected, "recovery {index}");
        assert_eq!(fs::read(preserved).unwrap(), sentinel);
        ws.assert_no_staging();
    }
    // Baseline + 50 immediate recoveries + 49 additional jobs = 100 successes.
    for index in 0..49 {
        let ws = Workspace::new();
        success(&run(
            &mut conversion(&input, &ws.output()),
            "repeat",
            &mut records,
        ));
        assert_eq!(fs::read(ws.output()).unwrap(), expected, "repeat {index}");
        ws.assert_no_staging();
    }
    assert_eq!(
        records
            .iter()
            .filter(|record| record["success"] == true)
            .count(),
        100
    );
    assert_eq!(
        records
            .iter()
            .filter(|record| record["success"] == false)
            .count(),
        50
    );
    fs::write(
        evidence.join("repeated-jobs.json"),
        serde_json::to_vec_pretty(&json!({
            "schema": "asciiflow-d1a-repeated-jobs-v1",
            "scope": "CPU software CLI child processes; not GPU cleanup evidence",
            "input": input,
            "max_frames": 3,
            "reference_bytes": expected.len(),
            "successes": 100,
            "expected_failures": 50,
            "byte_exact_recoveries": 50,
            "staging_leaks": 0,
            "records": records,
        }))
        .unwrap(),
    )
    .unwrap();
}
