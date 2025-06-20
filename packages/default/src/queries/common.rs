use crate::db::get_db_pool;
use crate::sys_fingerprint::FingerprintParts;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::{Row, postgres::PgPool};
use tracing::{debug, info, warn};


