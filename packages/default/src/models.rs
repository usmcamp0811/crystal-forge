use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct Flake {
    pub id: i32,
    pub name: String,
    pub repo_url: String,
}



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
