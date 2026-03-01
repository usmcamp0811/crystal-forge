use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use tracing::warn;

/// System resource metrics for builder capacity tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    /// CPU usage percentage (0.0-100.0)
    pub cpu_usage_percent: Option<f64>,

    /// Memory usage percentage (0.0-100.0)
    pub memory_usage_percent: Option<f64>,

    /// Total system memory in MB
    pub memory_total_mb: Option<i64>,

    /// Used system memory in MB
    pub memory_used_mb: Option<i64>,

    /// Available disk space in bytes
    pub disk_available_bytes: Option<i64>,

    /// Total disk space in bytes
    pub disk_total_bytes: Option<i64>,

    /// Number of active build jobs
    pub active_jobs: i32,
}

impl SystemMetrics {
    /// Collect current system metrics
    pub async fn collect(active_jobs: i32) -> Self {
        let memory_totals = Self::get_memory_totals_mb().await;
        Self {
            cpu_usage_percent: Self::get_cpu_usage().await,
            memory_usage_percent: Self::get_memory_usage().await,
            memory_total_mb: memory_totals.map(|(total, _)| total),
            memory_used_mb: memory_totals.map(|(_, used)| used),
            disk_available_bytes: Self::get_disk_available().await,
            disk_total_bytes: Self::get_disk_total().await,
            active_jobs,
        }
    }

    /// Get CPU usage percentage (Linux-specific via /proc/stat)
    async fn get_cpu_usage() -> Option<f64> {
        // Simple CPU usage approximation using /proc/stat
        // This reads two snapshots 100ms apart and calculates the difference
        let stat1 = Self::read_proc_stat()?;
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        let stat2 = Self::read_proc_stat()?;

        let idle_delta = stat2.idle - stat1.idle;
        let total_delta = stat2.total - stat1.total;

        if total_delta == 0 {
            return None;
        }

        let usage = 100.0 * (1.0 - (idle_delta as f64 / total_delta as f64));
        Some(usage.max(0.0).min(100.0))
    }

    /// Get memory usage percentage (Linux-specific via /proc/meminfo)
    async fn get_memory_usage() -> Option<f64> {
        let meminfo = fs::read_to_string("/proc/meminfo").ok()?;

        let mut total: Option<u64> = None;
        let mut available: Option<u64> = None;

        for line in meminfo.lines() {
            if line.starts_with("MemTotal:") {
                total = line.split_whitespace().nth(1)?.parse().ok();
            } else if line.starts_with("MemAvailable:") {
                available = line.split_whitespace().nth(1)?.parse().ok();
            }
        }

        let total = total?;
        let available = available?;

        if total == 0 {
            return None;
        }

        let used = total.saturating_sub(available);
        let usage = 100.0 * (used as f64 / total as f64);
        Some(usage.max(0.0).min(100.0))
    }

    /// Get memory totals in MB (total, used) from /proc/meminfo.
    async fn get_memory_totals_mb() -> Option<(i64, i64)> {
        let meminfo = fs::read_to_string("/proc/meminfo").ok()?;

        let mut total_kb: Option<u64> = None;
        let mut available_kb: Option<u64> = None;

        for line in meminfo.lines() {
            if line.starts_with("MemTotal:") {
                total_kb = line.split_whitespace().nth(1)?.parse().ok();
            } else if line.starts_with("MemAvailable:") {
                available_kb = line.split_whitespace().nth(1)?.parse().ok();
            }
        }

        let total_kb = total_kb?;
        let available_kb = available_kb?;
        let used_kb = total_kb.saturating_sub(available_kb);

        Some(((total_kb / 1024) as i64, (used_kb / 1024) as i64))
    }

    /// Get available disk space for /nix/store
    async fn get_disk_available() -> Option<i64> {
        Self::get_disk_space("/nix/store").map(|(available, _)| available)
    }

    /// Get total disk space for /nix/store
    async fn get_disk_total() -> Option<i64> {
        Self::get_disk_space("/nix/store").map(|(_, total)| total)
    }

    /// Get disk space statistics for a path using statvfs
    fn get_disk_space(path: &str) -> Option<(i64, i64)> {
        use std::os::unix::fs::MetadataExt;

        // Use nix crate for statvfs if available, otherwise estimate from metadata
        // For now, we'll use a simple approach
        let metadata = fs::metadata(path).ok()?;

        // This is a placeholder - in production you'd use nix::sys::statvfs::statvfs
        // For now we'll return None to avoid platform-specific code
        None
    }

    /// Read CPU statistics from /proc/stat
    fn read_proc_stat() -> Option<CpuStat> {
        let stat = fs::read_to_string("/proc/stat").ok()?;
        let line = stat.lines().next()?;

        if !line.starts_with("cpu ") {
            return None;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 5 {
            return None;
        }

        let user: u64 = parts[1].parse().ok()?;
        let nice: u64 = parts[2].parse().ok()?;
        let system: u64 = parts[3].parse().ok()?;
        let idle: u64 = parts[4].parse().ok()?;

        let total = user + nice + system + idle;

        Some(CpuStat { idle, total })
    }
}

#[derive(Debug, Clone, Copy)]
struct CpuStat {
    idle: u64,
    total: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_collect_metrics() {
        let metrics = SystemMetrics::collect(2).await;

        // Active jobs should always be set
        assert_eq!(metrics.active_jobs, 2);

        // Other metrics may or may not be available depending on the platform
        // Just verify they don't panic
        println!("CPU: {:?}", metrics.cpu_usage_percent);
        println!("Memory: {:?}", metrics.memory_usage_percent);
        println!("Disk available: {:?}", metrics.disk_available_bytes);
        println!("Disk total: {:?}", metrics.disk_total_bytes);
    }

    #[tokio::test]
    async fn test_memory_usage_on_linux() {
        if Path::new("/proc/meminfo").exists() {
            let usage = SystemMetrics::get_memory_usage().await;
            if let Some(usage) = usage {
                assert!(usage >= 0.0 && usage <= 100.0);
            }
        }
    }

    #[tokio::test]
    async fn test_cpu_usage_on_linux() {
        if Path::new("/proc/stat").exists() {
            let usage = SystemMetrics::get_cpu_usage().await;
            if let Some(usage) = usage {
                assert!(usage >= 0.0 && usage <= 100.0);
            }
        }
    }
}
