//! Samples numeric Linux pressure diagnostics and classifies sustained stalls.
//!
//! This module neither signals processes nor owns recovery. Shared ancestor
//! counters describe budget pressure, never which process caused an OOM or kill.
//! A procfs snapshot is racy even when complete; it is never a cleanup barrier.

use std::{
    collections::HashMap,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::time::Instant;

// Bounds cap allocation and cooperative collection work, not syscall latency.
const PID_SCAN_LIMIT: usize = 4096;
const MEMBER_LIMIT: usize = 256;
const ANCESTOR_LIMIT: usize = 32;
const DETAIL_LIMIT: usize = 16;
const TEXT_LIMIT: usize = 4096;
const SCALAR_LIMIT: usize = 128;
const COLLECTION_BUDGET: Duration = Duration::from_millis(250);
const SAMPLE_INTERVAL: Duration = Duration::from_secs(30);
// Three independent 30-second observations plus 180 seconds without completion
// avoid reacting to ordinary reclaim bursts. Gaps over 60 seconds break a run.
const NO_PROGRESS: Duration = Duration::from_secs(180);
const REQUIRED_SAMPLES: u8 = 3;
const HIGH_RATIO: f64 = 0.90;
const CLEAR_RATIO: f64 = 0.80;
// PSI avg10 is percent; a 1% interval stall fraction must corroborate avg10.
const PSI_PERCENT: f64 = 1.0;
const PSI_FRACTION: f64 = 0.01;
// Aggregate CPU below 1% of one core corroborates D-state I/O blockage.
const LOW_CPU_FRACTION: f64 = 0.01;

/// Identifies one process incarnation, so PID reuse cannot create CPU deltas.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct ProcessKey {
    /// Linux numeric process ID.
    pub(super) pid: u32,
    /// Kernel start time in clock ticks since boot.
    pub(super) start_ticks: u64,
}

/// Contains numeric process-group observations, without command names.
#[derive(Clone, Debug)]
pub(super) struct ProcessSample {
    /// Stable identity for this observation.
    pub(super) key: ProcessKey,
    /// Parent process ID reported by procfs.
    pub(super) parent_pid: u32,
    /// Recognized kernel state; unknown states make the scan incomplete.
    pub(super) state: char,
    /// User plus system CPU ticks since process start.
    pub(super) cpu_ticks: u64,
    /// CPU ticks since the previous observation of the same incarnation.
    pub(super) cpu_delta_ticks: Option<u64>,
    /// Resident bytes using the runtime page size.
    pub(super) rss_bytes: u64,
}

/// Distinguishes an unlimited kernel budget from a finite byte limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MemoryLimit {
    /// No finite limit is configured.
    Unlimited,
    /// A finite limit, including the valid zero-byte limit.
    Bytes(u64),
}

/// Contains the four relevant cumulative memory-event counters.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct MemoryEvents {
    /// Reclaim throttling at memory.high.
    pub(super) high: u64,
    /// Attempts to exceed memory.max.
    pub(super) max: u64,
    /// OOM invocations in this cgroup subtree.
    pub(super) oom: u64,
    /// OOM kills in this subtree, without victim attribution.
    pub(super) oom_kill: u64,
}

impl MemoryEvents {
    fn delta(self, old: Self) -> Option<Self> {
        Some(Self {
            high: self.high.checked_sub(old.high)?,
            max: self.max.checked_sub(old.max)?,
            oom: self.oom.checked_sub(old.oom)?,
            oom_kill: self.oom_kill.checked_sub(old.oom_kill)?,
        })
    }
    fn any(self) -> bool {
        self.high > 0 || self.max > 0 || self.oom > 0 || self.oom_kill > 0
    }
}

/// Contains a PSI line and its monotonic counter delta in microseconds.
#[derive(Clone, Copy, Debug)]
pub(super) struct PsiLine {
    /// Kernel 10-second stall average, in percent.
    pub(super) avg10: f64,
    /// Cumulative stall time in microseconds.
    pub(super) total_us: u64,
    /// Interval stall time; absent on first observation or counter reset.
    pub(super) delta_us: Option<u64>,
}

/// Contains both partial and whole-cgroup stall observations.
#[derive(Clone, Copy, Debug)]
pub(super) struct PsiSample {
    /// At least one task was stalled.
    pub(super) some: PsiLine,
    /// All non-idle tasks were stalled.
    pub(super) full: PsiLine,
}

impl PsiSample {
    fn delta(mut self, old: Self) -> Self {
        self.some.delta_us = self.some.total_us.checked_sub(old.some.total_us);
        self.full.delta_us = self.full.total_us.checked_sub(old.full.total_us);
        self
    }
    fn known(self) -> bool {
        self.some.delta_us.is_some() && self.full.delta_us.is_some()
    }
    fn meaningful(self, interval: Duration) -> bool {
        [self.some, self.full].iter().any(|line| {
            line.avg10 >= PSI_PERCENT
                && line.delta_us.is_some_and(|delta| {
                    delta as f64 >= interval.as_secs_f64() * 1e6 * PSI_FRACTION
                })
        })
    }
}

/// Contains one config-resolved cgroup's budget and pressure observations.
#[derive(Clone, Debug)]
pub(super) struct CgroupSample {
    /// Directory index only; ancestors may be shared with unrelated workloads.
    pub(super) index: usize,
    /// Resident memory charged to the cgroup subtree, in bytes.
    pub(super) memory_current: u64,
    /// Configured reclaim threshold.
    pub(super) memory_high: MemoryLimit,
    /// Configured hard limit.
    pub(super) memory_max: MemoryLimit,
    /// Current cumulative event counters.
    pub(super) events: MemoryEvents,
    /// Event increments; absent at baseline or after any reset.
    pub(super) event_delta: Option<MemoryEvents>,
    /// Memory PSI observations.
    pub(super) memory_psi: PsiSample,
    /// I/O PSI observations.
    pub(super) io_psi: PsiSample,
}

impl CgroupSample {
    fn ratio(&self) -> Option<f64> {
        [self.memory_high, self.memory_max]
            .into_iter()
            .filter_map(|limit| match limit {
                MemoryLimit::Bytes(0) => Some(if self.memory_current == 0 {
                    0.0
                } else {
                    f64::INFINITY
                }),
                MemoryLimit::Bytes(bytes) => Some(self.memory_current as f64 / bytes as f64),
                MemoryLimit::Unlimited => None,
            })
            .reduce(f64::max)
    }
    fn known(&self) -> bool {
        self.event_delta.is_some() && self.memory_psi.known() && self.io_psi.known()
    }
}

/// Contains one bounded diagnostic observation; completeness is not reap proof.
#[derive(Clone, Debug)]
pub(super) struct ResourceSample {
    /// Caller-supplied monotonic observation time.
    pub(super) now: Instant,
    /// Actual time since the preceding sample, absent on the first sample.
    pub(super) interval: Option<Duration>,
    /// False for missing diagnostics, deadline exhaustion or any truncation.
    pub(super) complete: bool,
    /// Numeric clock ticks per second from sysconf.
    pub(super) ticks_per_second: Option<u64>,
    /// Observed process-group members, bounded to 256.
    pub(super) processes: Vec<ProcessSample>,
    /// Config-owned cgroup chain, bounded to 32 directories.
    pub(super) cgroups: Vec<CgroupSample>,
    /// New direct-child worker incarnations replacing missing direct children;
    /// helper descendants do not contribute. Never a kill claim.
    pub(super) worker_replacements: Option<usize>,
}

impl ResourceSample {
    /// Emits only bounded numeric diagnostics and recognized process states.
    pub(super) fn log(&self) {
        tracing::info!(
            complete = self.complete,
            members = self.processes.len(),
            cgroups = self.cgroups.len(),
            interval_ms = self.interval.map(|v| v.as_millis() as u64),
            replacements = self.worker_replacements,
            "Evaluator pressure sample"
        );
        for p in self.processes.iter().take(DETAIL_LIMIT) {
            tracing::debug!(pid = p.key.pid, start_ticks = p.key.start_ticks,
                cpu_delta_ticks = p.cpu_delta_ticks, rss_bytes = p.rss_bytes,
                state = %p.state, "Evaluator process observation");
        }
        for c in &self.cgroups {
            tracing::debug!(
                index = c.index,
                current_bytes = c.memory_current,
                high_bytes = finite(c.memory_high),
                max_bytes = finite(c.memory_max),
                high_events = c.events.high,
                max_events = c.events.max,
                oom_events = c.events.oom,
                oom_kill_events = c.events.oom_kill,
                high_delta = c.event_delta.map(|v| v.high),
                max_delta = c.event_delta.map(|v| v.max),
                oom_delta = c.event_delta.map(|v| v.oom),
                oom_kill_delta = c.event_delta.map(|v| v.oom_kill),
                baseline = c.event_delta.is_none(),
                memory_some_avg10 = c.memory_psi.some.avg10,
                memory_full_avg10 = c.memory_psi.full.avg10,
                memory_some_total_us = c.memory_psi.some.total_us,
                memory_full_total_us = c.memory_psi.full.total_us,
                memory_some_delta_us = c.memory_psi.some.delta_us,
                memory_full_delta_us = c.memory_psi.full.delta_us,
                io_some_avg10 = c.io_psi.some.avg10,
                io_full_avg10 = c.io_psi.full.avg10,
                io_some_total_us = c.io_psi.some.total_us,
                io_full_total_us = c.io_psi.full.total_us,
                io_some_delta_us = c.io_psi.some.delta_us,
                io_full_delta_us = c.io_psi.full.delta_us,
                "Shared cgroup pressure observation"
            );
        }
    }
}

fn finite(limit: MemoryLimit) -> Option<u64> {
    match limit {
        MemoryLimit::Bytes(v) => Some(v),
        MemoryLimit::Unlimited => None,
    }
}

/// Owns bounded diagnostic baselines, with no background task or process control.
pub(super) struct PressureSampler {
    pgid: i32,
    evaluator_pid: u32,
    directories: Option<Vec<PathBuf>>,
    directories_truncated: bool,
    previous: Option<ResourceSample>,
    ticks: Option<u64>,
    pages: Option<u64>,
}

impl PressureSampler {
    /// Creates an unavailable-cgroup sampler until config supplies directories.
    pub(super) fn new(pgid: i32, evaluator_pid: u32) -> Self {
        // SAFETY: sysconf reads process-global numeric constants; no pointers or
        // ownership cross the FFI boundary. Nonpositive results stay unknown.
        let (ticks, pages) = unsafe {
            (
                libc::sysconf(libc::_SC_CLK_TCK),
                libc::sysconf(libc::_SC_PAGESIZE),
            )
        };
        Self {
            pgid,
            evaluator_pid,
            directories: None,
            directories_truncated: false,
            previous: None,
            ticks: u64::try_from(ticks).ok().filter(|v| *v > 0),
            pages: u64::try_from(pages).ok().filter(|v| *v > 0),
        }
    }

    /// Installs config-resolved directories, or records discovery unavailable.
    ///
    /// Pass the result of config's evaluator cgroup discovery as `result.ok()`.
    /// Directories run from the evaluator cgroup to the visible mount root;
    /// only the final directory may omit all memory-controller files. An
    /// existing root without those files is omitted, not treated as healthy.
    /// Changed chains invalidate baselines; overlong chains remain incomplete.
    pub(super) fn set_cgroup_directories(&mut self, mut directories: Option<Vec<PathBuf>>) {
        let truncated = directories
            .as_ref()
            .is_some_and(|v| v.len() > ANCESTOR_LIMIT);
        if let Some(paths) = &mut directories {
            paths.truncate(ANCESTOR_LIMIT);
        }
        if self.directories != directories || self.directories_truncated != truncated {
            self.previous = None;
        }
        self.directories = directories;
        self.directories_truncated = truncated;
    }

    /// Collects synchronous procfs diagnostics within cooperative work bounds.
    ///
    /// Call from one owner every 30 seconds. No tasks are spawned or queued.
    /// The deadline is checked between reads; kernel read latency is not bounded
    /// by userspace. Paths must come from trusted config discovery, not clients.
    pub(super) fn sample(&mut self, now: Instant) -> ResourceSample {
        let deadline = std::time::Instant::now() + COLLECTION_BUDGET;
        let mut sample = ResourceSample {
            now,
            interval: self
                .previous
                .as_ref()
                .and_then(|p| now.checked_duration_since(p.now)),
            complete: self.pgid > 0
                && self.ticks.is_some()
                && self.pages.is_some()
                && !self.directories_truncated,
            ticks_per_second: self.ticks,
            processes: Vec::new(),
            cgroups: Vec::new(),
            worker_replacements: None,
        };
        let mut scanned = 0;
        match std::fs::read_dir("/proc") {
            Err(_) => sample.complete = false,
            Ok(entries) => {
                for entry in entries {
                    if std::time::Instant::now() >= deadline {
                        sample.complete = false;
                        break;
                    }
                    let Ok(entry) = entry else {
                        sample.complete = false;
                        continue;
                    };
                    let Some(pid) = entry
                        .file_name()
                        .to_str()
                        .and_then(|s| s.parse::<u32>().ok())
                    else {
                        continue;
                    };
                    scanned += 1;
                    if scanned > PID_SCAN_LIMIT {
                        tracing::warn!(
                            pid_scan_limit = PID_SCAN_LIMIT,
                            "Evaluator process scan truncated"
                        );
                        sample.complete = false;
                        break;
                    }
                    let text = match read_bounded(&entry.path().join("stat"), TEXT_LIMIT) {
                        Ok(text) => text,
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                        Err(_) => {
                            sample.complete = false;
                            continue;
                        }
                    };
                    let Some((pgid, mut process)) = parse_stat(pid, &text, self.pages.unwrap_or(0))
                    else {
                        sample.complete = false;
                        continue;
                    };
                    if pgid != self.pgid {
                        continue;
                    }
                    if sample.processes.len() == MEMBER_LIMIT {
                        tracing::warn!(
                            member_limit = MEMBER_LIMIT,
                            "Evaluator member sample truncated"
                        );
                        sample.complete = false;
                        break;
                    }
                    process.cpu_delta_ticks = cpu_delta(&process, self.previous.as_ref());
                    sample.processes.push(process);
                }
            }
        }
        if let Some(directories) = &self.directories {
            if self.directories_truncated {
                tracing::warn!(
                    ancestor_limit = ANCESTOR_LIMIT,
                    "Evaluator cgroup chain truncated"
                );
            }
            let (cgroups, complete) = collect_cgroups(
                directories,
                self.directories_truncated,
                self.previous.as_ref(),
                deadline,
            );
            sample.cgroups = cgroups;
            sample.complete &= complete;
        } else {
            sample.complete = false;
        }
        if std::time::Instant::now() >= deadline {
            sample.complete = false;
        }
        if let Some(old) = self
            .previous
            .as_ref()
            .filter(|p| p.complete && sample.complete)
        {
            sample.worker_replacements = Some(replacement_count(old, &sample, self.evaluator_pid));
        }
        self.previous = Some(sample.clone());
        sample
    }
}

fn collect_cgroups(
    directories: &[PathBuf],
    truncated: bool,
    previous: Option<&ResourceSample>,
    deadline: std::time::Instant,
) -> (Vec<CgroupSample>, bool) {
    let mut complete = !truncated && !directories.is_empty() && directories.len() <= ANCESTOR_LIMIT;
    let mut samples = Vec::new();
    for (index, directory) in directories.iter().take(ANCESTOR_LIMIT).enumerate() {
        if std::time::Instant::now() >= deadline {
            complete = false;
            break;
        }
        if let Some(mut cgroup) = read_cgroup(index, directory, deadline) {
            if let Some(old) = previous
                .filter(|p| p.complete)
                .and_then(|p| p.cgroups.iter().find(|c| c.index == index))
            {
                cgroup.event_delta = cgroup.events.delta(old.events);
                cgroup.memory_psi = cgroup.memory_psi.delta(old.memory_psi);
                cgroup.io_psi = cgroup.io_psi.delta(old.io_psi);
            }
            samples.push(cgroup);
        } else if truncated
            || index + 1 != directories.len()
            || !root_without_memory_controller(directory, deadline)
        {
            complete = false;
        }
    }
    // A root-only hierarchy supplies no cgroup pressure evidence. Physical
    // capacity policy belongs to config; it cannot establish diagnostic health.
    complete &= !samples.is_empty() && std::time::Instant::now() < deadline;
    (samples, complete)
}

fn root_without_memory_controller(directory: &Path, deadline: std::time::Instant) -> bool {
    // COMPATIBILITY: The ordinary cgroup2 mount root lacks these controller
    // files. Config guarantees root-last ordering. Missing non-root files,
    // vanished roots, partial controllers and permission errors stay unknown.
    if !std::fs::metadata(directory).is_ok_and(|metadata| metadata.is_dir()) {
        return false;
    }
    [
        "memory.current",
        "memory.high",
        "memory.max",
        "memory.events",
    ]
    .iter()
    .all(|name| {
        std::time::Instant::now() < deadline
            && std::fs::symlink_metadata(directory.join(name))
                .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
    })
}

fn cpu_delta(process: &ProcessSample, previous: Option<&ResourceSample>) -> Option<u64> {
    previous
        .filter(|p| p.complete)?
        .processes
        .iter()
        .find(|old| old.key == process.key)
        .and_then(|old| process.cpu_ticks.checked_sub(old.cpu_ticks))
}

fn replacement_count(old: &ResourceSample, current: &ResourceSample, evaluator_pid: u32) -> usize {
    // INVARIANT: Only the evaluator leader's direct children represent workers.
    // Grandchild helper churn must not change the memory-pressure category.
    let workers = |s: &ResourceSample| {
        s.processes
            .iter()
            .filter(|p| p.parent_pid == evaluator_pid && p.key.pid != evaluator_pid)
            .map(|p| p.key)
            .collect::<Vec<_>>()
    };
    let before = workers(old);
    let after = workers(current);
    before
        .iter()
        .filter(|p| !after.contains(p))
        .count()
        .min(after.iter().filter(|p| !before.contains(p)).count())
}

fn read_bounded(path: &Path, limit: usize) -> std::io::Result<String> {
    let mut bytes = Vec::with_capacity(limit + 1);
    File::open(path)?
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(std::io::ErrorKind::InvalidData.into());
    }
    String::from_utf8(bytes).map_err(|_| std::io::ErrorKind::InvalidData.into())
}

fn parse_limit(text: &str) -> Option<MemoryLimit> {
    match text.trim() {
        "max" => Some(MemoryLimit::Unlimited),
        s => s.parse().ok().map(MemoryLimit::Bytes),
    }
}

fn parse_events(text: &str) -> Option<MemoryEvents> {
    let mut values = HashMap::new();
    for line in text.lines() {
        let mut words = line.split_whitespace();
        let name = words.next()?;
        let value = words.next()?.parse::<u64>().ok()?;
        if words.next().is_some() || values.insert(name, value).is_some() {
            return None;
        }
    }
    Some(MemoryEvents {
        high: *values.get("high")?,
        max: *values.get("max")?,
        oom: *values.get("oom")?,
        oom_kill: *values.get("oom_kill")?,
    })
}

fn parse_psi(text: &str) -> Option<PsiSample> {
    let mut some = None;
    let mut full = None;
    for line in text.lines() {
        let mut words = line.split_whitespace();
        let kind = words.next()?;
        let mut fields = HashMap::new();
        for word in words {
            let (key, value) = word.split_once('=')?;
            if fields.insert(key, value).is_some() {
                return None;
            }
        }
        for key in ["avg10", "avg60", "avg300"] {
            let avg = fields.get(key)?.parse::<f64>().ok()?;
            if !avg.is_finite() || !(0.0..=100.0).contains(&avg) {
                return None;
            }
        }
        let parsed = PsiLine {
            avg10: fields.get("avg10")?.parse().ok()?,
            total_us: fields.get("total")?.parse().ok()?,
            delta_us: None,
        };
        let target = match kind {
            "some" => &mut some,
            "full" => &mut full,
            _ => return None,
        };
        if target.replace(parsed).is_some() {
            return None;
        }
    }
    Some(PsiSample {
        some: some?,
        full: full?,
    })
}

fn parse_stat(pid: u32, text: &str, pages: u64) -> Option<(i32, ProcessSample)> {
    let (prefix, suffix) = text.rsplit_once(')')?;
    if prefix.split_once('(')?.0.trim().parse::<u32>().ok()? != pid {
        return None;
    }
    let fields: Vec<_> = suffix.split_whitespace().collect();
    let state = *fields.first()?;
    if !matches!(
        state,
        "R" | "S" | "D" | "Z" | "T" | "t" | "X" | "x" | "K" | "W" | "P" | "I"
    ) {
        return None;
    }
    // Suffix starts at field 3; CPU is fields 14/15, start 22, RSS 24.
    Some((
        fields.get(2)?.parse().ok()?,
        ProcessSample {
            key: ProcessKey {
                pid,
                start_ticks: fields.get(19)?.parse().ok()?,
            },
            parent_pid: fields.get(1)?.parse().ok()?,
            state: state.chars().next()?,
            cpu_ticks: fields
                .get(11)?
                .parse::<u64>()
                .ok()?
                .checked_add(fields.get(12)?.parse().ok()?)?,
            cpu_delta_ticks: None,
            rss_bytes: fields.get(21)?.parse::<u64>().ok()?.checked_mul(pages)?,
        },
    ))
}

fn read_cgroup(
    index: usize,
    directory: &Path,
    deadline: std::time::Instant,
) -> Option<CgroupSample> {
    let read = |name, limit| {
        if std::time::Instant::now() >= deadline {
            None
        } else {
            read_bounded(&directory.join(name), limit).ok()
        }
    };
    let scalar = |name| read(name, SCALAR_LIMIT);
    let text = |name| read(name, TEXT_LIMIT);
    Some(CgroupSample {
        index,
        memory_current: scalar("memory.current")?.trim().parse().ok()?,
        memory_high: parse_limit(&scalar("memory.high")?)?,
        memory_max: parse_limit(&scalar("memory.max")?)?,
        events: parse_events(&text("memory.events")?)?,
        event_delta: None,
        memory_psi: parse_psi(&text("memory.pressure")?)?,
        io_psi: parse_psi(&text("io.pressure")?)?,
    })
}

/// Identifies corroborated pressure without attributing process death.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SignalReason {
    /// Near-budget memory plus event increments or memory PSI.
    ReclaimPressure,
    /// Repeated D-state observations plus low CPU and I/O PSI.
    BlockedIo,
    /// Direct-child replacement throughout a corroborated memory-pressure run.
    WorkerReplacement,
}

/// Reports policy evidence only; the runtime owner decides how to recover.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StallDecision {
    /// Required numeric observations or interval baselines are unavailable.
    Unknown,
    /// No sustained actionable stall, including suppressed repeat decisions.
    Observe,
    /// Three corroborated samples and at least 180 seconds without completion.
    Recover(SignalReason),
    /// Three healthy samples rearm one subsequent escalation.
    Cleared,
}

/// Tracks consecutive evidence and suppresses repeats until healthy clearance.
#[derive(Default)]
pub(super) struct PressureHysteresis {
    completed: Option<usize>,
    last_now: Option<Instant>,
    reason: Option<SignalReason>,
    pressure_run: u8,
    clear_run: u8,
    latched: bool,
    blocked_keys: Vec<ProcessKey>,
}

impl PressureHysteresis {
    /// Creates an unlatched policy with no observation history.
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// Updates pure evidence with progress supplied by the runtime owner.
    ///
    /// Missing data breaks consecutive runs without clearing a latched stall.
    /// Completion changes break pressure runs. Health requires every finite
    /// budget below 80%, no events and no meaningful memory or I/O PSI. Samples
    /// with unknown CPU or worker-replacement baselines cannot confirm health.
    /// Samples must be 30–60 seconds apart; repeated updates cannot accelerate
    /// recovery. Reclaim and replacement observations share the same sustained
    /// memory-pressure run. Mixed runs report `ReclaimPressure`; replacement-only
    /// runs report `WorkerReplacement`. I/O runs retain their separate persistent
    /// D-state requirement and cannot borrow a preceding memory-pressure run.
    pub(super) fn update(
        &mut self,
        sample: &ResourceSample,
        completed_count: usize,
        last_completion_elapsed: Duration,
    ) -> StallDecision {
        let progressed = self.completed.is_some_and(|old| old != completed_count);
        self.completed = Some(completed_count);
        let spaced = self
            .last_now
            .and_then(|old| sample.now.checked_duration_since(old))
            .is_some_and(|elapsed| elapsed >= SAMPLE_INTERVAL && elapsed <= SAMPLE_INTERVAL * 2);
        self.last_now = Some(sample.now);
        if progressed || !spaced {
            self.pressure_run = 0;
            self.clear_run = 0;
            self.reason = None;
            self.blocked_keys.clear();
        }
        let Some(interval) = sample
            .interval
            .filter(|i| *i >= SAMPLE_INTERVAL && *i <= SAMPLE_INTERVAL * 2)
        else {
            return self.unknown();
        };
        if !sample.complete
            || sample.cgroups.is_empty()
            || sample.cgroups.iter().any(|c| !c.known())
        {
            return self.unknown();
        }
        let reclaim = sample.cgroups.iter().any(|c| {
            c.ratio().is_some_and(|r| r >= HIGH_RATIO)
                && (c.event_delta.is_some_and(|e| e.high > 0 || e.max > 0)
                    || c.memory_psi.meaningful(interval))
        });
        let io = sample.cgroups.iter().any(|c| c.io_psi.meaningful(interval));
        let cpu = sample
            .processes
            .iter()
            .try_fold(0_u64, |sum, p| sum.checked_add(p.cpu_delta_ticks?));
        let low_cpu = cpu.zip(sample.ticks_per_second).is_some_and(|(ticks, hz)| {
            hz > 0 && ticks as f64 <= hz as f64 * interval.as_secs_f64() * LOW_CPU_FRACTION
        });
        let blocked = sample.processes.iter().any(|p| p.state == 'D') && low_cpu && io;
        let reason = if reclaim && sample.worker_replacements.is_some_and(|v| v > 0) {
            Some(SignalReason::WorkerReplacement)
        } else if reclaim {
            Some(SignalReason::ReclaimPressure)
        } else if blocked {
            Some(SignalReason::BlockedIo)
        } else {
            None
        };
        // INVARIANT: I/O evidence must retain a D-state incarnation across the
        // entire run. Unrelated transient D-state tasks do not accumulate.
        if reason == Some(SignalReason::BlockedIo) {
            let keys: Vec<_> = sample
                .processes
                .iter()
                .filter(|p| p.state == 'D')
                .take(MEMBER_LIMIT)
                .map(|p| p.key)
                .collect();
            if self.reason == reason {
                self.blocked_keys.retain(|key| keys.contains(key));
                if self.blocked_keys.is_empty() {
                    self.pressure_run = 0;
                    self.blocked_keys = keys;
                }
            } else {
                self.blocked_keys = keys;
            }
        } else {
            self.blocked_keys.clear();
        }
        let healthy = cpu.is_some()
            && sample.worker_replacements.is_some()
            && sample.cgroups.iter().all(|c| {
                c.ratio().is_none_or(|r| r < CLEAR_RATIO)
                    && c.event_delta.is_some_and(|e| !e.any())
                    && !c.memory_psi.meaningful(interval)
                    && !c.io_psi.meaningful(interval)
            });
        if healthy {
            self.clear_run = self.clear_run.saturating_add(1);
            if self.latched && self.clear_run >= REQUIRED_SAMPLES {
                self.latched = false;
                self.clear_run = 0;
                self.pressure_run = 0;
                self.reason = None;
                return StallDecision::Cleared;
            }
        } else {
            self.clear_run = 0;
        }
        if progressed || last_completion_elapsed < NO_PROGRESS || reason.is_none() {
            self.pressure_run = 0;
            self.reason = None;
            return StallDecision::Observe;
        }
        let memory_run = matches!(
            self.reason,
            Some(SignalReason::ReclaimPressure | SignalReason::WorkerReplacement)
        ) && matches!(
            reason,
            Some(SignalReason::ReclaimPressure | SignalReason::WorkerReplacement)
        );
        if self.reason == reason || memory_run {
            self.pressure_run = self.pressure_run.saturating_add(1);
            // Replacement adds specificity, not a different stall condition.
            // Once a memory run is mixed, retain its common reclaim reason.
            if self.reason != reason {
                self.reason = Some(SignalReason::ReclaimPressure);
            }
        } else {
            self.pressure_run = 1;
            self.reason = reason;
        }
        if !self.latched && self.pressure_run >= REQUIRED_SAMPLES {
            if let Some(reason) = self.reason {
                self.latched = true;
                return StallDecision::Recover(reason);
            }
        }
        StallDecision::Observe
    }

    fn unknown(&mut self) -> StallDecision {
        self.pressure_run = 0;
        self.clear_run = 0;
        self.reason = None;
        self.blocked_keys.clear();
        StallDecision::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn psi(avg: f64, delta: u64) -> PsiSample {
        let line = PsiLine {
            avg10: avg,
            total_us: delta,
            delta_us: Some(delta),
        };
        PsiSample {
            some: line,
            full: line,
        }
    }

    fn sample(now: Instant) -> ResourceSample {
        ResourceSample {
            now,
            interval: Some(SAMPLE_INTERVAL),
            complete: true,
            ticks_per_second: Some(100),
            worker_replacements: Some(0),
            processes: vec![ProcessSample {
                key: ProcessKey {
                    pid: 12,
                    start_ticks: 10,
                },
                parent_pid: 11,
                state: 'S',
                cpu_ticks: 100,
                cpu_delta_ticks: Some(0),
                rss_bytes: 4096,
            }],
            cgroups: vec![CgroupSample {
                index: 0,
                memory_current: 95,
                memory_high: MemoryLimit::Bytes(100),
                memory_max: MemoryLimit::Unlimited,
                events: MemoryEvents::default(),
                event_delta: Some(MemoryEvents::default()),
                memory_psi: psi(0.0, 0),
                io_psi: psi(0.0, 0),
            }],
        }
    }

    fn update(policy: &mut PressureHysteresis, sample: &ResourceSample) -> StallDecision {
        policy.update(sample, 0, NO_PROGRESS)
    }

    #[test]
    fn finite_unlimited_and_zero_limits() {
        assert_eq!(parse_limit("123\n"), Some(MemoryLimit::Bytes(123)));
        assert_eq!(parse_limit("max\n"), Some(MemoryLimit::Unlimited));
        assert_eq!(parse_limit("0"), Some(MemoryLimit::Bytes(0)));
        assert_eq!(parse_limit("-1"), None);
        let mut s = sample(Instant::now());
        s.cgroups[0].memory_high = MemoryLimit::Bytes(0);
        assert_eq!(s.cgroups[0].ratio(), Some(f64::INFINITY));
        s.cgroups[0].memory_current = 0;
        assert_eq!(s.cgroups[0].ratio(), Some(0.0));
    }

    #[test]
    fn psi_requires_both_lines_and_finite_valid_fields() {
        let valid = "some avg10=1.50 avg60=0.00 avg300=0.00 total=123\nfull avg10=0.00 avg60=0.00 avg300=0.00 total=0\n";
        let p = parse_psi(valid).unwrap();
        assert_eq!(p.some.avg10, 1.5);
        assert_eq!(p.some.total_us, 123);
        assert_eq!(p.some.delta_us, None);
        for invalid in [
            valid.replace("1.50", "NaN"),
            valid.replace("total=123", "total=bad"),
            valid.replace("avg60=0.00", "avg60=-1"),
            valid.lines().next().unwrap().to_owned(),
            valid.replace("avg10=1.50", "avg10=1.50 avg10=2"),
        ] {
            assert!(parse_psi(&invalid).is_none());
        }
    }

    #[test]
    fn stat_uses_last_parenthesis_and_numeric_fields() {
        let stat =
            "12 (a name ) with spaces) D 11 12 12 0 -1 0 0 0 0 0 30 20 0 0 20 0 1 0 77 8192 2";
        let (pgid, p) = parse_stat(12, stat, 4096).unwrap();
        assert_eq!(pgid, 12);
        assert_eq!(p.parent_pid, 11);
        assert_eq!(p.state, 'D');
        assert_eq!(p.cpu_ticks, 50);
        assert_eq!(p.key.start_ticks, 77);
        assert_eq!(p.rss_bytes, 8192);
        assert!(parse_stat(13, stat, 4096).is_none());
        assert!(parse_stat(12, &stat.replace(") D", ") ?"), 4096).is_none());
    }

    #[test]
    fn counter_resets_and_first_samples_have_unknown_deltas() {
        let old = MemoryEvents {
            high: 20,
            max: 3,
            oom: 2,
            oom_kill: 1,
        };
        assert!(MemoryEvents { high: 19, ..old }.delta(old).is_none());
        assert_eq!(MemoryEvents { high: 21, ..old }.delta(old).unwrap().high, 1);
        assert!(parse_events("high 0\nmax 0\noom 0\n").is_none());
        let p = psi(2.0, 100).delta(psi(2.0, 101));
        assert!(p.some.delta_us.is_none());
        let mut s = sample(Instant::now());
        s.cgroups[0].event_delta = None;
        assert_eq!(
            update(&mut PressureHysteresis::new(), &s),
            StallDecision::Unknown
        );
    }

    #[test]
    fn isolated_d_high_or_low_cpu_never_recovers() {
        for kind in 0..3 {
            let mut policy = PressureHysteresis::new();
            let mut s = sample(Instant::now());
            if kind == 0 {
                s.processes[0].state = 'D';
            }
            if kind == 1 {
                s.processes[0].cpu_delta_ticks = Some(500);
            }
            if kind == 2 {
                s.cgroups[0].memory_current = 40;
            }
            for _ in 0..6 {
                assert_eq!(update(&mut policy, &s), StallDecision::Observe);
                s.now += SAMPLE_INTERVAL;
            }
        }
    }

    #[test]
    fn sustained_reclaim_io_and_replacement_require_three_samples() {
        for reason in [
            SignalReason::ReclaimPressure,
            SignalReason::BlockedIo,
            SignalReason::WorkerReplacement,
        ] {
            let mut policy = PressureHysteresis::new();
            let mut s = sample(Instant::now());
            match reason {
                SignalReason::ReclaimPressure => {
                    s.cgroups[0].event_delta.as_mut().unwrap().high = 1
                }
                SignalReason::WorkerReplacement => {
                    s.cgroups[0].memory_psi = psi(2.0, 600_000);
                    s.worker_replacements = Some(1);
                }
                SignalReason::BlockedIo => {
                    s.processes[0].state = 'D';
                    s.cgroups[0].io_psi = psi(2.0, 600_000);
                }
            }
            for _ in 0..2 {
                assert_eq!(update(&mut policy, &s), StallDecision::Observe);
                s.now += SAMPLE_INTERVAL;
            }
            assert_eq!(update(&mut policy, &s), StallDecision::Recover(reason));
            s.now += SAMPLE_INTERVAL;
            assert_eq!(update(&mut policy, &s), StallDecision::Observe);
        }
    }

    #[test]
    fn progress_missing_data_and_gaps_break_runs_and_clear_needs_health() {
        let mut policy = PressureHysteresis::new();
        let mut s = sample(Instant::now());
        s.cgroups[0].event_delta.as_mut().unwrap().high = 1;
        assert_eq!(update(&mut policy, &s), StallDecision::Observe);
        s.now += SAMPLE_INTERVAL;
        assert_eq!(policy.update(&s, 1, NO_PROGRESS), StallDecision::Observe);
        s.now += SAMPLE_INTERVAL;
        s.complete = false;
        assert_eq!(policy.update(&s, 1, NO_PROGRESS), StallDecision::Unknown);
        s.complete = true;
        for i in 0..3 {
            s.now += SAMPLE_INTERVAL;
            let expected = if i == 2 {
                StallDecision::Recover(SignalReason::ReclaimPressure)
            } else {
                StallDecision::Observe
            };
            assert_eq!(policy.update(&s, 1, NO_PROGRESS), expected);
        }
        s.cgroups[0].memory_current = 79;
        s.cgroups[0].event_delta = Some(MemoryEvents::default());
        for i in 0..3 {
            s.now += SAMPLE_INTERVAL;
            assert_eq!(
                policy.update(&s, 1, NO_PROGRESS),
                if i == 2 {
                    StallDecision::Cleared
                } else {
                    StallDecision::Observe
                }
            );
        }
        s.cgroups[0].memory_current = 95;
        s.cgroups[0].event_delta.as_mut().unwrap().high = 1;
        for i in 0..3 {
            s.now += SAMPLE_INTERVAL;
            assert_eq!(
                policy.update(&s, 1, NO_PROGRESS),
                if i == 2 {
                    StallDecision::Recover(SignalReason::ReclaimPressure)
                } else {
                    StallDecision::Observe
                }
            );
        }
        let mut policy = PressureHysteresis::new();
        for _ in 0..3 {
            s.now += Duration::from_secs(90);
            assert_eq!(update(&mut policy, &s), StallDecision::Observe);
        }
    }

    #[test]
    fn no_progress_threshold_and_repeated_timestamp_cannot_trigger() {
        let mut policy = PressureHysteresis::new();
        let mut s = sample(Instant::now());
        s.cgroups[0].event_delta.as_mut().unwrap().high = 1;
        for _ in 0..4 {
            assert_eq!(
                policy.update(&s, 0, Duration::from_secs(179)),
                StallDecision::Observe
            );
            s.now += SAMPLE_INTERVAL;
        }
        for _ in 0..4 {
            assert_eq!(update(&mut policy, &s), StallDecision::Observe);
        }
    }

    #[test]
    fn bounded_reads_reject_truncation_and_directory_overflow() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stat");
        std::fs::write(&path, "12345").unwrap();
        assert!(read_bounded(&path, 4).is_err());
        assert_eq!(read_bounded(&path, 5).unwrap(), "12345");
        let mut sampler = PressureSampler::new(1, 1);
        sampler.set_cgroup_directories(Some(vec![dir.path().to_path_buf(); 33]));
        assert!(sampler.directories_truncated);
        assert_eq!(sampler.directories.as_ref().unwrap().len(), 32);
    }

    #[test]
    fn pid_replacement_starts_new_baseline_without_false_cpu_or_kill_claim() {
        let old = sample(Instant::now());
        let mut current = old.clone();
        current.processes[0].cpu_ticks = 120;
        assert_eq!(cpu_delta(&current.processes[0], Some(&old)), Some(20));
        assert_eq!(replacement_count(&old, &current, 11), 0);
        current.processes[0].key.start_ticks += 1;
        current.processes[0].cpu_ticks = 1;
        assert_eq!(cpu_delta(&current.processes[0], Some(&old)), None);
        assert_eq!(replacement_count(&old, &current, 11), 1);
        assert_eq!(replacement_count(&old, &current, 12), 0);
        let mut reset = old.clone();
        reset.processes[0].cpu_ticks = 99;
        assert_eq!(cpu_delta(&reset.processes[0], Some(&old)), None);
        reset.complete = false;
        assert_eq!(cpu_delta(&current.processes[0], Some(&reset)), None);
        assert_eq!(cpu_delta(&current.processes[0], None), None);
    }

    #[test]
    fn unrelated_transient_d_states_do_not_form_sustained_io() {
        let mut policy = PressureHysteresis::new();
        let mut s = sample(Instant::now());
        s.processes[0].state = 'D';
        s.cgroups[0].io_psi = psi(2.0, 600_000);
        for _ in 0..6 {
            s.processes[0].key.start_ticks += 1;
            assert_eq!(update(&mut policy, &s), StallDecision::Observe);
            s.now += SAMPLE_INTERVAL;
        }
    }

    #[test]
    fn helper_grandchild_churn_preserves_stable_worker_reclaim_run() {
        let mut policy = PressureHysteresis::new();
        let mut old = sample(Instant::now());
        let mut helper = old.processes[0].clone();
        helper.key.pid = 13;
        helper.parent_pid = 12;
        old.processes.push(helper);
        old.cgroups[0].event_delta.as_mut().unwrap().high = 1;
        for observation in 0..4 {
            let mut current = old.clone();
            current.now += SAMPLE_INTERVAL;
            // Alternate helper replacement with a stable-helper observation.
            if observation % 2 == 0 {
                current.processes[1].key.start_ticks += 1;
            }
            current.worker_replacements = Some(replacement_count(&old, &current, 11));
            assert_eq!(current.worker_replacements, Some(0));
            assert_eq!(
                update(&mut policy, &current),
                if observation == 2 {
                    StallDecision::Recover(SignalReason::ReclaimPressure)
                } else {
                    StallDecision::Observe
                }
            );
            old = current;
        }
    }

    #[test]
    fn intermittent_direct_worker_replacement_preserves_memory_pressure_run() {
        for replacement_first in [false, true] {
            let mut policy = PressureHysteresis::new();
            let mut old = sample(Instant::now());
            old.cgroups[0].event_delta.as_mut().unwrap().high = 1;
            for observation in 0..3 {
                let mut current = old.clone();
                current.now += SAMPLE_INTERVAL;
                if (observation % 2 == 0) == replacement_first {
                    current.processes[0].key.start_ticks += 1;
                }
                current.worker_replacements = Some(replacement_count(&old, &current, 11));
                assert_eq!(
                    update(&mut policy, &current),
                    if observation == 2 {
                        StallDecision::Recover(SignalReason::ReclaimPressure)
                    } else {
                        StallDecision::Observe
                    }
                );
                old = current;
            }
        }
    }

    #[test]
    fn unavailable_observations_cannot_clear_a_latched_stall() {
        let mut policy = PressureHysteresis::new();
        let mut s = sample(Instant::now());
        s.cgroups[0].event_delta.as_mut().unwrap().high = 1;
        for _ in 0..3 {
            update(&mut policy, &s);
            s.now += SAMPLE_INTERVAL;
        }
        assert!(policy.latched);
        s.cgroups[0].memory_current = 40;
        s.cgroups[0].event_delta = Some(MemoryEvents::default());
        s.complete = false;
        for _ in 0..3 {
            assert_eq!(update(&mut policy, &s), StallDecision::Unknown);
            s.now += SAMPLE_INTERVAL;
        }
        assert!(policy.latched);
        s.complete = true;
        s.processes[0].cpu_delta_ticks = None;
        for _ in 0..3 {
            assert_eq!(update(&mut policy, &s), StallDecision::Observe);
            s.now += SAMPLE_INTERVAL;
        }
        assert!(policy.latched);
    }

    fn write_cgroup(directory: &Path, high_events: u64) {
        for (name, value) in [
            ("memory.current", "95".to_owned()),
            ("memory.high", "100".to_owned()),
            ("memory.max", "max".to_owned()),
            (
                "memory.events",
                format!("high {high_events}\nmax 0\noom 0\noom_kill 0\n"),
            ),
            (
                "memory.pressure",
                "some avg10=0 avg60=0 avg300=0 total=0\nfull avg10=0 avg60=0 avg300=0 total=0\n"
                    .to_owned(),
            ),
            (
                "io.pressure",
                "some avg10=0 avg60=0 avg300=0 total=0\nfull avg10=0 avg60=0 avg300=0 total=0\n"
                    .to_owned(),
            ),
        ] {
            std::fs::write(directory.join(name), value).unwrap();
        }
    }

    #[test]
    fn controllerless_root_preserves_complete_child_pressure_and_recovery() {
        let root = tempfile::tempdir().unwrap();
        let child = root.path().join("service");
        std::fs::create_dir(&child).unwrap();
        let directories = vec![child.clone(), root.path().to_path_buf()];
        let mut policy = PressureHysteresis::new();
        let mut previous = None;
        let start = Instant::now();
        for observation in 0..4 {
            write_cgroup(&child, observation);
            let (cgroups, complete) = collect_cgroups(
                &directories,
                false,
                previous.as_ref(),
                std::time::Instant::now() + COLLECTION_BUDGET,
            );
            assert!(complete);
            assert_eq!(cgroups.len(), 1);
            assert_eq!(cgroups[0].index, 0);
            assert_eq!(cgroups[0].memory_high, MemoryLimit::Bytes(100));
            let mut s = sample(start + SAMPLE_INTERVAL * observation as u32);
            s.cgroups = cgroups;
            s.complete = complete;
            let expected = match observation {
                0 => StallDecision::Unknown,
                3 => StallDecision::Recover(SignalReason::ReclaimPressure),
                _ => StallDecision::Observe,
            };
            assert_eq!(update(&mut policy, &s), expected);
            previous = Some(s);
        }
    }

    #[test]
    fn missing_nonroot_partial_root_and_vanished_root_remain_incomplete() {
        let root = tempfile::tempdir().unwrap();
        let child = root.path().join("service");
        let missing_controller = root.path().join("nonroot");
        std::fs::create_dir(&child).unwrap();
        std::fs::create_dir(&missing_controller).unwrap();
        write_cgroup(&child, 0);
        let collect = |directories: &[PathBuf], truncated| {
            collect_cgroups(
                directories,
                truncated,
                None,
                std::time::Instant::now() + COLLECTION_BUDGET,
            )
        };
        let (cgroups, complete) = collect(
            &[child.clone(), missing_controller, root.path().to_path_buf()],
            false,
        );
        assert!(!complete);
        assert_eq!(cgroups.len(), 1);
        let mut s = sample(Instant::now());
        s.cgroups = cgroups;
        s.complete = complete;
        assert_eq!(
            update(&mut PressureHysteresis::new(), &s),
            StallDecision::Unknown
        );
        assert!(!collect(&[root.path().to_path_buf()], false).1);
        assert!(!collect(&[child.clone(), root.path().join("vanished")], false).1);
        assert!(!collect(&[child.clone(), root.path().to_path_buf()], true).1);
        std::fs::write(root.path().join("memory.current"), "0").unwrap();
        assert!(!collect(&[child.clone(), root.path().to_path_buf()], false).1);
        // A readable root controller is real ancestor evidence, not omitted.
        write_cgroup(root.path(), 0);
        let (cgroups, complete) = collect(&[child, root.path().to_path_buf()], false);
        assert!(complete);
        assert_eq!(cgroups.len(), 2);
    }
}
