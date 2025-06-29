use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::fmt;
use std::option::Option;
use std::{fs, io::ErrorKind, path::Path, process::Command};
use sysinfo::System;
use tracing::debug;

// Assuming your UserType enum looks like this
#[derive(Debug, sqlx::Type)]
#[sqlx(type_name = "user_type", rename_all = "lowercase")]
pub enum UserType {
    Human,
    Service,
    Bot,
    System,
}

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct Users {
    pub id: Uuid,
    pub username: String,
    pub first_name: String,
    pub last_name: String,
    pub email: String,
    pub user_type: String,
    pub is_active: bool,
    pub created_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
