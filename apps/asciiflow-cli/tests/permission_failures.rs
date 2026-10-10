//! Real Unix permission-denial checks. These must run as an unprivileged user;
//! root silently bypasses the mode-bit failures this test is intended to prove.
#![cfg(target_os = "linux")]

#[allow(dead_code)]
mod audio_support;

use audio_support::{Process, command, fixture};
use std::{
    ffi::CString,
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
// Descriptor counts are process-wide, so isolate each test's setup and cleanup.
static PERMISSION_TEST: Mutex<()> = Mutex::new(());

struct Workspace(PathBuf);

impl Workspace {
    fn new() -> Self {
        assert_ne!(
            unsafe { libc::geteuid() },
            0,
            "permission qualification must run as a non-root user"
        );
        let path = std::env::temp_dir().join(format!(
            "asciiflow-permission-test-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock before Unix epoch")
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&path).expect("create isolated permission-test workspace");
        Self(path)
    }

    fn assert_no_staging(&self) {
        for entry in fs::read_dir(&self.0).expect("read test workspace") {
            let entry = entry.expect("read workspace entry");
            assert!(
                !entry
                    .file_name()
                    .to_string_lossy()
                    .contains("asciiflow-part"),
                "staging file leaked: {}",
                entry.path().display()
            );
        }
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        // A failed assertion must not leave a mode-locked temporary tree.
        restore_writable_tree(&self.0);
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn restore_writable_tree(path: &Path) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            restore_writable_tree(&entry.path());
            let _ = fs::set_permissions(entry.path(), fs::Permissions::from_mode(0o700));
        }
    }
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
}

fn denied_directory(path: &Path) {
    fs::set_permissions(path, fs::Permissions::from_mode(0o555))
        .expect("make directory genuinely non-writable");
}

fn conversion(input: &Path, output: &Path) -> Command {
    let mut cmd = command(input, output, "none");
    cmd.args(["--max-frames", "3"]);
    cmd
}

fn fd_count() -> usize {
    fs::read_dir("/proc/self/fd")
        .expect("Linux procfs required for descriptor assertion")
        .count()
        .saturating_sub(1) // Exclude this read_dir iterator's own descriptor.
}

#[test]
fn existing_output_directory_permission_denies_staging_creation() {
    let _serial = PERMISSION_TEST.lock().unwrap();
    let ws = Workspace::new();
    let input = fixture("no-audio.mp4");
    let output_dir = ws.0.join("output");
    fs::create_dir(&output_dir).unwrap();
    let output = output_dir.join("result.mp4");
    fs::write(&output, b"permission-test-sentinel").unwrap();
    denied_directory(&output_dir);

    let before_fds = fd_count();
    let result = Process::start(&mut conversion(&input, &output)).finish();
    let after_fds = fd_count();
    assert!(
        !result.status.success(),
        "read-only output directory succeeded"
    );
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("failed to reserve temporary output"),
        "expected staging create_new permission failure, got: {stderr}"
    );
    assert_eq!(fs::read(&output).unwrap(), b"permission-test-sentinel");
    assert_eq!(before_fds, after_fds, "parent process descriptor leak");
    fs::set_permissions(&output_dir, fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(
        fs::read_dir(&output_dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>(),
        [output.file_name().unwrap()]
    );
    ws.assert_no_staging();
}

#[test]
fn read_only_parent_denies_creation_of_target_directory() {
    let _serial = PERMISSION_TEST.lock().unwrap();
    let ws = Workspace::new();
    let input = fixture("no-audio.mp4");
    let blocked_parent = ws.0.join("blocked");
    fs::create_dir(&blocked_parent).unwrap();
    denied_directory(&blocked_parent);
    let output = blocked_parent.join("not-created").join("result.mp4");

    let before_fds = fd_count();
    let result = Process::start(&mut conversion(&input, &output)).finish();
    let after_fds = fd_count();
    assert!(
        !result.status.success(),
        "read-only parent allowed target creation"
    );
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("failed to create output directory"),
        "expected directory creation permission failure, got: {stderr}"
    );
    assert!(!output.exists());
    assert!(!blocked_parent.join("not-created").exists());
    assert_eq!(before_fds, after_fds, "parent process descriptor leak");
    fs::set_permissions(&blocked_parent, fs::Permissions::from_mode(0o700)).unwrap();
    ws.assert_no_staging();
}

#[test]
fn non_regular_inputs_are_rejected_before_blocking_probe() {
    let _serial = PERMISSION_TEST.lock().unwrap();
    let ws = Workspace::new();
    let fifo = ws.0.join("input.mp4");
    let fifo_name = CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo_name.as_ptr(), 0o600) }, 0);
    let directory = ws.0.join("directory.mp4");
    fs::create_dir(&directory).unwrap();
    let output = ws.0.join("result.mp4");
    fs::write(&output, b"existing destination sentinel").unwrap();

    // Process::finish has a bounded watchdog and kills a hung child on failure.
    // Cover both diagnostic entry points, which also open native input media.
    for input in [&fifo, &directory] {
        for diagnostic in [None, Some("--capabilities"), Some("--explain-plan")] {
            let mut cmd = conversion(input, &output);
            if let Some(option) = diagnostic {
                cmd.arg(option);
            }
            let result = Process::start(&mut cmd).finish();
            let stderr = String::from_utf8_lossy(&result.stderr);
            assert!(!result.status.success(), "non-regular input accepted");
            assert!(
                stderr.contains("input media must be a regular file"),
                "{stderr}"
            );
            assert_eq!(fs::read(&output).unwrap(), b"existing destination sentinel");
            ws.assert_no_staging();
        }
    }
}

#[test]
fn regular_file_symlink_remains_a_supported_input() {
    let _serial = PERMISSION_TEST.lock().unwrap();
    let ws = Workspace::new();
    let input = ws.0.join("alias.mp4");
    symlink(fixture("no-audio.mp4"), &input).unwrap();
    let output = ws.0.join("result.mp4");
    let result = Process::start(&mut conversion(&input, &output)).finish();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(fs::metadata(&output).unwrap().len() > 0);
    ws.assert_no_staging();
}

#[test]
fn missing_input_preserves_io_diagnostic_and_destination() {
    let _serial = PERMISSION_TEST.lock().unwrap();
    let ws = Workspace::new();
    let input = ws.0.join("missing.mp4");
    let output = ws.0.join("result.mp4");
    fs::write(&output, b"existing destination sentinel").unwrap();
    let report = ws.0.join("report.json");
    let mut cmd = conversion(&input, &output);
    cmd.arg("--diagnostic-report").arg(&report);
    let result = Process::start(&mut cmd).finish();
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(!result.status.success());
    assert!(stderr.contains("failed to inspect input media"), "{stderr}");
    assert!(stderr.contains("No such file or directory"), "{stderr}");
    let diagnostic: serde_json::Value = serde_json::from_slice(&fs::read(report).unwrap()).unwrap();
    assert_eq!(diagnostic["failure"]["stage"], "InputProbe");
    assert_eq!(diagnostic["failure"]["category"], "Media");
    assert_eq!(fs::read(&output).unwrap(), b"existing destination sentinel");
    ws.assert_no_staging();
}
