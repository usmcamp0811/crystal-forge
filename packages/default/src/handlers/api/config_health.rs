//! Admin config-health handler — `GET /api/v1/admin/config-health`
//!
//! Returns a structured pipeline readiness report derived from counts of
//! existing entities. Requires Admin role. No database schema changes.

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use sqlx::PgPool;
use tracing::error;

use crate::api::models::{ApiError, ConfigHealthCheck, ConfigHealthResponse};
use crate::handlers::api::rbac::require_admin;
use crate::queries::config_health::{
    count_builders, count_cache_destinations, count_environments, count_flakes,
    count_flakes_with_eval_errors,
};

/// `GET /api/v1/admin/config-health`
///
/// Returns a [`ConfigHealthResponse`] summarising pipeline readiness.
/// Requires Admin role — returns 403 for all other roles.
pub async fn config_health(State(pool): State<PgPool>, headers: HeaderMap) -> impl IntoResponse {
    if require_admin(&pool, &headers).await.is_none() {
        return (
            StatusCode::FORBIDDEN,
            Json(ApiError {
                error: "forbidden".to_string(),
                message: "Admin privileges are required".to_string(),
                details: None,
            }),
        )
            .into_response();
    }

    match build_config_health_response(&pool).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(e) => {
            error!("Config health query failed: {e:#}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "internal_error",
                    "message": "Failed to build config health response"
                })),
            )
                .into_response()
        }
    }
}

async fn build_config_health_response(pool: &PgPool) -> anyhow::Result<ConfigHealthResponse> {
    let (flakes, environments, builders, caches, flakes_with_errors) = tokio::try_join!(
        count_flakes(pool),
        count_environments(pool),
        count_builders(pool),
        count_cache_destinations(pool),
        count_flakes_with_eval_errors(pool),
    )?;

    let has_flakes = flakes > 0;
    let has_environments = environments > 0;
    let has_builders = builders > 0;
    let has_cache_destinations = caches > 0;
    let has_flakes_with_errors = flakes_with_errors > 0;

    let checks = vec![
        ConfigHealthCheck {
            id: "no_flakes".to_string(),
            passed: has_flakes,
            message: "No flakes are being watched. Add a flake to begin evaluating NixOS configurations.".to_string(),
            action_url: "/flakes".to_string(),
        },
        ConfigHealthCheck {
            id: "no_environments".to_string(),
            passed: has_environments,
            message: "No environments exist. Environments are required to organize systems, builders, and caches.".to_string(),
            action_url: "/environments".to_string(),
        },
        ConfigHealthCheck {
            id: "no_builders".to_string(),
            passed: has_builders,
            message: "No builders are registered. Derivations will be evaluated but never built.".to_string(),
            action_url: "/builders".to_string(),
        },
        ConfigHealthCheck {
            id: "no_cache_destinations".to_string(),
            passed: has_cache_destinations,
            message: "No cache destinations configured. Builds will succeed but agents won't be able to pull deployments.".to_string(),
            action_url: "/caches".to_string(),
        },
        ConfigHealthCheck {
            id: "flake_eval_errors".to_string(),
            passed: !has_flakes_with_errors,
            message: "One or more flakes have evaluation errors on their latest commit. Check flake configuration.".to_string(),
            action_url: "/flakes".to_string(),
        },
    ];

    let total_issues = checks.iter().filter(|c| !c.passed).count() as u32;

    Ok(ConfigHealthResponse {
        has_flakes,
        has_environments,
        has_builders,
        has_cache_destinations,
        total_issues,
        checks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;
    use sqlx::postgres::PgPoolOptions;

    fn lazy_pool() -> PgPool {
        PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct")
    }

    #[tokio::test]
    async fn config_health_requires_admin() {
        let pool = lazy_pool();
        let response = config_health(State(pool), HeaderMap::new())
            .await
            .into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn all_checks_fail_when_counts_zero() {
        // Simulate what build_config_health_response does with all-zero counts.
        let flakes = 0i64;
        let environments = 0i64;
        let builders = 0i64;
        let caches = 0i64;
        let flakes_with_errors = 0i64;

        let has_flakes = flakes > 0;
        let has_environments = environments > 0;
        let has_builders = builders > 0;
        let has_cache_destinations = caches > 0;
        let has_flakes_with_errors = flakes_with_errors > 0;

        let checks = vec![
            ConfigHealthCheck {
                id: "no_flakes".to_string(),
                passed: has_flakes,
                message: String::new(),
                action_url: String::new(),
            },
            ConfigHealthCheck {
                id: "no_environments".to_string(),
                passed: has_environments,
                message: String::new(),
                action_url: String::new(),
            },
            ConfigHealthCheck {
                id: "no_builders".to_string(),
                passed: has_builders,
                message: String::new(),
                action_url: String::new(),
            },
            ConfigHealthCheck {
                id: "no_cache_destinations".to_string(),
                passed: has_cache_destinations,
                message: String::new(),
                action_url: String::new(),
            },
            ConfigHealthCheck {
                id: "flake_eval_errors".to_string(),
                passed: !has_flakes_with_errors,
                message: String::new(),
                action_url: String::new(),
            },
        ];

        let total_issues = checks.iter().filter(|c| !c.passed).count() as u32;

        // With no entities at all, only the first 4 checks fail.
        // flake_eval_errors passes when there are no flakes (nothing to error).
        assert!(!has_flakes);
        assert!(!has_environments);
        assert!(!has_builders);
        assert!(!has_cache_destinations);
        assert!(!has_flakes_with_errors); // no flakes → no errors
        assert_eq!(total_issues, 4); // 4 failing checks (eval errors passes)
    }

    #[test]
    fn all_checks_pass_when_fully_configured() {
        let checks = vec![
            ConfigHealthCheck {
                id: "no_flakes".to_string(),
                passed: true,
                message: String::new(),
                action_url: String::new(),
            },
            ConfigHealthCheck {
                id: "no_environments".to_string(),
                passed: true,
                message: String::new(),
                action_url: String::new(),
            },
            ConfigHealthCheck {
                id: "no_builders".to_string(),
                passed: true,
                message: String::new(),
                action_url: String::new(),
            },
            ConfigHealthCheck {
                id: "no_cache_destinations".to_string(),
                passed: true,
                message: String::new(),
                action_url: String::new(),
            },
            ConfigHealthCheck {
                id: "flake_eval_errors".to_string(),
                passed: true,
                message: String::new(),
                action_url: String::new(),
            },
        ];

        let total_issues = checks.iter().filter(|c| !c.passed).count() as u32;
        assert_eq!(total_issues, 0);
    }

    #[test]
    fn partial_config_counts_correctly() {
        // Only flakes and environments are configured.
        let checks = vec![
            ConfigHealthCheck {
                id: "no_flakes".to_string(),
                passed: true,
                message: String::new(),
                action_url: String::new(),
            },
            ConfigHealthCheck {
                id: "no_environments".to_string(),
                passed: true,
                message: String::new(),
                action_url: String::new(),
            },
            ConfigHealthCheck {
                id: "no_builders".to_string(),
                passed: false,
                message: String::new(),
                action_url: String::new(),
            },
            ConfigHealthCheck {
                id: "no_cache_destinations".to_string(),
                passed: false,
                message: String::new(),
                action_url: String::new(),
            },
            ConfigHealthCheck {
                id: "flake_eval_errors".to_string(),
                passed: true,
                message: String::new(),
                action_url: String::new(),
            },
        ];

        let total_issues = checks.iter().filter(|c| !c.passed).count() as u32;
        assert_eq!(total_issues, 2);
    }

    #[test]
    fn eval_error_check_fails_when_flake_has_error() {
        let has_flakes_with_errors = true;
        let check = ConfigHealthCheck {
            id: "flake_eval_errors".to_string(),
            passed: !has_flakes_with_errors,
            message: String::new(),
            action_url: String::new(),
        };
        assert!(!check.passed);
    }
}
