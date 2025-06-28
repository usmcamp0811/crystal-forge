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
pub struct Environment {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub r#type: Option<String>, // tier/type like 'sandbox', 'regulated'
    pub is_active: bool,
    pub created_by: Option<String>,
    pub updated_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
