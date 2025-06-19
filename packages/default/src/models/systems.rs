use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct SystemState {
    pub id: i32,
    pub hostname: String,
    pub system_derivation_id: String,
    pub context: String,
    pub os: Option<String>,
    pub kernel: Option<String>,
    pub memory_gb: Option<f64>,
    pub uptime_secs: Option<i64>,
    pub cpu_brand: Option<String>,
    pub cpu_cores: Option<i32>,
    pub board_serial: Option<String>,
    pub product_uuid: Option<String>,
    pub rootfs_uuid: Option<String>,
}

impl SystemState {
    pub fn gather(hostname: &str, context: &str, system_derivation_id: &str) -> Result<Self> {
        let mut sys = System::new_all();
        sys.refresh_all();

        debug!("🔍 reading uptime_secs");
        let uptime_secs = System::uptime();

        debug!("🔍 reading os");
        let os = System::os_version().unwrap_or_else(|| "unknown".to_string());
        debug!("🔍 reading kernel");
        let kernel = System::kernel_version().unwrap_or_else(|| "unknown".to_string());

        debug!("🔍 reading memory_gb");
        let memory_gb = sys.total_memory() as f64 / 1024.0 / 1024.0;
        debug!("🔍 reading cpu_brand");
        let cpu_brand = sys
            .cpus()
            .get(0)
            .map(|c| c.brand().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let cpu_cores = sys.cpus().len();

        debug!("🔍 reading board_serial");
        let board_serial = fs::read_to_string("/sys/class/dmi/id/board_serial")
            .map(|s| Some(s.trim().to_string()))
            .or_else(|e| {
                eprintln!("[fingerprint] board_serial read error: {:?}", e);
                if matches!(e.kind(), ErrorKind::PermissionDenied | ErrorKind::NotFound) {
                    Ok(None)
                } else {
                    Err(e)
                }
            })?;

        debug!("🔍 reading product_uuid");
        let product_uuid = fs::read_to_string("/sys/class/dmi/id/product_uuid")
            .map(|s| Some(s.trim().to_string()))
            .or_else(|e| {
                eprintln!("[fingerprint] product_uuid read error: {:?}", e);
                if matches!(e.kind(), ErrorKind::PermissionDenied | ErrorKind::NotFound) {
                    Ok(None)
                } else {
                    Err(e)
                }
            })?;

        debug!("🔍 reading rootfs_uuid");
        let rootfs_uuid = get_rootfs_uuid();

        Ok(SystemState {
            id: 0,
            hostname: hostname.to_string(),
            system_derivation_id: system_derivation_id.to_string(),
            context: context.to_string(),
            os,
            kernel,
            memory_gb,
            uptime_secs,
            cpu_brand,
            cpu_cores,
            board_serial,
            product_uuid,
            rootfs_uuid,
        })
    }
}
