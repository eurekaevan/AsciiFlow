//! Observation-only qualification: no allocator policy changes or cache purges.
use super::*;
use std::{ffi::CString, io::Read};

const CATEGORIES: [&str; 7] = [
    "heap",
    "anonymous_non_heap",
    "kernel_named_stack",
    "driver_device",
    "file_backed",
    "special",
    "other",
];

#[derive(Clone, Copy, Debug, Default)]
struct Allocator {
    arena: usize,
    mmap: usize,
    in_use_arena: usize,
    free_arena: usize,
    fastbin_free: usize,
    mmap_regions: usize,
}

impl Allocator {
    fn read() -> Self {
        // SAFETY: glibc's read-only, process-wide statistics have no arguments.
        let info = unsafe { libc::mallinfo2() };
        Self {
            arena: info.arena,
            mmap: info.hblkhd,
            in_use_arena: info.uordblks,
            free_arena: info.fordblks,
            fastbin_free: info.fsmblks,
            mmap_regions: info.hblks,
        }
    }

    fn json(self) -> Value {
        json!({"arena_bytes": self.arena, "mmap_bytes": self.mmap,
            "mapped_bytes": self.arena + self.mmap,
            "in_use_arena_bytes": self.in_use_arena,
            "in_use_including_mmap_bytes": self.in_use_arena + self.mmap,
            "free_retained_arena_bytes": self.free_arena,
            "fastbin_free_bytes": self.fastbin_free, "mmap_regions": self.mmap_regions,
            "scope": "glibc accounting, not application-live bytes or RSS; tcache may count as in-use; mmap bytes include allocation overhead"})
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Mapping {
    count: u64,
    size: u64,
    rss: u64,
    anonymous: u64,
    private_dirty: u64,
    private_clean: u64,
    shared_clean: u64,
    shared_dirty: u64,
    private_anonymous_lower: u64,
    private_anonymous_upper: u64,
}

fn category(header: &str) -> usize {
    let path = header.split_whitespace().nth(5).unwrap_or("");
    if path == "[heap]" {
        0
    } else if path.is_empty() || path.starts_with("[anon:") {
        1
    } else if path.starts_with("[stack") {
        2
    } else if path.starts_with("/dev/dri/") || path.starts_with("/dev/udmabuf") {
        3
    } else if path.starts_with('/') {
        4
    } else if path.starts_with('[') {
        5
    } else {
        6
    }
}

fn mapping_summary(text: &str) -> [Mapping; 7] {
    let mut result = [Mapping::default(); 7];
    let mut current = 6;
    let mut vma = [0u64; 5]; // Anonymous, private dirty/clean, shared dirty/clean.
    for line in text.lines() {
        let first = line.split_whitespace().next().unwrap_or("");
        if first.split_once('-').is_some_and(|(a, b)| {
            !a.is_empty()
                && !b.is_empty()
                && a.bytes().chain(b.bytes()).all(|v| v.is_ascii_hexdigit())
        }) {
            add_private_bounds(&mut result[current], vma);
            vma = [0; 5];
            current = category(line);
            result[current].count += 1;
        } else if let Some((key, value)) = line.split_once(':') {
            let target = match key {
                "Size" => &mut result[current].size,
                "Rss" => &mut result[current].rss,
                "Anonymous" => &mut result[current].anonymous,
                "Private_Dirty" => &mut result[current].private_dirty,
                "Private_Clean" => &mut result[current].private_clean,
                "Shared_Clean" => &mut result[current].shared_clean,
                "Shared_Dirty" => &mut result[current].shared_dirty,
                _ => continue,
            };
            let number = value
                .split_whitespace()
                .next()
                .unwrap()
                .parse::<u64>()
                .unwrap();
            *target += number;
            if let Some(index) = [
                "Anonymous",
                "Private_Dirty",
                "Private_Clean",
                "Shared_Dirty",
                "Shared_Clean",
            ]
            .iter()
            .position(|v| *v == key)
            {
                vma[index] = number;
            }
        }
    }
    add_private_bounds(&mut result[current], vma);
    result
}

fn add_private_bounds(mapping: &mut Mapping, vma: [u64; 5]) {
    // smaps cannot distinguish anonymous from file-backed shared/private pages
    // within a mixed VMA. Keep the interval instead of inventing exact ownership.
    mapping.private_anonymous_lower += vma[0].saturating_sub(vma[3] + vma[4]);
    mapping.private_anonymous_upper += vma[0].min(vma[1] + vma[2]);
}

fn field(text: &str, name: &str) -> u64 {
    text.lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            (key == name).then(|| value.split_whitespace().next().unwrap().parse().unwrap())
        })
        .unwrap_or_else(|| panic!("missing Linux accounting field {name}"))
}

struct Snapshot {
    allocator: Allocator,
    anonymous: u64,
    rss: u64,
    pss: u64,
    private_clean: u64,
    private_dirty: u64,
    swap: u64,
    fd: usize,
    threads: usize,
    maps: [Mapping; 7],
}

impl Snapshot {
    fn assert_baseline(&self, initial: &Self) {
        assert_eq!(
            self.fd, initial.fd,
            "FDs did not return to initial baseline"
        );
        assert_eq!(
            self.threads, initial.threads,
            "threads did not return to initial baseline"
        );
    }
    fn json(&self) -> Value {
        let maps: serde_json::Map<String, Value> = CATEGORIES
            .iter()
            .zip(self.maps)
            .map(|(name, m)| {
                (
                    (*name).into(),
                    json!({"count":m.count,"size_kib":m.size,"rss_kib":m.rss,
                    "anonymous_kib":m.anonymous,"private_dirty_kib":m.private_dirty,
                    "private_clean_kib":m.private_clean,"shared_clean_kib":m.shared_clean,"shared_dirty_kib":m.shared_dirty,
                    "private_anonymous_lower_kib":m.private_anonymous_lower,"private_anonymous_upper_kib":m.private_anonymous_upper}),
                )
            })
            .collect();
        json!({"private_anonymous_kib":self.anonymous,"smaps_rollup_anonymous_kib":self.anonymous,"rss_kib":self.rss,"pss_kib":self.pss,
            "private_clean_kib":self.private_clean,"private_dirty_kib":self.private_dirty,
            "swap_kib":self.swap,"fd_count":self.fd,"thread_count":self.threads,
            "private_anonymous_lower_kib":self.maps.iter().map(|m|m.private_anonymous_lower).sum::<u64>(),
            "private_anonymous_upper_kib":self.maps.iter().map(|m|m.private_anonymous_upper).sum::<u64>(),
            "mapping_count": self.maps.iter().map(|m|m.count).sum::<u64>(),
            "mapping_categories":maps,"allocator":self.allocator.json(),
            "metric_scope":"smaps_rollup.Anonymous is the historical conservative anonymous-resident HWM metric, not intrinsically an exclusive private-page census; per-mapping shared/private fields remain available for attribution. Private_Dirty may also include private file-backed pages.",
            "snapshot_scope":"Sequential correlated reads, not atomic; JSON/XML observer allocations are ephemeral but allocator retention is included",
            "mapping_count_source":"VMA headers in /proc/self/smaps, corresponding to /proc/self/maps entries; unnamed cached stacks remain anonymous_non_heap, not guessed"})
    }
}

// Fixed, pre-touched buffers keep growing proc-file buffers out of the HWM
// experiment. Capacity exhaustion is an explicit failure, never truncation.
struct ProcBuffer {
    bytes: Vec<u8>,
    len: usize,
}
impl ProcBuffer {
    fn new(capacity: usize) -> Self {
        Self {
            bytes: vec![1; capacity],
            len: 0,
        }
    }
    fn read(&mut self, path: &str) {
        let mut file = fs::File::open(path).expect("open proc accounting");
        self.len = 0;
        while self.len < self.bytes.len() {
            let n = file
                .read(&mut self.bytes[self.len..])
                .expect("read proc accounting");
            if n == 0 {
                return;
            }
            self.len += n;
        }
        let mut extra = [0];
        assert_eq!(
            file.read(&mut extra).unwrap(),
            0,
            "proc observation buffer exhausted"
        );
    }
    fn text(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.len]).expect("UTF-8 proc data")
    }
    fn save(&self, path: PathBuf) {
        fs::write(path, &self.bytes[..self.len]).expect("save exact proc snapshot");
    }
}

struct Observer {
    smaps: ProcBuffer,
    rollup: ProcBuffer,
    status: ProcBuffer,
    high_water: u64,
    last_hwm_cycle: usize,
    largest_increment: u64,
    events: BufWriter<fs::File>,
    samples: BufWriter<fs::File>,
}

impl Observer {
    fn new(root: &Path) -> Self {
        Self {
            smaps: ProcBuffer::new(8 * 1024 * 1024),
            rollup: ProcBuffer::new(128 * 1024),
            status: ProcBuffer::new(128 * 1024),
            high_water: 0,
            last_hwm_cycle: 0,
            largest_increment: 0,
            events: BufWriter::new(fs::File::create(root.join("hwm.jsonl")).unwrap()),
            samples: BufWriter::new(fs::File::create(root.join("memory.jsonl")).unwrap()),
        }
    }

    fn snapshot(&mut self) -> Snapshot {
        let allocator = Allocator::read();
        self.smaps.read("/proc/self/smaps");
        self.rollup.read("/proc/self/smaps_rollup");
        self.status.read("/proc/self/status");
        let text = self.rollup.text();
        Snapshot {
            allocator,
            anonymous: field(text, "Anonymous"),
            rss: field(text, "Rss"),
            pss: field(text, "Pss"),
            private_clean: field(text, "Private_Clean"),
            private_dirty: field(text, "Private_Dirty"),
            swap: field(text, "Swap"),
            fd: proc_count("/proc/self/fd").expect("FD census unavailable"),
            threads: proc_count("/proc/self/task").expect("thread census unavailable"),
            maps: mapping_summary(self.smaps.text()),
        }
    }

    fn observe(
        &mut self,
        root: &Path,
        cycle: usize,
        job: usize,
        kind: &str,
        boundary: &str,
    ) -> Snapshot {
        let snapshot = self.snapshot();
        let mut value = snapshot.json();
        value["cycle"] = json!(cycle);
        value["job_index"] = json!(job);
        value["job_type"] = json!(kind);
        value["boundary"] = json!(boundary);
        if snapshot.anonymous > self.high_water {
            let previous = self.high_water;
            self.high_water = snapshot.anonymous;
            self.last_hwm_cycle = cycle;
            let delta = self.high_water - previous;
            if cycle > 0 {
                self.largest_increment = self.largest_increment.max(delta);
            }
            value["hwm_event"] = json!({"previous_kib":previous,"new_kib":self.high_water,"delta_kib":delta,
                "resource_evidence":if cycle==0 {"Initial process observation, before any job"} else {"jobs.jsonl linked by job_index; all tracked counts/known bytes zero, workers joined and staging removed before this sample"}});
            let prefix = format!("hwm-c{cycle:03}-j{job:04}-{boundary}");
            self.smaps.save(root.join(format!("{prefix}.smaps")));
            self.rollup
                .save(root.join(format!("{prefix}.smaps_rollup")));
            self.status.save(root.join(format!("{prefix}.status")));
            malloc_xml(&root.join(format!("{prefix}.malloc.xml")));
            serde_json::to_writer(&mut self.events, &value).unwrap();
            writeln!(self.events).unwrap();
            self.events.flush().unwrap();
        }
        value["last_hwm_cycle"] = json!(self.last_hwm_cycle);
        let completed_cycle = if boundary == "cycle-end" {
            cycle
        } else {
            cycle.saturating_sub(1)
        };
        value["complete_cycles_without_new_hwm"] =
            json!(completed_cycle.saturating_sub(self.last_hwm_cycle));
        serde_json::to_writer(&mut self.samples, &value).unwrap();
        writeln!(self.samples).unwrap();
        self.samples.flush().unwrap();
        snapshot
    }
}

fn malloc_xml(path: &Path) {
    use std::os::unix::ffi::OsStrExt;
    let path = CString::new(path.as_os_str().as_bytes()).unwrap();
    // SAFETY: valid NUL-terminated path/mode; FILE is exclusively owned until
    // fclose, including the malloc_info error path. No trimming/tuning calls.
    unsafe {
        let file = libc::fopen(path.as_ptr(), c"wx".as_ptr());
        assert!(!file.is_null(), "create allocator XML snapshot");
        let result = libc::malloc_info(0, file);
        let close = libc::fclose(file);
        assert_eq!(result, 0, "malloc_info failed");
        assert_eq!(close, 0, "close allocator XML failed");
    }
}

#[derive(Default)]
struct Plateau {
    last_hwm: usize,
}
impl Plateau {
    fn observed(&self, cycle: usize) -> bool {
        cycle.saturating_sub(self.last_hwm) >= 100
    }
    fn stop(&self, cycle: usize, extended: bool) -> bool {
        cycle >= 500 || (!extended && cycle >= 200 && self.observed(cycle))
    }
}

#[test]
fn plateau_resets_on_every_increment_and_extended_run_cannot_stop_early() {
    let mut p = Plateau { last_hwm: 117 };
    assert!(!p.observed(216));
    assert!(p.observed(217));
    assert!(!p.stop(217, true));
    assert!(p.stop(217, false));
    p.last_hwm = 499;
    assert!(!p.observed(500));
    assert!(p.stop(500, true));
}

#[test]
fn mapping_categories_do_not_guess_anonymous_library_or_cached_stack_ownership() {
    let text = "1000-2000 rw-p 00000000 00:00 0 [heap]\nAnonymous: 4 kB\n2000-3000 rw-p 00000000 00:00 0\nAnonymous: 8 kB\n3000-4000 rw-p 00000000 00:00 0 [stack]\nAnonymous: 12 kB\n4000-5000 rw-p 00000000 00:00 0 /dev/dri/renderD128\nAnonymous: 16 kB\n5000-6000 r--p 00000000 00:00 1 /usr/lib/libva.so\nAnonymous: 20 kB\n";
    let m = mapping_summary(text);
    assert_eq!(m[0].anonymous, 4);
    assert_eq!(m[1].anonymous, 8);
    assert_eq!(m[2].anonymous, 12);
    assert_eq!(m[3].anonymous, 16);
    assert_eq!(m[4].anonymous, 20);
}

#[test]
fn read_only_telemetry_saves_complete_proc_and_allocator_snapshots() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "asciiflow-memory-observer-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir(&root).unwrap();
    let mut observer = Observer::new(&root);
    let sample = observer.observe(&root, 0, 0, "initial", "initial");
    assert!(sample.allocator.arena + sample.allocator.mmap > 0);
    assert!(sample.maps.iter().map(|m| m.count).sum::<u64>() > 0);
    assert!(
        fs::read_to_string(root.join("hwm-c000-j0000-initial.malloc.xml"))
            .unwrap()
            .contains("<malloc version=")
    );
    assert!(
        fs::read_to_string(root.join("hwm-c000-j0000-initial.smaps"))
            .unwrap()
            .contains("Anonymous:")
    );
    drop(observer);
    for entry in fs::read_dir(&root).unwrap() {
        fs::remove_file(entry.unwrap().path()).unwrap();
    }
    fs::remove_dir(&root).unwrap();
}

#[test]
#[ignore = "default allocator, Intel GPU, explicit evidence directory; 200-500 complete fixed cycles"]
fn default_memory_plateau() {
    for key in ["ASCIIFLOW_D1A_NATIVE_GPU", "ASCIIFLOW_C4B_PRODUCTION"] {
        assert_eq!(std::env::var(key).as_deref(), Ok("1"));
    }
    for (key, _) in std::env::vars_os() {
        let k = key.to_string_lossy();
        assert!(
            !k.starts_with("MALLOC_")
                && !matches!(
                    k.as_ref(),
                    "GLIBC_TUNABLES"
                        | "LD_PRELOAD"
                        | "ASCIIFLOW_VULKAN_VALIDATION"
                        | "ASCIIFLOW_REQUIRE_VULKAN_VALIDATION"
                        | "VK_INSTANCE_LAYERS"
                ),
            "non-default environment: {k}"
        );
    }
    let extended = match std::env::var("ASCIIFLOW_MEMORY_PLATEAU_MODE").as_deref() {
        Ok("adaptive") => false,
        Ok("extended-500") => true,
        v => panic!("explicit plateau mode required: {v:?}"),
    };
    let root = unique_directory();
    let pq_audio = pq_audio_inputs(&root);
    let mut jobs = BufWriter::new(fs::File::create(root.join("jobs.jsonl")).unwrap());
    let mut observer = Observer::new(&root);
    let mut references = HashMap::new();
    let initial = observer.observe(&root, 0, 0, "initial", "initial");
    let mut p = Plateau::default();
    let mut index = 0;
    let mut cycle = 0;
    let final_snapshot = loop {
        cycle += 1;
        let outputs = [
            OutputKind::Sdr8H264,
            OutputKind::Pq10Hevc,
            OutputKind::PqToSdr8H264,
            OutputKind::PqToSdr10Hevc,
        ];
        for (slot, output) in outputs.into_iter().enumerate() {
            let job = case(slot, output, HookAction::None);
            plateau_job(&job, index, &root, &pq_audio, &mut references, &mut jobs);
            observer
                .observe(&root, cycle, index, &job.label, "job-end")
                .assert_baseline(&initial);
            index += 1;
        }
        // Repeat the original 50-cycle macro sequence, including its four
        // declared recovery points; no new job shape or growing cache keyspace.
        let within = cycle % 50;
        if matches!(within, 10 | 20 | 30 | 40) {
            let action = if within % 20 == 0 {
                HookAction::InjectFailure {
                    phase: "EncoderBusy",
                    cancel_on_phase: true,
                }
            } else {
                HookAction::Cancel("VulkanInFlight")
            };
            let job = case(0, OutputKind::PqToSdr10Hevc, action);
            plateau_job(&job, index, &root, &pq_audio, &mut references, &mut jobs);
            observer
                .observe(&root, cycle, index, &job.label, "job-end")
                .assert_baseline(&initial);
            index += 1;
        }
        let snapshot = observer.observe(
            &root,
            cycle,
            index.saturating_sub(1),
            "complete-fixed-mixed-cycle",
            "cycle-end",
        );
        snapshot.assert_baseline(&initial);
        p.last_hwm = observer.last_hwm_cycle;
        if p.stop(cycle, extended) {
            break snapshot;
        }
    };
    jobs.flush().unwrap();
    fs::write(root.join("plateau-result.json"),serde_json::to_vec_pretty(&json!({
        "schema":"asciiflow-default-memory-plateau-v1","process_id":std::process::id(),"cycles":cycle,"jobs":index,"extended":extended,
        "last_hwm_cycle":p.last_hwm,"complete_cycles_without_new_hwm":cycle-p.last_hwm,
        "observed_plateau":p.observed(cycle),"largest_hwm_increment_kib":observer.largest_increment,
        "initial_private_anonymous_kib":initial.anonymous,"final_high_water_kib":observer.high_water,
        "final_private_anonymous_kib":final_snapshot.anonymous,"final_allocator":final_snapshot.allocator.json(),
        "reference_configurations":references.len(),"retained_reference_bytes":references.values().map(Vec::len).sum::<usize>(),
        "observer_fixed_proc_buffer_bytes":8*1024*1024+2*128*1024,
        "allocator_policy":"unchanged glibc default; read-only mallinfo2/malloc_info; no trim/tuning",
        "interpretation":"Operational finite-workload plateau only; allocation/mapping/live-byte review still required; not an automatic seal"
    })).unwrap()).unwrap();
}

fn plateau_job(
    job: &Case,
    index: usize,
    root: &Path,
    pq_audio: &[PathBuf; 3],
    references: &mut HashMap<String, Vec<u8>>,
    jobs: &mut BufWriter<fs::File>,
) {
    let record = execute_case(job, index, root, pq_audio, references);
    assert_eq!(record["thread_count_before"], record["thread_count_after"]);
    if matches!(job.action, HookAction::None) {
        assert_eq!(
            record["byte_exact_with_first_output_for_configuration"],
            true
        );
    }
    serde_json::to_writer(&mut *jobs, &record).unwrap();
    writeln!(jobs).unwrap();
    jobs.flush().unwrap();
    // Record is dropped before allocator/proc sampling, not kept as live memory.
}
