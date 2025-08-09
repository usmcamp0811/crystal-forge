use axum::{extract::State, response::Json};
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::handlers::agent_request::CFState;

pub async fn status(State(state): State<CFState>) -> Json<Value> {
    let db_status = match sqlx::query("SELECT 1 as health_check")
        .fetch_one(state.pool())
        .await
    {
        Ok(_) => "healthy",
        Err(_) => "unhealthy",
    };

    let (total_systems, total_derivations, pending_evaluations) =
        get_basic_stats(state.pool()).await;

    Json(json!({
        "service": "Crystal Forge",
        "status": "running",
        "database": db_status,
        "stats": {
            "total_systems": total_systems,
            "total_derivations": total_derivations,
            "pending_evaluations": pending_evaluations
        },
        "timestamp": chrono::Utc::now().to_rfc3339()
    }))
}

async fn get_basic_stats(pool: &PgPool) -> (i64, i64, i64) {
    let systems_count = sqlx::query_scalar!("SELECT COUNT(*) FROM systems")
        .fetch_one(pool)
        .await
        .unwrap_or(Some(0))
        .unwrap_or(0);

    let derivations_count = sqlx::query_scalar!("SELECT COUNT(*) FROM derivations")
        .fetch_one(pool)
        .await
        .unwrap_or(Some(0))
        .unwrap_or(0);

    let pending_count = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM derivations d 
         JOIN derivation_statuses ds ON d.status_id = ds.id 
         WHERE ds.is_terminal = false"
    )
    .fetch_one(pool)
    .await
    .unwrap_or(Some(0))
    .unwrap_or(0);

    (systems_count, derivations_count, pending_count)
}
