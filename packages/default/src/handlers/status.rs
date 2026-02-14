use axum::{extract::State, response::Json};
use serde_json::{Value, json};

use crate::handlers::agent_request::CFState;
use crate::queries::status::{check_database_health, get_basic_stats};

pub async fn status(State(state): State<CFState>) -> Json<Value> {
    let db_status = if check_database_health(state.pool()).await {
        "healthy"
    } else {
        "unhealthy"
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
