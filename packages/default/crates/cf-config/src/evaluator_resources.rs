//! Resolves evaluator memory policy from one runtime capacity snapshot.
//!
//! Uses capacity, never free memory or RSS. Cgroup limits include all visible
//! ancestors up to the cgroup2 mount root; limits hidden by a cgroup namespace
//! cannot be observed. If an ancestor read fails, detected finite limits still
//! bound the plan; a static warning identifies the incomplete snapshot. Plans
//! are estimates, not kernel memory enforcement.

use crate::ServerConfig;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Identifies the source of the evaluator memory policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvaluatorResourceMode {
    /// Divides the detected capacity budget across resolved workers.
    Auto,
    /// Preserves an explicit per-worker override without automatic sizing.
    Fixed,
    /// Uses the legacy 4096 MiB per worker when no capacity is available.
    Fallback,
}

/// Records a resolved evaluator budget and its capacity provenance in MiB.
///
/// Resolution guarantees positive workers and per-worker memory. Public fields
/// permit inspection; callers that construct plans must preserve those properties.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluatorResourcePlan {
    /// Configured worker count; zero requests runtime CPU detection.
    pub requested_workers: usize,
    /// Explicit positive worker count to pass to nix-eval-jobs 2.34.3.
    pub effective_workers: usize,
    /// Physical MemTotal capacity, rounded down to MiB if detected.
    pub physical_memory_mb: Option<usize>,
    /// Minimum detected finite cgroup-v2 limit, rounded down to MiB. Failed
    /// ancestor reads never discard a finite limit already detected.
    pub cgroup_memory_mb: Option<usize>,
    /// Minimum detected physical and finite cgroup capacities, or sole source.
    pub effective_limit_mb: Option<usize>,
    /// Capacity reserved outside evaluation, in MiB.
    pub reserve_mb: usize,
    /// Maximum percentage of effective capacity available for evaluation.
    pub max_memory_percent: usize,
    /// Total budget before per-worker division; fixed/fallback use exact total.
    pub total_budget_mb: usize,
    /// Positive per-worker MiB, rounded down in automatic mode.
    pub per_worker_mb: usize,
    /// Indicates automatic sizing, explicit override, or legacy fallback.
    pub mode: EvaluatorResourceMode,
}

/// Validates evaluator policy without reading runtime resources.
///
/// Zero workers defer multiplication until CPU resolution. Watchdog durations
/// accept all positive `u64` seconds; callers must use checked deadline addition.
///
/// # Errors
/// Returns a static error for invalid ranges or fixed-budget overflow.
pub fn validate_evaluator_policy(
    workers: usize,
    memory_mb: Option<usize>,
    reserve_mb: usize,
    percent: usize,
    idle_seconds: u64,
    timeout_seconds: u64,
) -> Result<(), String> {
    if let Some(memory) = memory_mb {
        if memory == 0 {
            return Err("eval_max_memory_mb must be greater than 0 when specified".into());
        }
        checked_total(workers, memory)?;
    }
    if reserve_mb == 0 {
        return Err("eval_memory_reserve_mb must be greater than 0".into());
    }
    if !(1..=100).contains(&percent) {
        return Err("eval_memory_max_percent must be between 1 and 100".into());
    }
    if idle_seconds == 0 || timeout_seconds == 0 {
        return Err(
            "eval_output_idle_timeout_secs and eval_overall_timeout_secs must be greater than 0"
                .into(),
        );
    }
    Ok(())
}

fn checked_total(workers: usize, memory: usize) -> Result<usize, String> {
    workers.checked_mul(memory).ok_or_else(|| {
        "evaluation memory budget overflows usize; reduce workers or per-worker memory".into()
    })
}

/// Validates resolved CLI values against the worker and fixed-memory request.
///
/// # Errors
/// Returns an error for mismatched requests, zero values, or product overflow.
pub fn validate_resolved_args(
    workers: usize,
    memory: Option<usize>,
    plan: &EvaluatorResourcePlan,
) -> Result<(), String> {
    if plan.requested_workers != workers
        || plan.effective_workers == 0
        || plan.per_worker_mb == 0
        || (workers != 0 && workers != plan.effective_workers)
        || memory.is_some_and(|value| value != plan.per_worker_mb)
    {
        return Err("evaluator resource plan does not match configured request".into());
    }
    checked_total(plan.effective_workers, plan.per_worker_mb)?;
    Ok(())
}

impl EvaluatorResourcePlan {
    /// Resolves workers and snapshots physical and cgroup capacities once.
    ///
    /// Detection failures emit static warnings without file contents. Incomplete
    /// ancestor reads retain every detected finite cgroup limit. Fixed mode
    /// retains the override even when it exceeds detected capacity. The caller
    /// owns logging the bounded plan summary before spawning the evaluator.
    ///
    /// # Errors
    /// Returns invalid-policy, CPU-detection, overflow, or insufficient-budget
    /// errors. Reduce workers/reserve or supply a positive fixed override when
    /// automatic capacity cannot provide at least one MiB per worker.
    ///
    /// # Examples
    /// ```no_run
    /// use cf_config::{EvaluatorResourcePlan, ServerConfig};
    /// let config = ServerConfig::default();
    /// let plan = EvaluatorResourcePlan::resolve(&config)?;
    /// let args = config.nix_eval_jobs_args(&plan)?;
    /// # Ok::<(), String>(())
    /// ```
    pub fn resolve(config: &ServerConfig) -> Result<Self, String> {
        config.validate_evaluator_policy()?;
        let workers = resolve_workers(config.eval_workers, || {
            std::thread::available_parallelism()
                .map(|value| value.get())
                .map_err(|_| ())
        })?;
        let physical = match read_bounded(Path::new("/proc/meminfo"), 64 * 1024)
            .and_then(|text| parse_mem_total(&text))
        {
            Ok(value) => Some(value),
            Err(_) => {
                tracing::warn!("Evaluator physical memory detection failed");
                None
            }
        };
        let cgroup = match detect_cgroup() {
            Ok(value) => value,
            Err(_) => {
                tracing::warn!("Evaluator cgroup-v2 memory detection failed");
                None
            }
        };
        let plan = Self::from_snapshot(config, workers, physical, cgroup)?;
        if plan.mode == EvaluatorResourceMode::Fallback {
            tracing::warn!("Evaluator capacity unavailable; using legacy 4096 MiB per worker");
        }
        if plan.mode == EvaluatorResourceMode::Fixed
            && plan
                .effective_limit_mb
                .is_some_and(|limit| plan.total_budget_mb > limit)
        {
            tracing::warn!("Evaluator fixed memory budget exceeds detected capacity");
        }
        Ok(plan)
    }

    fn from_snapshot(
        config: &ServerConfig,
        workers: usize,
        physical: Option<usize>,
        cgroup: Option<usize>,
    ) -> Result<Self, String> {
        config.validate_evaluator_policy()?;
        if workers == 0 || (config.eval_workers != 0 && workers != config.eval_workers) {
            return Err(
                "evaluator resolved worker count must be positive and match explicit workers"
                    .into(),
            );
        }
        let limit = match (physical, cgroup) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        let (mode, total, per_worker) = if let Some(memory) = config.eval_max_memory_mb {
            (
                EvaluatorResourceMode::Fixed,
                checked_total(workers, memory)?,
                memory,
            )
        } else if let Some(limit) = limit {
            let remaining = limit.checked_sub(config.eval_memory_reserve_mb).ok_or(
                "evaluator capacity is below reserve; reduce eval_memory_reserve_mb or set a fixed override",
            )?;
            // u128 prevents overflow before division on supported usize widths.
            let percentage =
                ((limit as u128 * config.eval_memory_max_percent as u128) / 100) as usize;
            let total = remaining.min(percentage);
            let memory = total / workers;
            if memory == 0 {
                return Err("evaluator budget cannot provide one MiB per worker; reduce workers/reserve or set a fixed override".into());
            }
            (EvaluatorResourceMode::Auto, total, memory)
        } else {
            (
                EvaluatorResourceMode::Fallback,
                checked_total(workers, 4096)?,
                4096,
            )
        };
        Ok(Self {
            requested_workers: config.eval_workers,
            effective_workers: workers,
            physical_memory_mb: physical,
            cgroup_memory_mb: cgroup,
            effective_limit_mb: limit,
            reserve_mb: config.eval_memory_reserve_mb,
            max_memory_percent: config.eval_memory_max_percent,
            total_budget_mb: total,
            per_worker_mb: per_worker,
            mode,
        })
    }
}

// Bounds cap procfs allocation and reject truncated data rather than interpreting
// a partial snapshot. memory.max is one short scalar; mountinfo can be larger.
fn read_bounded(path: &Path, bound: usize) -> Result<String, String> {
    let file = File::open(path).map_err(|_| "cannot read evaluator capacity source")?;
    let mut bytes = Vec::new();
    file.take(bound as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read evaluator capacity source")?;
    if bytes.len() > bound {
        return Err("evaluator capacity source exceeds read bound".into());
    }
    String::from_utf8(bytes).map_err(|_| "evaluator capacity source is not UTF-8".into())
}

fn unsigned(value: &str) -> Result<u128, String> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("invalid evaluator capacity number".into());
    }
    value
        .parse()
        .map_err(|_| "evaluator capacity number overflows".into())
}

fn parse_mem_total(text: &str) -> Result<usize, String> {
    let mut found = None;
    for line in text.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.first() == Some(&"MemTotal:") {
            if found.is_some() || fields.len() != 3 || fields[2] != "kB" {
                return Err("invalid MemTotal capacity".into());
            }
            found = Some(
                usize::try_from(unsigned(fields[1])? / 1024)
                    .map_err(|_| "physical memory capacity overflows usize")?,
            );
        }
    }
    found.ok_or_else(|| "missing MemTotal capacity".into())
}

fn parse_memory_max(text: &str) -> Result<Option<usize>, String> {
    let value = text.trim();
    if value == "max" {
        return Ok(None);
    }
    usize::try_from(unsigned(value)? / (1024 * 1024))
        .map(Some)
        .map_err(|_| "cgroup memory capacity overflows usize".into())
}

// SECURITY: Reject traversal before Path normalization can hide dot components.
// Kernel procfs paths are trusted inputs, but cannot select outside the mount.
fn absolute_path(value: &str) -> Result<PathBuf, String> {
    if !value.starts_with('/')
        || value.contains('\0')
        || value
            .split('/')
            .skip(1)
            .any(|part| part == "." || part == "..")
    {
        return Err("invalid cgroup absolute path".into());
    }
    Ok(PathBuf::from(value))
}

fn unified_path(text: &str) -> Result<PathBuf, String> {
    let mut found = None;
    for line in text.lines() {
        if let Some(path) = line.strip_prefix("0::") {
            if found.is_some() {
                return Err("duplicate unified cgroup path".into());
            }
            found = Some(absolute_path(path)?);
        }
    }
    found.ok_or_else(|| "missing unified cgroup path".into())
}

fn mount_path(value: &str) -> Result<PathBuf, String> {
    let mut decoded = String::new();
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            decoded.push(ch);
            continue;
        }
        let escape: String = chars.by_ref().take(3).collect();
        decoded.push(match escape.as_str() {
            "040" => ' ',
            "011" => '\t',
            "012" => '\n',
            "134" => '\\',
            _ => return Err("invalid mountinfo path escape".into()),
        });
    }
    absolute_path(&decoded)
}

fn cgroup_mount(text: &str, group: &Path) -> Result<(PathBuf, PathBuf), String> {
    let mut selected = None;
    for line in text.lines() {
        let Some((before, after)) = line.split_once(" - ") else {
            continue;
        };
        if after.split_whitespace().next() != Some("cgroup2") {
            continue;
        }
        let fields: Vec<_> = before.split_whitespace().collect();
        if fields.len() < 6 {
            return Err("invalid cgroup2 mountinfo".into());
        }
        let root = mount_path(fields[3])?;
        let mount = mount_path(fields[4])?;
        let Ok(relative) = group.strip_prefix(&root) else {
            continue;
        };
        let depth = root.components().count();
        // Prefer the broadest visible hierarchy so a subtree bind mount cannot
        // hide finite ancestor limits available through another cgroup2 mount.
        if selected.as_ref().is_none_or(|(old, _, _)| depth < *old) {
            selected = Some((depth, mount.clone(), mount.join(relative)));
        }
    }
    selected
        .map(|(_, mount, path)| (mount, path))
        .ok_or_else(|| "no matching cgroup2 mount".into())
}

fn detect_cgroup() -> Result<Option<usize>, String> {
    let group = unified_path(&read_bounded(Path::new("/proc/self/cgroup"), 64 * 1024)?)?;
    let (mount, path) = cgroup_mount(
        &read_bounded(Path::new("/proc/self/mountinfo"), 1024 * 1024)?,
        &group,
    )?;
    read_cgroup_limits(&mount, path)
}

fn resolve_workers(
    requested: usize,
    detect: impl FnOnce() -> Result<usize, ()>,
) -> Result<usize, String> {
    if requested != 0 {
        return Ok(requested);
    }
    match detect() {
        Ok(value) if value != 0 => Ok(value),
        _ => Err("cannot detect evaluator CPU count; set eval_workers explicitly".into()),
    }
}

fn read_cgroup_limits(mount: &Path, mut path: PathBuf) -> Result<Option<usize>, String> {
    if !path.starts_with(mount) {
        return Err("cgroup path escapes mount root".into());
    }
    // SECURITY: Canonical equality rejects symlink components in the kernel
    // cgroup path. Ancestors are visited only inside this verified mount root.
    // Production sources are kernel cgroupfs, which cannot create symlinks;
    // these checks do not authorize reads from a mutable user-owned hierarchy.
    if std::fs::canonicalize(mount).map_err(|_| "cannot verify cgroup mount")? != mount
        || std::fs::canonicalize(&path).map_err(|_| "cannot verify cgroup path")? != path
    {
        return Err("cgroup path contains symlinks".into());
    }
    let mut limit: Option<usize> = None;
    let mut ancestor_failed = false;
    let mut actual_group = true;
    loop {
        match read_cgroup_limit(mount, &path) {
            Ok(Some(value)) => limit = Some(limit.map_or(value, |old| old.min(value))),
            Ok(None) => {}
            Err(error) if actual_group => return Err(error),
            Err(_) => ancestor_failed = true,
        }
        if path == mount {
            break;
        }
        if !path.pop() || !path.starts_with(mount) {
            return Err("cgroup ancestor escapes mount root".into());
        }
        actual_group = false;
    }
    // INVARIANT: A partial snapshot must never discard a known finite limit
    // and substitute the larger physical capacity. Continue through failed
    // ancestors to retain any smaller limits that remain readable.
    if ancestor_failed {
        if limit.is_none() {
            return Err("cgroup ancestor detection failed without a finite limit".into());
        }
        tracing::warn!(
            "Evaluator cgroup ancestor detection incomplete; retaining detected finite limit"
        );
    }
    Ok(limit)
}

fn read_cgroup_limit(mount: &Path, path: &Path) -> Result<Option<usize>, String> {
    let file = path.join("memory.max");
    if std::fs::symlink_metadata(&file).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err("cgroup memory limit is a symlink".into());
    }
    // The hierarchy root has no memory controller limit file on some kernels.
    if path == mount
        && !file
            .try_exists()
            .map_err(|_| "cannot inspect cgroup root")?
    {
        return Ok(None);
    }
    parse_memory_max(&read_bounded(&file, 128)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(workers: usize, memory: Option<usize>) -> ServerConfig {
        ServerConfig {
            eval_workers: workers,
            eval_max_memory_mb: memory,
            ..ServerConfig::default()
        }
    }

    #[test]
    fn cpu_detection_errors_and_explicit_workers_skip_detection() {
        assert_eq!(resolve_workers(2, || panic!("must not detect")).unwrap(), 2);
        assert_eq!(resolve_workers(0, || Ok(8)).unwrap(), 8);
        assert!(resolve_workers(0, || Ok(0)).is_err());
        assert!(resolve_workers(0, || Err(())).is_err());
    }

    #[test]
    fn visible_ancestor_limits_and_symlink_rejection() {
        let root = tempfile::tempdir().unwrap();
        let parent = root.path().join("parent");
        let child = parent.join("child");
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(parent.join("memory.max"), "8388608").unwrap();
        std::fs::write(child.join("memory.max"), "max").unwrap();
        assert_eq!(
            read_cgroup_limits(root.path(), child.clone()).unwrap(),
            Some(8)
        );
        std::fs::write(child.join("memory.max"), "4194304").unwrap();
        assert_eq!(
            read_cgroup_limits(root.path(), child.clone()).unwrap(),
            Some(4)
        );
        std::fs::write(root.path().join("memory.max"), "2097152").unwrap();
        assert_eq!(
            read_cgroup_limits(root.path(), child.clone()).unwrap(),
            Some(2)
        );
        std::fs::write(parent.join("memory.max"), "invalid").unwrap();
        assert_eq!(
            read_cgroup_limits(root.path(), child.clone()).unwrap(),
            Some(2)
        );
        std::fs::write(parent.join("memory.max"), "max").unwrap();
        std::fs::write(root.path().join("memory.max"), "max").unwrap();
        std::fs::write(child.join("memory.max"), "max").unwrap();
        assert_eq!(
            read_cgroup_limits(root.path(), child.clone()).unwrap(),
            None
        );
        assert!(read_cgroup_limits(&child, parent).is_err());
        #[cfg(unix)]
        {
            let link = root.path().join("link");
            std::os::unix::fs::symlink(&child, &link).unwrap();
            assert!(read_cgroup_limits(root.path(), link).is_err());
            std::fs::remove_file(child.join("memory.max")).unwrap();
            std::os::unix::fs::symlink(root.path().join("memory.max"), child.join("memory.max"))
                .unwrap();
            assert!(read_cgroup_limits(root.path(), child).is_err());
        }
    }

    #[test]
    fn capacity_sources_budget_rounding_and_provenance() {
        let cfg = config(3, None);
        for (physical, cgroup, effective) in [
            (Some(65536), Some(32768), 32768),
            (Some(32768), Some(65536), 32768),
            (None, Some(32768), 32768),
            (Some(32768), None, 32768),
        ] {
            let plan = EvaluatorResourcePlan::from_snapshot(&cfg, 3, physical, cgroup).unwrap();
            assert_eq!(plan.mode, EvaluatorResourceMode::Auto);
            assert_eq!(plan.physical_memory_mb, physical);
            assert_eq!(plan.cgroup_memory_mb, cgroup);
            assert_eq!(plan.effective_limit_mb, Some(effective));
            assert_eq!(plan.total_budget_mb, 27852);
            assert_eq!(plan.per_worker_mb, 9284);
            assert_eq!(plan.reserve_mb, 4096);
            assert_eq!(plan.max_memory_percent, 85);
        }
        let plan = EvaluatorResourcePlan::from_snapshot(&cfg, 3, Some(10001), None).unwrap();
        assert_eq!(plan.total_budget_mb, 5905);
        assert_eq!(plan.per_worker_mb, 1968);
        assert!(plan.per_worker_mb * 3 <= plan.total_budget_mb);
    }

    #[test]
    fn owner_two_worker_auto_and_fixed_budgets_are_exact() {
        let cfg = config(2, None);
        let plan = EvaluatorResourcePlan::from_snapshot(&cfg, 2, Some(65536), Some(49152)).unwrap();
        assert_eq!(plan.mode, EvaluatorResourceMode::Auto);
        assert_eq!(plan.effective_limit_mb, Some(49152));
        assert_eq!(plan.total_budget_mb, 41779);
        assert_eq!(plan.per_worker_mb, 20889);
        let plan = EvaluatorResourcePlan::from_snapshot(&cfg, 2, Some(8192), None).unwrap();
        assert_eq!(plan.total_budget_mb, 4096);
        assert_eq!(plan.per_worker_mb, 2048);
        let cfg = config(2, Some(12288));
        let plan = EvaluatorResourcePlan::from_snapshot(&cfg, 2, Some(65536), Some(49152)).unwrap();
        assert_eq!(plan.mode, EvaluatorResourceMode::Fixed);
        assert_eq!(plan.total_budget_mb, 24576);
        assert_eq!(plan.per_worker_mb, 12288);
        assert_eq!(
            cfg.nix_eval_jobs_args(&plan).unwrap(),
            [
                "--workers",
                "2",
                "--max-memory-size",
                "12288",
                "--check-cache-status",
            ]
        );
    }

    #[test]
    fn finite_actual_group_survives_missing_malformed_and_unreadable_ancestors() {
        let root = tempfile::tempdir().unwrap();
        let parent = root.path().join("parent");
        let child = parent.join("child");
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(child.join("memory.max"), (49152_u64 * 1048576).to_string()).unwrap();
        // Missing parent file is a deterministic ancestor read failure.
        let limit = read_cgroup_limits(root.path(), child.clone()).unwrap();
        assert_eq!(limit, Some(49152));
        let plan =
            EvaluatorResourcePlan::from_snapshot(&config(2, None), 2, Some(65536), limit).unwrap();
        assert_eq!(plan.effective_limit_mb, Some(49152));
        assert_eq!(plan.total_budget_mb, 41779);
        std::fs::write(parent.join("memory.max"), "malformed").unwrap();
        assert_eq!(
            read_cgroup_limits(root.path(), child.clone()).unwrap(),
            Some(49152)
        );
        std::fs::remove_file(parent.join("memory.max")).unwrap();
        std::fs::create_dir(parent.join("memory.max")).unwrap();
        assert_eq!(
            read_cgroup_limits(root.path(), child.clone()).unwrap(),
            Some(49152)
        );
        std::fs::write(
            root.path().join("memory.max"),
            (32768_u64 * 1048576).to_string(),
        )
        .unwrap();
        assert_eq!(
            read_cgroup_limits(root.path(), child.clone()).unwrap(),
            Some(32768)
        );
        // The actual group must be readable even if an ancestor is finite.
        std::fs::remove_file(child.join("memory.max")).unwrap();
        assert!(read_cgroup_limits(root.path(), child.clone()).is_err());
        std::fs::write(child.join("memory.max"), "malformed").unwrap();
        assert!(read_cgroup_limits(root.path(), child.clone()).is_err());
        std::fs::write(child.join("memory.max"), "max").unwrap();
        std::fs::write(root.path().join("memory.max"), "max").unwrap();
        assert!(read_cgroup_limits(root.path(), child).is_err());
    }

    #[test]
    fn serde_owner_timeout_names_are_shared_and_static() {
        for (idle, overall) in [(17, 23), (0, 23), (17, 0), (u64::MAX, u64::MAX)] {
            let value = serde_json::json!({
                "host": "127.0.0.1", "port": 3000, "eval_workers": 0,
                "eval_output_idle_timeout_secs": idle,
                "eval_overall_timeout_secs": overall,
            });
            let current: ServerConfig = serde_json::from_value(value.clone()).unwrap();
            let old: crate::obsolete_server_config::ServerConfig =
                serde_json::from_value(value).unwrap();
            assert_eq!(current.eval_output_idle_timeout_secs, idle);
            assert_eq!(old.eval_output_idle_timeout_secs, idle);
            assert_eq!(current.eval_overall_timeout_secs, overall);
            assert_eq!(old.eval_overall_timeout_secs, overall);
            assert_eq!(current.eval_workers, 0);
            assert_eq!(old.eval_workers, 0);
            assert_eq!(current.eval_max_memory_mb, None);
            assert_eq!(old.eval_max_memory_mb, None);
            assert_eq!(
                current.validate_evaluator_policy(),
                old.validate_evaluator_policy()
            );
            assert_eq!(
                current.validate_evaluator_policy().is_ok(),
                idle > 0 && overall > 0
            );
            if idle == 0 || overall == 0 {
                assert_eq!(
                    current.validate_evaluator_policy().unwrap_err(),
                    "eval_output_idle_timeout_secs and eval_overall_timeout_secs must be greater than 0"
                );
            }
        }
    }

    #[test]
    fn fixed_preserves_legacy_even_above_detected_capacity() {
        let cfg = config(4, Some(12288));
        cfg.validate().unwrap(); // No former 32768 MiB ceiling.
        let plan = EvaluatorResourcePlan::from_snapshot(&cfg, 4, Some(8192), Some(4096)).unwrap();
        assert_eq!(plan.mode, EvaluatorResourceMode::Fixed);
        assert_eq!(plan.per_worker_mb, 12288);
        assert_eq!(plan.total_budget_mb, 49152);
        assert_eq!(plan.effective_limit_mb, Some(4096));
        assert_eq!(
            cfg.nix_eval_jobs_args(&plan).unwrap(),
            [
                "--workers",
                "4",
                "--max-memory-size",
                "12288",
                "--check-cache-status",
            ]
        );
    }

    #[test]
    fn fallback_and_resolved_zero_worker_request() {
        let cfg = config(0, None);
        let plan = EvaluatorResourcePlan::from_snapshot(&cfg, 7, None, None).unwrap();
        assert_eq!(plan.mode, EvaluatorResourceMode::Fallback);
        assert_eq!(plan.requested_workers, 0);
        assert_eq!(plan.effective_workers, 7);
        assert_eq!(plan.total_budget_mb, 28672);
        assert_eq!(plan.per_worker_mb, 4096);
        assert_eq!(plan.effective_limit_mb, None);
        assert_eq!(cfg.nix_eval_jobs_args(&plan).unwrap()[1], "7");
        assert!(EvaluatorResourcePlan::from_snapshot(&cfg, 0, None, None).is_err());
        assert!(EvaluatorResourcePlan::from_snapshot(&config(2, None), 3, None, None).is_err());
    }

    #[test]
    fn arithmetic_extremes_and_insufficient_budget() {
        for limit in [0, 4095, 4096, 4097] {
            assert!(
                EvaluatorResourcePlan::from_snapshot(&config(2, None), 2, Some(limit), None)
                    .is_err()
            );
        }
        let cfg = ServerConfig {
            eval_memory_reserve_mb: 1,
            ..config(1, None)
        };
        let plan = EvaluatorResourcePlan::from_snapshot(&cfg, 1, Some(usize::MAX), None).unwrap();
        assert_eq!(
            plan.total_budget_mb,
            ((usize::MAX as u128 * 85) / 100) as usize
        );
        assert!(
            EvaluatorResourcePlan::from_snapshot(&config(usize::MAX, None), usize::MAX, None, None)
                .is_err()
        );
        assert!(
            config(2, Some(usize::MAX))
                .validate_evaluator_policy()
                .is_err()
        );
        let cfg = config(0, Some(usize::MAX));
        cfg.validate_evaluator_policy().unwrap();
        assert!(EvaluatorResourcePlan::from_snapshot(&cfg, 2, None, None).is_err());
        assert!(EvaluatorResourcePlan::from_snapshot(&cfg, 1, None, None).is_ok());
    }

    #[test]
    fn static_policy_ranges_no_arbitrary_upper_limits() {
        for memory in [None, Some(12288), Some(usize::MAX)] {
            validate_evaluator_policy(1, memory, usize::MAX, 100, u64::MAX, u64::MAX).unwrap();
        }
        for args in [
            (Some(0), 4096, 85, 900, 3600),
            (None, 0, 85, 900, 3600),
            (None, 4096, 0, 900, 3600),
            (None, 4096, 101, 900, 3600),
            (None, 4096, 85, 0, 3600),
            (None, 4096, 85, 900, 0),
        ] {
            assert!(validate_evaluator_policy(2, args.0, args.1, args.2, args.3, args.4).is_err());
        }
    }

    #[test]
    fn parsers_floor_units_and_reject_malformed_or_overflowing_inputs() {
        assert_eq!(
            parse_mem_total("MemFree: 1 kB\nMemTotal: 2049 kB\n").unwrap(),
            2
        );
        for text in [
            "",
            "MemTotal: 1 MB",
            "MemTotal: -1 kB",
            "MemTotal: +1 kB",
            "MemTotal: 2 kB extra",
            "MemTotal: 2 kB\nMemTotal: 3 kB",
        ] {
            assert!(parse_mem_total(text).is_err());
        }
        assert_eq!(parse_memory_max("max\n").unwrap(), None);
        assert_eq!(parse_memory_max("2097153\n").unwrap(), Some(2));
        assert_eq!(parse_memory_max("0").unwrap(), Some(0));
        assert_eq!(parse_memory_max("1048575").unwrap(), Some(0));
        for text in [
            "",
            "MAX",
            "-1",
            "+1",
            "1 2",
            "1.0",
            "340282366920938463463374607431768211456",
        ] {
            assert!(parse_memory_max(text).is_err());
        }
        let overflow = (usize::MAX as u128 + 1) * 1048576;
        assert!(parse_memory_max(&overflow.to_string()).is_err());
        assert!(
            parse_mem_total(&format!("MemTotal: {} kB", (usize::MAX as u128 + 1) * 1024)).is_err()
        );
    }

    #[test]
    fn cgroup_paths_mount_roots_containers_and_escapes() {
        assert_eq!(
            unified_path("1:cpu:/other\n0::/tenant/service\n").unwrap(),
            Path::new("/tenant/service")
        );
        for text in [
            "1:memory:/",
            "0::relative",
            "0::/../outside",
            "0::/a/./b",
            "0::/a\n0::/b",
        ] {
            assert!(unified_path(text).is_err());
        }
        let mounts = "1 0 0:1 / /sys/fs/cgroup rw - cgroup2 cgroup rw\n2 0 0:1 /tenant /container/cgroup rw - cgroup2 cgroup rw";
        assert_eq!(
            cgroup_mount(mounts, Path::new("/tenant/service")).unwrap(),
            (
                PathBuf::from("/sys/fs/cgroup"),
                PathBuf::from("/sys/fs/cgroup/tenant/service")
            )
        );
        assert_eq!(
            cgroup_mount(
                "2 0 0:1 /tenant /container/cgroup rw - cgroup2 cgroup rw",
                Path::new("/tenant/service")
            )
            .unwrap(),
            (
                PathBuf::from("/container/cgroup"),
                PathBuf::from("/container/cgroup/service")
            )
        );
        assert!(
            cgroup_mount(
                "1 0 0:1 /other /sys/fs/cgroup rw - cgroup2 cgroup rw",
                Path::new("/tenant")
            )
            .is_err()
        );
        assert_eq!(
            mount_path("/space\\040mount").unwrap(),
            Path::new("/space mount")
        );
        for path in ["/bad\\999", "/bad\\04", "/a/../b", "/a/./b", "/nul\0"] {
            assert!(mount_path(path).is_err());
        }
    }

    #[test]
    fn bounded_reads_reject_truncation_and_invalid_utf8() {
        use std::io::Write;
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(b"12345").unwrap();
        assert_eq!(read_bounded(file.path(), 5).unwrap(), "12345");
        assert!(read_bounded(file.path(), 4).is_err());
        file.write_all(&[255]).unwrap();
        assert!(read_bounded(file.path(), 6).is_err());
    }

    #[test]
    fn serde_and_obsolete_copy_share_evaluator_contract() {
        for memory in [None, Some(12288), Some(0)] {
            let mut value = serde_json::json!({"host":"127.0.0.1", "port":3000});
            if let Some(memory) = memory {
                value["eval_max_memory_mb"] = memory.into();
            }
            let current: ServerConfig = serde_json::from_value(value.clone()).unwrap();
            let old: crate::obsolete_server_config::ServerConfig =
                serde_json::from_value(value).unwrap();
            assert_eq!(old.bind_address(), current.bind_address());
            assert_eq!(old.execution_mode.as_str(), current.execution_mode.as_str());
            assert_eq!(old.allow_registration, current.allow_registration);
            assert_eq!(
                old.commit_cache_retention_days,
                current.commit_cache_retention_days
            );
            assert_eq!(
                old.allow_private_cache_test_targets,
                current.allow_private_cache_test_targets
            );
            assert_eq!(current.eval_max_memory_mb, memory);
            assert_eq!(old.eval_max_memory_mb, memory);
            assert_eq!(current.eval_workers, old.eval_workers);
            assert_eq!(current.eval_memory_reserve_mb, 4096);
            assert_eq!(current.eval_memory_reserve_mb, old.eval_memory_reserve_mb);
            assert_eq!(current.eval_memory_max_percent, 85);
            assert_eq!(current.eval_memory_max_percent, old.eval_memory_max_percent);
            assert_eq!(current.eval_output_idle_timeout_secs, 900);
            assert_eq!(
                current.eval_output_idle_timeout_secs,
                old.eval_output_idle_timeout_secs
            );
            assert_eq!(current.eval_overall_timeout_secs, 3600);
            assert_eq!(
                current.eval_overall_timeout_secs,
                old.eval_overall_timeout_secs
            );
            assert_eq!(
                current.validate_evaluator_policy(),
                old.validate_evaluator_policy()
            );
            if memory != Some(0) {
                let plan =
                    EvaluatorResourcePlan::from_snapshot(&current, 2, Some(65536), None).unwrap();
                assert_eq!(
                    current.nix_eval_jobs_args(&plan),
                    old.nix_eval_jobs_args(&plan)
                );
            }
        }
        let mut current = config(4, Some(12288));
        let mut old = crate::obsolete_server_config::ServerConfig::default();
        old.eval_workers = 4;
        old.eval_max_memory_mb = Some(12288);
        current.validate().unwrap();
        old.validate().unwrap();
        current.eval_memory_max_percent = 101;
        old.eval_memory_max_percent = 101;
        assert_eq!(
            current.validate_evaluator_policy(),
            old.validate_evaluator_policy()
        );
    }

    #[test]
    fn cli_rejects_invalid_or_mismatched_public_plans() {
        let cfg = config(2, Some(12288));
        let plan = EvaluatorResourcePlan::from_snapshot(&cfg, 2, None, None).unwrap();
        let mut invalid = plan.clone();
        invalid.effective_workers = 0;
        assert!(cfg.nix_eval_jobs_args(&invalid).is_err());
        invalid = plan.clone();
        invalid.per_worker_mb = 0;
        assert!(cfg.nix_eval_jobs_args(&invalid).is_err());
        assert!(config(3, Some(12288)).nix_eval_jobs_args(&plan).is_err());
        assert!(config(2, Some(4096)).nix_eval_jobs_args(&plan).is_err());
        let mut no_cache = cfg;
        no_cache.eval_check_cache = false;
        assert_eq!(no_cache.nix_eval_jobs_args(&plan).unwrap().len(), 4);
    }
}
