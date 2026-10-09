//! Opt-in ownership observations, not a native-driver memory census.
//!
//! Compiled out of default builds. One conversion session per process is
//! supported. Counters never influence media decisions. Records are streamed
//! under the counter lock, without an asynchronous or unbounded log queue.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{self, Write},
    path::Path,
    sync::{Mutex, OnceLock},
};

#[derive(Default)]
struct Resource {
    active_count: u64,
    active_bytes: u64,
    peak_count: u64,
    peak_bytes: u64,
    bytes_known: bool,
}

struct State {
    generation: u64,
    output: File,
    frames: u64,
    packets: u64,
    packet_bytes: u64,
    resources: BTreeMap<&'static str, Resource>,
    queues: BTreeMap<&'static str, Value>,
    mux: Value,
    errors: Vec<String>,
    write_error: Option<io::Error>,
}

#[derive(Default)]
struct Registry {
    generation: u64,
    state: Option<State>,
}

fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(Mutex::default)
}

fn with_state(f: impl FnOnce(&mut State)) {
    // No code holding this lock calls media code or user callbacks. Recovering
    // poisoned state retains partial evidence instead of panicking in Drop.
    let mut registry = registry().lock().unwrap_or_else(|error| error.into_inner());
    if let Some(state) = registry.state.as_mut() {
        f(state);
    }
}

impl State {
    fn error(&mut self, detail: String) {
        if self.errors.len() < 32 {
            self.errors.push(detail);
        }
    }

    fn sample(&mut self, phase: &str) {
        if self.write_error.is_some() {
            return;
        }
        let resources: BTreeMap<_, _> = self
            .resources
            .iter()
            .map(|(name, value)| {
                (
                    *name,
                    json!({
                        "active_count": value.active_count,
                        "active_bytes": value.bytes_known.then_some(value.active_bytes),
                        "peak_count": value.peak_count,
                        "peak_bytes": value.bytes_known.then_some(value.peak_bytes),
                    }),
                )
            })
            .collect();
        let row = json!({
            "schema_version": 1,
            "phase": phase,
            "frames_processed": self.frames,
            "packets_processed": self.packets,
            "packet_bytes_written": self.packet_bytes,
            "rss_kib": rss_kib(),
            "fd_count": fd_count(),
            "resources": resources,
            "queues": self.queues,
            "mux": self.mux,
            "unavailable": ["exact_queue_high_water", "queue_bytes", "ffmpeg_internal_mux_bytes", "driver_gpu_bytes", "driver_vaapi_surface_count"],
            "accounting_errors": self.errors,
        });
        let result = serde_json::to_writer(&mut self.output, &row)
            .map_err(io::Error::other)
            .and_then(|()| self.output.write_all(b"\n"))
            .and_then(|()| self.output.flush());
        if let Err(error) = result {
            self.write_error = Some(error);
        }
    }
}

fn rss_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status.lines().find_map(|line| {
        line.strip_prefix("VmRSS:")?
            .split_whitespace()
            .next()?
            .parse()
            .ok()
    })
}

fn fd_count() -> Option<u64> {
    // read_dir itself owns one FD during enumeration; exclude that observer.
    let entries = std::fs::read_dir("/proc/self/fd").ok()?;
    let mut count = 0u64;
    for entry in entries {
        entry.ok()?;
        count += 1;
    }
    count.checked_sub(1)
}

/// A create-new JSONL report, kept alive across construction and native cleanup.
pub struct Session {
    generation: u64,
}

impl Session {
    pub fn start(path: impl AsRef<Path>) -> io::Result<Self> {
        let mut registry = registry().lock().unwrap_or_else(|error| error.into_inner());
        if registry.state.is_some() {
            return Err(io::Error::other("a reliability session is already active"));
        }
        let output = OpenOptions::new().write(true).create_new(true).open(path)?;
        registry.generation += 1;
        let generation = registry.generation;
        let mut state = State {
            generation,
            output,
            frames: 0,
            packets: 0,
            packet_bytes: 0,
            resources: BTreeMap::new(),
            queues: BTreeMap::new(),
            mux: Value::Null,
            errors: Vec::new(),
            write_error: None,
        };
        state.sample("initial");
        registry.state = Some(state);
        Ok(Self { generation })
    }

    pub fn from_environment(excluded: &[&Path]) -> io::Result<Option<Self>> {
        let Some(path) = std::env::var_os("ASCIIFLOW_RELIABILITY_REPORT") else {
            return Ok(None);
        };
        Self::start_excluding(Path::new(&path), excluded).map(Some)
    }

    /// Reject output aliases before creating a report that atomic commit could
    /// replace, including missing parents that conversion could create later.
    pub fn start_excluding(path: &Path, excluded: &[&Path]) -> io::Result<Self> {
        fn identity(path: &Path) -> io::Result<std::path::PathBuf> {
            use std::path::Component;
            let absolute = std::path::absolute(path)?;
            let mut resolved = std::path::PathBuf::new();
            for component in absolute.components() {
                match component {
                    Component::CurDir => {}
                    Component::ParentDir => {
                        resolved.pop();
                    }
                    component => {
                        resolved.push(component.as_os_str());
                        match resolved.canonicalize() {
                            Ok(canonical) => resolved = canonical,
                            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                            Err(error) => return Err(error),
                        }
                    }
                }
            }
            Ok(resolved)
        }
        let report = identity(path)?;
        if excluded
            .iter()
            .any(|other| identity(other).is_ok_and(|other| other == report))
        {
            return Err(io::Error::other(
                "reliability report must not alias input, output or diagnostics",
            ));
        }
        Self::start(path)
    }

    /// Call only after workers have joined and native owners have dropped.
    pub fn finish(self) -> io::Result<()> {
        self.close()
    }

    fn close(&self) -> io::Result<()> {
        let mut registry = registry().lock().unwrap_or_else(|error| error.into_inner());
        if registry
            .state
            .as_ref()
            .is_none_or(|state| state.generation != self.generation)
        {
            return Ok(());
        }
        let mut state = registry.state.take().expect("session checked above");
        state.sample("post-cleanup");
        if let Some(error) = state.write_error {
            return Err(error);
        }
        if !state.errors.is_empty()
            || state
                .resources
                .values()
                .any(|value| value.active_count != 0)
        {
            return Err(io::Error::other(
                "unreleased ownership or accounting errors in reliability report",
            ));
        }
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Best effort on unwind. Explicit finish surfaces report failures.
        let _ = self.close();
    }
}

pub fn sample(phase: &str) {
    with_state(|state| state.sample(phase));
}

pub fn frame_completed() {
    with_state(|state| {
        state.frames += 1;
        if state.frames == 1 {
            state.sample("post-init");
        } else if state.frames % 1000 == 0 {
            state.sample("progress");
        }
    });
}

pub fn packet_written(bytes: u64) {
    with_state(|state| {
        state.packets += 1;
        state.packet_bytes += bytes;
    });
}

/// Pre-attempt snapshots are retained separately from successful-operation peaks.
pub fn observe_queue(name: &'static str, depth: usize, capacity: Option<usize>) {
    observe_queue_at(name, depth, capacity, false);
    #[cfg(feature = "native-reliability")]
    if capacity == Some(depth) {
        match name {
            "pipeline_decoded" => crate::reliability_hooks::checkpoint("PipelineDecodedFull"),
            "pipeline_processed" => crate::reliability_hooks::checkpoint("PipelineProcessedFull"),
            _ => {}
        }
    }
}

/// Snapshot immediately after a successful enqueue/dequeue. Another thread can
/// mutate the channel before `len()` is read, so this is not an atomic exact HWM.
pub fn observe_queue_boundary(name: &'static str, depth: usize, capacity: Option<usize>) {
    observe_queue_at(name, depth, capacity, true);
}

fn observe_queue_at(name: &'static str, depth: usize, capacity: Option<usize>, boundary: bool) {
    with_state(|state| {
        if capacity.is_some_and(|capacity| depth > capacity) {
            state.error(format!("queue exceeds configured capacity: {name}"));
        }
        let queue = state.queues.entry(name).or_insert_with(|| json!({
            "depth": null, "capacity": capacity, "peak_depth": null,
            "peak_kind": "Unavailable", "bytes": null,
            "boundary_observations": 0, "pre_operation_depth": null,
            "pre_operation_peak_depth": null,
            "atomic_exact_high_water_mark": false,
            "scope": "maximum len snapshots after each successful enqueue/dequeue; concurrent operations may race the snapshot; depth is last boundary observation, not cleanup-time depth",
        }));
        queue["capacity"] = json!(capacity);
        let key = if boundary {
            "peak_depth"
        } else {
            "pre_operation_peak_depth"
        };
        queue[key] = json!(queue[key].as_u64().unwrap_or(0).max(depth as u64));
        if boundary {
            queue["depth"] = json!(depth);
            queue["peak_kind"] = json!("MeasuredPeakAtOperationBoundary");
            queue["boundary_observations"] =
                json!(queue["boundary_observations"].as_u64().unwrap_or(0) + 1);
        } else {
            queue["pre_operation_depth"] = json!(depth);
        }
    });
}

/// These are writes since explicit flush, not FFmpeg's internal live bytes.
pub fn observe_mux(packets: u32, bytes: u64, flushed: Option<&'static str>) {
    with_state(|state| {
        let count = state.mux["flush_count"].as_u64().unwrap_or(0);
        let previous_reason = state.mux["last_flush_reason"].clone();
        state.mux = json!({
            "writes_since_explicit_flush": packets,
            "bytes_written_since_explicit_flush": bytes,
            "flush_count": count + u64::from(flushed.is_some()),
            "last_flush_reason": flushed.map(Value::from).unwrap_or(previous_reason),
            "internal_pending_packets": null, "internal_pending_bytes": null,
        });
    });
}

/// Tracks successful owned bindings/references, not driver allocations.
/// Release explicitly *after* native release. Dropping an unreleased token
/// records an error and leaves it active, including intentional abandonment.
pub struct ResourceToken {
    generation: Option<u64>,
    name: &'static str,
    bytes: Option<u64>,
}

impl ResourceToken {
    pub fn acquire(name: &'static str, bytes: Option<u64>) -> Self {
        let mut generation = None;
        with_state(|state| {
            generation = Some(state.generation);
            let value = state.resources.entry(name).or_insert_with(|| Resource {
                bytes_known: bytes.is_some(),
                ..Resource::default()
            });
            value.active_count += 1;
            value.active_bytes += bytes.unwrap_or(0);
            // Once any acquisition has unknown size, whole-series byte totals
            // remain unavailable even if later acquisitions provide sizes.
            value.bytes_known &= bytes.is_some();
            value.peak_count = value.peak_count.max(value.active_count);
            value.peak_bytes = value.peak_bytes.max(value.active_bytes);
        });
        Self {
            generation,
            name,
            bytes,
        }
    }

    pub fn release(&mut self) {
        let Some(generation) = self.generation.take() else {
            return;
        };
        with_state(|state| {
            if state.generation != generation {
                return;
            }
            let value = state
                .resources
                .get_mut(self.name)
                .expect("registered resource");
            if let (Some(count), Some(bytes)) = (
                value.active_count.checked_sub(1),
                value.active_bytes.checked_sub(self.bytes.unwrap_or(0)),
            ) {
                value.active_count = count;
                value.active_bytes = bytes;
            } else {
                state.error(format!("counter underflow: {}", self.name));
            }
        });
    }
}

impl Drop for ResourceToken {
    fn drop(&mut self) {
        if let Some(generation) = self.generation {
            with_state(|state| {
                if state.generation == generation {
                    state.error(format!("ownership not explicitly released: {}", self.name));
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrent_ownership_partial_evidence_and_session_isolation() {
        // The observation session is process-scoped. Other parallel pipeline
        // unit tests must not contribute frames to this session's oracle.
        const CHILD: &str = "ASCIIFLOW_RELIABILITY_UNIT_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "reliability::tests::concurrent_ownership_partial_evidence_and_session_isolation", "--test-threads=1"])
                .env(CHILD, "1")
                .status().unwrap();
            assert!(status.success());
            return;
        }
        let directory =
            std::env::temp_dir().join(format!("asciiflow-observe-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let alias = directory.join("aliased.mp4");
        assert!(Session::start_excluding(&alias, &[&directory.join("./aliased.mp4")]).is_err());
        assert!(!alias.exists());
        assert!(
            Session::start_excluding(&alias, &[&directory.join("not-created/../aliased.mp4")])
                .is_err()
        );
        assert!(!directory.join("not-created").exists());
        #[cfg(unix)]
        {
            let link = directory.join("directory-alias");
            std::os::unix::fs::symlink(&directory, &link).unwrap();
            assert!(Session::start_excluding(&alias, &[&link.join("aliased.mp4")]).is_err());
            assert!(!alias.exists());
        }
        let path = directory.join("balanced.jsonl");
        let session = Session::start(&path).unwrap();
        assert!(Session::start(directory.join("overlap")).is_err());
        let (sender, receiver) = crossbeam_channel::bounded(16);
        for value in 0..16 {
            sender.send(value).unwrap();
            observe_queue_boundary("queue_boundary_test", sender.len(), sender.capacity());
        }
        for _ in 0..16 {
            receiver.recv().unwrap();
            observe_queue_boundary("queue_boundary_test", receiver.len(), receiver.capacity());
        }
        // A later pre-attempt sample cannot erase or relabel boundary evidence.
        observe_queue("queue_boundary_test", 0, Some(16));
        observe_queue("pre_attempt_only", 16, Some(16));
        std::thread::scope(|scope| {
            for _ in 0..4 {
                scope.spawn(|| {
                    for _ in 0..1000 {
                        let mut token = ResourceToken::acquire("test", Some(64));
                        frame_completed();
                        token.release();
                        token.release();
                    }
                });
            }
        });
        session.finish().unwrap();
        let records: Vec<Value> = std::fs::read_to_string(&path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let final_row = records.last().unwrap();
        assert_eq!(final_row["frames_processed"], 4000);
        let queue = &final_row["queues"]["queue_boundary_test"];
        assert_eq!(queue["capacity"], 16);
        assert_eq!(queue["peak_depth"], 16);
        assert_eq!(queue["depth"], 0);
        assert_eq!(queue["boundary_observations"], 32);
        assert_eq!(queue["peak_kind"], "MeasuredPeakAtOperationBoundary");
        assert_eq!(queue["atomic_exact_high_water_mark"], false);
        assert!(final_row["queues"]["pre_attempt_only"]["peak_depth"].is_null());
        assert_eq!(final_row["resources"]["test"]["active_count"], 0);
        assert_eq!(final_row["resources"]["test"]["active_bytes"], 0);
        assert!(
            final_row["resources"]["test"]["peak_count"]
                .as_u64()
                .unwrap()
                <= 4
        );
        assert!(Session::start(&path).is_err());

        let failed_path = directory.join("unreleased.jsonl");
        let session = Session::start(&failed_path).unwrap();
        let mut late = ResourceToken::acquire("abandoned", None);
        assert!(session.finish().is_err());
        let next = Session::start(directory.join("next.jsonl")).unwrap();
        late.release(); // A previous generation must not affect this session.
        let mut unknown = ResourceToken::acquire("mixed", None);
        unknown.release();
        let mut known = ResourceToken::acquire("mixed", Some(64));
        known.release();
        next.finish().unwrap();
        let text = std::fs::read_to_string(directory.join("next.jsonl")).unwrap();
        let final_row: Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
        assert!(final_row["resources"]["mixed"]["active_bytes"].is_null());
        let io_path = directory.join("write-error.jsonl");
        let failed_io = Session::start(&io_path).unwrap();
        with_state(|state| {
            // Deterministic writer failure without filling a filesystem: a
            // read-only handle rejects the next record, retaining initial JSON.
            state.output = File::open(&io_path).unwrap();
        });
        frame_completed();
        assert!(failed_io.finish().is_err());
        let text = std::fs::read_to_string(io_path).unwrap();
        assert_eq!(text.lines().count(), 1);
        assert_eq!(
            serde_json::from_str::<Value>(text.trim()).unwrap()["phase"],
            "initial"
        );
        let text = std::fs::read_to_string(failed_path).unwrap();
        assert!(text.contains("\"active_bytes\":null"));
        std::fs::remove_dir_all(directory).unwrap();
    }
}
