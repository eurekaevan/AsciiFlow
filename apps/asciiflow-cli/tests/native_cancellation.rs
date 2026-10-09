#![cfg(all(target_os = "linux", feature = "native-reliability"))]

use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::{
        fd::AsRawFd,
        unix::{
            fs::PermissionsExt,
            net::{UnixListener, UnixStream},
        },
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Duration,
};

fn receive(listener: &UnixListener) -> (UnixStream, String) {
    let mut poll = libc::pollfd {
        fd: listener.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    assert_eq!(
        unsafe { libc::poll(&mut poll, 1, 30_000) },
        1,
        "native gate not reached"
    );
    let (stream, _) = listener.accept().unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    (reader.into_inner(), line.trim().to_owned())
}

fn checked_resources(directory: &Path) -> Vec<serde_json::Value> {
    let stderr = fs::read_to_string(directory.join("stderr.log")).unwrap();
    for diagnostic in ["VUID-", "Validation Error", "SYNC-HAZARD"] {
        assert!(!stderr.contains(diagnostic), "{stderr}");
    }
    let rows: Vec<serde_json::Value> = fs::read_to_string(directory.join("resources.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let last = rows.last().unwrap();
    assert_eq!(last["phase"], "post-cleanup");
    assert_eq!(rows[0]["fd_count"], last["fd_count"]);
    assert!(last["accounting_errors"].as_array().unwrap().is_empty());
    assert!(
        last["resources"]["vulkan_buffer_bindings"]["peak_count"]
            .as_u64()
            .unwrap_or(0)
            > 0,
        "missing positive Vulkan binding observations"
    );
    for value in last["resources"].as_object().unwrap().values() {
        assert_eq!(value["active_count"], 0);
    }
    rows
}

struct Running {
    child: Child,
    directory: PathBuf,
    output: PathBuf,
    listener: UnixListener,
}
impl Running {
    fn start(root: &Path, phase: &str, index: usize) -> Self {
        Self::start_fault(root, phase, index, None)
    }
    fn start_fault(root: &Path, phase: &str, index: usize, fault: Option<&str>) -> Self {
        let directory = root.join(format!("{index:02}-{phase}"));
        fs::create_dir(&directory).unwrap();
        let socket = directory.join("gate.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let output = directory.join("output.mp4");
        fs::write(&output, b"existing destination sentinel").unwrap();
        let stderr = fs::File::create(directory.join("stderr.log")).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_asciiflow"));
        command
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/media/multiple.mp4"),
            )
            .arg(&output)
            .args([
                "--backend",
                "vulkan",
                "--decode",
                "vaapi",
                "--encode",
                "vaapi",
                "--input-interop",
                "on",
                "--output-interop",
                "on",
                "--audio",
                "copy",
                "--width",
                "80",
                "--max-frames",
                "0",
                "--no-progress",
            ])
            .env("ASCIIFLOW_RELIABILITY_PHASE", phase)
            .env("ASCIIFLOW_RELIABILITY_SOCKET", &socket)
            .env(
                "ASCIIFLOW_RELIABILITY_REPORT",
                directory.join("resources.jsonl"),
            )
            .arg("--diagnostic-report")
            .arg(directory.join("diagnostic.json"))
            .stdout(Stdio::null())
            .stderr(stderr);
        if let Some(fault) = fault {
            command.env("ASCIIFLOW_RELIABILITY_FAILURE", fault);
        }
        let child = command.spawn().unwrap();
        Self {
            child,
            directory,
            output,
            listener,
        }
    }
    fn finish(&mut self, expected: i32, committed: bool) -> serde_json::Value {
        // Qualification child has a 30-second gate watchdog; external runner
        // additionally bounds native hangs outside the synchronization gate.
        let status = self.child.wait().unwrap();
        let stderr = fs::read_to_string(self.directory.join("stderr.log")).unwrap();
        assert_eq!(status.code(), Some(expected), "{stderr}");
        checked_resources(&self.directory);
        assert_eq!(
            fs::read(&self.output).unwrap() != b"existing destination sentinel",
            committed
        );
        assert!(
            !fs::read_dir(&self.directory).unwrap().any(|p| p
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains("asciiflow-part")),
            "staging leaked"
        );
        let rows: Vec<serde_json::Value> =
            fs::read_to_string(self.directory.join("resources.jsonl"))
                .unwrap()
                .lines()
                .map(|l| serde_json::from_str(l).unwrap())
                .collect();
        let last = rows.last().unwrap();
        assert_eq!(last["phase"], "post-cleanup");
        assert_eq!(rows[0]["fd_count"], last["fd_count"], "child FD baseline");
        assert!(last["accounting_errors"].as_array().unwrap().is_empty());
        for resource in last["resources"].as_object().unwrap().values() {
            assert_eq!(resource["active_count"], 0);
        }
        if committed {
            assert!(
                Command::new("ffmpeg")
                    .args(["-v", "error", "-xerror", "-i"])
                    .arg(&self.output)
                    .args(["-f", "null", "-"])
                    .output()
                    .unwrap()
                    .status
                    .success()
            );
        }
        serde_json::json!({"directory": self.directory, "exit": expected, "committed": committed, "fd_initial": rows[0]["fd_count"], "fd_final": last["fd_count"], "resources_final": last["resources"], "staging_leaks": 0})
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
#[ignore = "real GPU + synchronized SIGINT; new ASCIIFLOW_NATIVE_CANCEL_DIR; run alone under external watchdog"]
fn native_sigint_phase_and_commit_matrix() {
    let root = PathBuf::from(
        std::env::var_os("ASCIIFLOW_NATIVE_CANCEL_DIR").expect("new evidence directory required"),
    );
    fs::create_dir(&root).unwrap();
    let phases = [
        "DecoderActive",
        "VulkanInFlight",
        "EncoderBusy",
        "MuxQueueOccupied",
        "AudioAhead",
        "VideoAhead",
        "NearEof",
        "BeforeMuxFinalization",
        "AfterMuxFinalization",
        "DuringWorkerShutdown",
        "BeforeCommit",
        "AfterCommit",
    ];
    let mut records = Vec::new();
    for repeat in 0..3 {
        for (index, phase) in phases.iter().enumerate() {
            let mut job = Running::start(&root, phase, repeat * phases.len() + index);
            let (mut gate, observed) = receive(&job.listener);
            assert_eq!(&observed, phase);
            assert_eq!(
                unsafe { libc::kill(job.child.id() as i32, libc::SIGINT) },
                0
            );
            let (_, cancelled) = receive(&job.listener);
            assert_eq!(
                cancelled, "CancellationRequested",
                "handler must acknowledge before releasing phase"
            );
            gate.write_all(&[1]).unwrap();
            let committed = *phase == "AfterCommit";
            let mut record = job.finish(if committed { 0 } else { 130 }, committed);
            record["phase"] = (*phase).into();
            record["order"] =
                serde_json::json!([*phase, "SIGINT", "CancellationRequested", "gate released"]);
            records.push(record);
        }
    }
    fs::write(root.join("cancellation.json"), serde_json::to_vec_pretty(&serde_json::json!({"classification": "NativePass", "runs": records.len(), "records": records})).unwrap()).unwrap();
}

#[test]
#[ignore = "real non-root rename denial with completed native GPU output; new ASCIIFLOW_NATIVE_PERMISSION_DIR"]
fn native_commit_permission_denial_preserves_root_and_reports_cleanup() {
    assert_ne!(
        unsafe { libc::geteuid() },
        0,
        "must test real non-root permissions"
    );
    let root = PathBuf::from(std::env::var_os("ASCIIFLOW_NATIVE_PERMISSION_DIR").unwrap());
    fs::create_dir(&root).unwrap();
    let mut job = Running::start(&root, "BeforeCommit", 0);
    let (mut gate, phase) = receive(&job.listener);
    assert_eq!(phase, "BeforeCommit");
    fs::set_permissions(&job.directory, fs::Permissions::from_mode(0o555)).unwrap();
    gate.write_all(&[1]).unwrap();
    let status = job.child.wait().unwrap();
    // Restore parent permissions before inspecting or removing retained staging.
    fs::set_permissions(&job.directory, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(status.code(), Some(1));
    checked_resources(&job.directory);
    assert_eq!(
        fs::read(&job.output).unwrap(),
        b"existing destination sentinel"
    );
    let diagnostic = fs::read_to_string(job.directory.join("stderr.log")).unwrap();
    assert!(
        diagnostic.contains("failed to atomically commit output"),
        "{diagnostic}"
    );
    assert!(
        diagnostic.contains("staging cleanup failed"),
        "secondary failure must be visible: {diagnostic}"
    );
    assert!(
        diagnostic.contains("diagnostic report failed"),
        "report failure must not replace commit root: {diagnostic}"
    );
    let retained: Vec<_> = fs::read_dir(&job.directory)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .contains("asciiflow-part")
        })
        .collect();
    assert_eq!(
        retained.len(),
        1,
        "failed cleanup must not be falsely reported as removed"
    );
    fs::remove_file(&retained[0]).unwrap();
    fs::write(root.join("permission.json"), serde_json::to_vec_pretty(&serde_json::json!({"classification":"NativePass", "uid":unsafe{libc::geteuid()}, "root":"commit permission denied", "secondary":"staging removal permission denied", "destination_preserved":true, "staging_retained_until_permissions_restored":true, "staging_recovered":true})).unwrap()).unwrap();
}

#[test]
#[ignore = "real GPU encoder/mux errors plus actual non-root staging removal failure; new ASCIIFLOW_NATIVE_COMPOSITE_DIR"]
fn native_encoder_and_mux_errors_survive_cleanup_failure() {
    assert_ne!(unsafe { libc::geteuid() }, 0);
    let root = PathBuf::from(std::env::var_os("ASCIIFLOW_NATIVE_COMPOSITE_DIR").unwrap());
    fs::create_dir(&root).unwrap();
    let mut records = Vec::new();
    for (index, phase) in ["EncoderBusy", "MuxPacketWrite"].iter().enumerate() {
        let mut job = Running::start_fault(&root, phase, index, Some(phase));
        let (mut gate, observed) = receive(&job.listener);
        assert_eq!(&observed, phase);
        fs::set_permissions(&job.directory, fs::Permissions::from_mode(0o555)).unwrap();
        gate.write_all(&[1]).unwrap();
        let status = job.child.wait().unwrap();
        fs::set_permissions(&job.directory, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(status.code(), Some(1));
        checked_resources(&job.directory);
        assert_eq!(
            fs::read(&job.output).unwrap(),
            b"existing destination sentinel"
        );
        let diagnostic = fs::read_to_string(job.directory.join("stderr.log")).unwrap();
        assert!(
            diagnostic.contains(&format!("qualification injected {phase} error")),
            "{diagnostic}"
        );
        assert!(
            diagnostic.contains("staging cleanup failed"),
            "{diagnostic}"
        );
        let rows: Vec<serde_json::Value> =
            fs::read_to_string(job.directory.join("resources.jsonl"))
                .unwrap()
                .lines()
                .map(|l| serde_json::from_str(l).unwrap())
                .collect();
        assert_eq!(rows[0]["fd_count"], rows.last().unwrap()["fd_count"]);
        for value in rows.last().unwrap()["resources"]
            .as_object()
            .unwrap()
            .values()
        {
            assert_eq!(value["active_count"], 0);
        }
        let retained: Vec<_> = fs::read_dir(&job.directory)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .contains("asciiflow-part")
            })
            .collect();
        assert_eq!(retained.len(), 1);
        fs::remove_file(&retained[0]).unwrap();
        records.push(serde_json::json!({"phase":phase,"classification":"SimulatedPass", "cleanup":"Native permission denial", "root_retained":true,"sentinel_preserved":true,"fd_restored":true,"resources_released":true,"staging_recovered":true}));
    }
    fs::write(
        root.join("composite.json"),
        serde_json::to_vec_pretty(&records).unwrap(),
    )
    .unwrap();
}
