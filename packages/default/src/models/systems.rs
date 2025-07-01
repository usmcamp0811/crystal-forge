use crate::models::public_key::PublicKey;
use crate::queries::systems::insert_system;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use sqlx::PgPool;
use std::option::Option;
use uuid::Uuid;

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

impl System {
    /// Creates a new validated System instance and inserts it using `insert_system()`.
    /// UUID & derivation are left for the database or later processing.
    pub async fn new(
        pool: &PgPool,
        hostname: String,
        environment_id: Option<Uuid>,
        is_active: bool,
        public_key_base64: String,
        flake_id: Option<i32>,
    ) -> Result<System> {
        let public_key = PublicKey::from_base64(&public_key_base64, &hostname)?;

        let system = System {
            id: Uuid::nil(), // placeholder; DB will assign real UUID
            hostname,
            environment_id,
            is_active,
            public_key,
            flake_id,
            derivation: "".into(), // leave empty; DB or later logic sets it
            created_at: chrono::Utc::now(), // placeholder; overwritten by DB
            updated_at: chrono::Utc::now(), // placeholder; overwritten by DB
        };

        insert_system(pool, &system).await
    }
}
