use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::fmt;
use std::option::Option;
use std::{fs, io::ErrorKind, path::Path, process::Command};
use sysinfo::System;
use tracing::debug;

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct System {
    pub id: Uuid,
    pub hostname: String,
    pub environment_id: Option<Uuid>,
    pub is_active: bool,
    pub public_key: String,
    pub flake_id: Option<Uuid>,
    pub derivation: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub impl System {
    pub async fn get_by_hostname(pool: &PgPool, hostname: &str) -> Result<Option<System>> {
        let system = sqlx::query_as::<_, System>("SELECT * FROM tbl_systems WHERE hostname = $1")
            .bind(hostname)
            .fetch_optional(pool)
            .await?;
        Ok(system)
    }

    pub async fn get_by_id(pool: &PgPool, id: Uuid) -> Result<Option<System>> {
        let system = sqlx::query_as::<_, System>("SELECT * FROM tbl_systems WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?;
        Ok(system)
    }
}
