use anyhow::Result;
use base64::Engine;
use base64::engine::general_purpose;
use chrono::{DateTime, Utc};
use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::option::Option;
use uuid::Uuid;

use crate::models::public_key::PublicKey;

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct System {
    pub id: Uuid,
    pub hostname: String,
    pub environment_id: Option<Uuid>,
    pub is_active: bool,
    pub public_key: PublicKey,
    pub flake_id: Option<i32>,
    pub derivation: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
