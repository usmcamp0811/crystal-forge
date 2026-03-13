use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use sqlx::PgPool;

use crate::api::models::{
    ApiError, SetupWizardAcknowledgeAgentRequest, SetupWizardDismissRequest,
    SetupWizardProgressResponse, SetupWizardStepStatus,
};
use crate::handlers::api::rbac::require_admin as require_admin_user;
use crate::queries::users::{
    get_setup_wizard_agent_acknowledged, get_setup_wizard_dismissed,
    set_setup_wizard_agent_acknowledged, set_setup_wizard_dismissed,
};

#[derive(Debug, Clone, Copy)]
struct SetupWizardCounts {
    environment: i64,
    flake: i64,
    builder_with_environment: i64,
    cache_with_environment: i64,
    system_with_links: i64,
}

pub async fn get_setup_progress(State(pool): State<PgPool>, headers: HeaderMap) -> impl IntoResponse {
    let Some(user_id) = require_admin_user(&pool, &headers).await else {
        return forbidden();
    };

    let counts = match load_counts(&pool).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to query setup progress"),
    };

    let dismissed = match get_setup_wizard_dismissed(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load setup wizard state"),
    };

    let agent_acknowledged = match get_setup_wizard_agent_acknowledged(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load setup wizard state"),
    };

    let response = build_progress_response(counts, dismissed, agent_acknowledged);
    (StatusCode::OK, Json(response)).into_response()
}

pub async fn dismiss_setup_wizard(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(payload): Json<SetupWizardDismissRequest>,
) -> impl IntoResponse {
    let Some(user_id) = require_admin_user(&pool, &headers).await else {
        return forbidden();
    };

    if set_setup_wizard_dismissed(&pool, user_id, payload.dismissed)
        .await
        .is_err()
    {
        return internal_error("Failed to update setup wizard dismissal");
    }

    StatusCode::NO_CONTENT.into_response()
}

pub async fn acknowledge_agent_step(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(payload): Json<SetupWizardAcknowledgeAgentRequest>,
) -> impl IntoResponse {
    let Some(user_id) = require_admin_user(&pool, &headers).await else {
        return forbidden();
    };

    if set_setup_wizard_agent_acknowledged(&pool, user_id, payload.acknowledged)
        .await
        .is_err()
    {
        return internal_error("Failed to update setup wizard agent acknowledgment");
    }

    StatusCode::NO_CONTENT.into_response()
}

async fn load_counts(pool: &PgPool) -> anyhow::Result<SetupWizardCounts> {
    let environment = sqlx::query_scalar::<_, i64>("SELECT COUNT(*)::bigint FROM environments")
        .fetch_one(pool)
        .await?;

    let flake = sqlx::query_scalar::<_, i64>("SELECT COUNT(*)::bigint FROM flakes")
        .fetch_one(pool)
        .await?;

    let builder_with_environment = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(DISTINCT b.id)::bigint FROM builders b JOIN builder_environment_assignments bea ON bea.builder_id = b.id",
    )
    .fetch_one(pool)
    .await?;

    let cache_with_environment = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(DISTINCT c.id)::bigint FROM cache_destinations c JOIN cache_destination_environments cde ON cde.cache_destination_id = c.id",
    )
    .fetch_one(pool)
    .await?;

    let system_with_links = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)::bigint FROM systems WHERE environment_id IS NOT NULL AND flake_id IS NOT NULL",
    )
    .fetch_one(pool)
    .await?;

    Ok(SetupWizardCounts {
        environment,
        flake,
        builder_with_environment,
        cache_with_environment,
        system_with_links,
    })
}

fn build_progress_response(
    counts: SetupWizardCounts,
    dismissed: bool,
    agent_acknowledged: bool,
) -> SetupWizardProgressResponse {
    let environment = SetupWizardStepStatus {
        complete: counts.environment > 0,
        count: counts.environment,
    };
    let flake = SetupWizardStepStatus {
        complete: counts.flake > 0,
        count: counts.flake,
    };
    let builder = SetupWizardStepStatus {
        complete: counts.builder_with_environment > 0,
        count: counts.builder_with_environment,
    };
    let cache = SetupWizardStepStatus {
        complete: counts.cache_with_environment > 0,
        count: counts.cache_with_environment,
    };
    let system = SetupWizardStepStatus {
        complete: counts.system_with_links > 0,
        count: counts.system_with_links,
    };

    let all_required_complete =
        environment.complete && flake.complete && builder.complete && cache.complete && system.complete;

    SetupWizardProgressResponse {
        dismissed,
        agent_acknowledged,
        environment,
        flake,
        builder,
        cache,
        system,
        all_required_complete,
    }
}

fn forbidden() -> axum::response::Response {
    (
        StatusCode::FORBIDDEN,
        Json(ApiError {
            error: "forbidden".to_string(),
            message: "Administrator role required".to_string(),
            details: None,
        }),
    )
        .into_response()
}

fn internal_error(message: &str) -> axum::response::Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            error: "internal_error".to_string(),
            message: message.to_string(),
            details: None,
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;
    use sqlx::postgres::PgPoolOptions;

    #[test]
    fn setup_progress_empty_instance() {
        let counts = SetupWizardCounts {
            environment: 0,
            flake: 0,
            builder_with_environment: 0,
            cache_with_environment: 0,
            system_with_links: 0,
        };

        let result = build_progress_response(counts, false, false);
        assert!(!result.environment.complete);
        assert!(!result.flake.complete);
        assert!(!result.builder.complete);
        assert!(!result.cache.complete);
        assert!(!result.system.complete);
        assert!(!result.all_required_complete);
    }

    #[test]
    fn setup_progress_partial_instance() {
        let counts = SetupWizardCounts {
            environment: 1,
            flake: 1,
            builder_with_environment: 0,
            cache_with_environment: 0,
            system_with_links: 0,
        };

        let result = build_progress_response(counts, false, false);
        assert!(result.environment.complete);
        assert!(result.flake.complete);
        assert!(!result.builder.complete);
        assert!(!result.cache.complete);
        assert!(!result.system.complete);
        assert!(!result.all_required_complete);
    }

    #[test]
    fn setup_progress_fully_configured_instance() {
        let counts = SetupWizardCounts {
            environment: 1,
            flake: 2,
            builder_with_environment: 1,
            cache_with_environment: 1,
            system_with_links: 3,
        };

        let result = build_progress_response(counts, false, true);
        assert!(result.environment.complete);
        assert!(result.flake.complete);
        assert!(result.builder.complete);
        assert!(result.cache.complete);
        assert!(result.system.complete);
        assert!(result.agent_acknowledged);
        assert!(result.all_required_complete);
    }

    #[tokio::test]
    async fn setup_progress_requires_admin() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/crystal_forge")
            .expect("lazy pool should construct");

        let response = get_setup_progress(State(pool), HeaderMap::new())
            .await
            .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}
