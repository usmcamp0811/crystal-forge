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

#[derive(Debug, Clone, Copy, Default, sqlx::FromRow)]
struct SetupWizardCounts {
    environment: i64,
    flake: i64,
    // Wildcard builders (no explicit env rows) are valid and can process
    // jobs from all environments, so this is a total builder count.
    builder_count: i64,
    // Cache destinations can be scoped to environments or global.
    // Either variant satisfies the setup step.
    cache_destination_count: i64,
    system_with_links: i64,
    policy: i64,
    bundle: i64,
    poam: i64,
}

const SETUP_WIZARD_COUNTS_QUERY: &str = r#"
    SELECT
        (SELECT COUNT(*)::bigint FROM environments) AS environment,
        (SELECT COUNT(*)::bigint FROM flakes) AS flake,
        (SELECT COUNT(*)::bigint FROM builders) AS builder_count,
        (SELECT COUNT(*)::bigint FROM cache_destinations) AS cache_destination_count,
        (SELECT COUNT(*)::bigint FROM systems
            WHERE environment_id IS NOT NULL AND flake_id IS NOT NULL) AS system_with_links,
        (SELECT COUNT(DISTINCT policy_id)::bigint FROM deployment_policy_versions
            WHERE created_by IS NOT NULL) AS policy,
        (SELECT COUNT(*)::bigint FROM compliance_bundles) AS bundle,
        (SELECT COUNT(*)::bigint FROM poams) AS poam
    "#;

pub async fn get_setup_progress(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> impl IntoResponse {
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
    // Policy progress counts lineages, not versions. Requiring an attributed
    // version excludes migration-seeded defaults while retaining imports and
    // manually created policies. POA&M history remains complete in any status.
    sqlx::query_as::<_, SetupWizardCounts>(SETUP_WIZARD_COUNTS_QUERY)
        .fetch_one(pool)
        .await
        .map_err(Into::into)
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
        complete: counts.builder_count > 0,
        count: counts.builder_count,
    };
    let cache = SetupWizardStepStatus {
        complete: counts.cache_destination_count > 0,
        count: counts.cache_destination_count,
    };
    let system = SetupWizardStepStatus {
        complete: counts.system_with_links > 0,
        count: counts.system_with_links,
    };
    let policy = SetupWizardStepStatus {
        complete: counts.policy > 0,
        count: counts.policy,
    };
    let bundle = SetupWizardStepStatus {
        complete: counts.bundle > 0,
        count: counts.bundle,
    };
    let poam = SetupWizardStepStatus {
        complete: counts.poam > 0,
        count: counts.poam,
    };

    let all_required_complete = environment.complete
        && flake.complete
        && builder.complete
        && cache.complete
        && system.complete;
    let all_coach_steps_complete = all_required_complete
        && agent_acknowledged
        && policy.complete
        && bundle.complete
        && poam.complete;

    SetupWizardProgressResponse {
        dismissed,
        agent_acknowledged,
        environment,
        flake,
        builder,
        cache,
        system,
        policy,
        bundle,
        poam,
        all_required_complete,
        all_coach_steps_complete,
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
            ..Default::default()
        };

        let result = build_progress_response(counts, false, false);
        assert!(!result.environment.complete);
        assert!(!result.flake.complete);
        assert!(!result.builder.complete);
        assert!(!result.cache.complete);
        assert!(!result.system.complete);
        assert!(!result.all_required_complete);
        assert!(!result.all_coach_steps_complete);
    }

    #[test]
    fn setup_progress_partial_instance() {
        let counts = SetupWizardCounts {
            environment: 1,
            flake: 1,
            builder_count: 0,
            cache_destination_count: 0,
            system_with_links: 0,
            ..Default::default()
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
            builder_count: 1,
            cache_destination_count: 1,
            system_with_links: 3,
            ..Default::default()
        };

        let result = build_progress_response(counts, false, true);
        assert!(result.environment.complete);
        assert!(result.flake.complete);
        assert!(result.builder.complete);
        assert!(result.cache.complete);
        assert!(result.system.complete);
        assert!(result.agent_acknowledged);
        assert!(result.all_required_complete);
        assert!(!result.all_coach_steps_complete);
    }

    #[test]
    fn setup_progress_cache_step_accepts_global_destination() {
        let counts = SetupWizardCounts {
            environment: 1,
            flake: 1,
            builder_count: 1,
            cache_destination_count: 1,
            system_with_links: 0,
            ..Default::default()
        };

        let result = build_progress_response(counts, false, false);
        assert!(result.cache.complete);
        assert_eq!(result.cache.count, 1);
    }

    #[test]
    fn setup_progress_attributable_policy_is_independent() {
        let result = build_progress_response(
            SetupWizardCounts {
                policy: 1,
                ..Default::default()
            },
            false,
            false,
        );

        assert!(result.policy.complete);
        assert_eq!(result.policy.count, 1);
        assert!(!result.bundle.complete);
        assert!(!result.poam.complete);
        assert!(!result.all_required_complete);
    }

    #[test]
    fn setup_progress_seed_like_unattributed_policy_is_excluded() {
        let result = build_progress_response(SetupWizardCounts::default(), false, false);

        assert!(!result.policy.complete);
        assert_eq!(result.policy.count, 0);
    }

    #[test]
    fn setup_progress_query_counts_distinct_attributed_policy_lineages() {
        assert!(SETUP_WIZARD_COUNTS_QUERY.contains(
            "COUNT(DISTINCT policy_id)::bigint FROM deployment_policy_versions\n            WHERE created_by IS NOT NULL"
        ));
    }

    #[test]
    fn setup_progress_bundle_is_independent() {
        let result = build_progress_response(
            SetupWizardCounts {
                bundle: 2,
                ..Default::default()
            },
            false,
            false,
        );

        assert!(result.bundle.complete);
        assert_eq!(result.bundle.count, 2);
        assert!(!result.policy.complete);
        assert!(!result.poam.complete);
    }

    #[test]
    fn setup_progress_poam_history_in_any_status_counts() {
        let result = build_progress_response(
            SetupWizardCounts {
                poam: 1,
                ..Default::default()
            },
            false,
            false,
        );

        assert!(result.poam.complete);
        assert_eq!(result.poam.count, 1);
        assert!(SETUP_WIZARD_COUNTS_QUERY.contains("COUNT(*)::bigint FROM poams"));
        assert!(!SETUP_WIZARD_COUNTS_QUERY.contains("FROM poams WHERE"));
    }

    #[test]
    fn setup_progress_original_six_do_not_complete_nine_step_coach() {
        let result = build_progress_response(
            SetupWizardCounts {
                environment: 1,
                flake: 1,
                builder_count: 1,
                cache_destination_count: 1,
                system_with_links: 1,
                ..Default::default()
            },
            false,
            true,
        );

        assert!(result.all_required_complete);
        assert!(result.agent_acknowledged);
        assert!(!result.all_coach_steps_complete);
    }

    #[test]
    fn setup_progress_all_nine_complete_sets_coach_aggregate() {
        let result = build_progress_response(
            SetupWizardCounts {
                environment: 1,
                flake: 1,
                builder_count: 1,
                cache_destination_count: 1,
                system_with_links: 1,
                policy: 1,
                bundle: 1,
                poam: 1,
            },
            false,
            true,
        );

        assert!(result.all_required_complete);
        assert!(result.all_coach_steps_complete);
    }

    #[tokio::test]
    #[ignore = "requires migrated CRYSTAL_FORGE_TEST_DATABASE_URL"]
    async fn setup_progress_counts_production_policy_bundle_and_poam_rows() {
        let database_url = std::env::var("CRYSTAL_FORGE_TEST_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .expect("test database URL must be set");
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&database_url)
            .await
            .expect("connect to setup-progress test database");
        let before = load_counts(&pool).await.expect("load baseline counts");
        let token = uuid::Uuid::new_v4().simple().to_string();
        let user_id = uuid::Uuid::new_v4();
        let environment_id = uuid::Uuid::new_v4();
        let system_id = uuid::Uuid::new_v4();
        let seeded_policy_id = uuid::Uuid::new_v4();
        let attributed_policy_id = uuid::Uuid::new_v4();
        let finding_id = uuid::Uuid::new_v4();
        let poam_id = uuid::Uuid::new_v4();
        let mut tx = pool.begin().await.expect("begin setup-progress fixture");

        sqlx::query(
            "INSERT INTO users (id, username, first_name, last_name, email, user_type, is_active) \
             VALUES ($1, $2, 'Setup', 'Coach', $3, 'human', TRUE)",
        )
        .bind(user_id)
        .bind(format!("setup-coach-{token}"))
        .bind(format!("setup-coach-{token}@example.test"))
        .execute(&mut *tx)
        .await
        .expect("insert setup-progress user");
        for (policy_id, suffix) in [(seeded_policy_id, "seeded"), (attributed_policy_id, "user")] {
            sqlx::query(
                "INSERT INTO deployment_policies (id, name, policy_type, config, enabled) \
                 VALUES ($1, $2, 'custom_check', '{\"expression\": \"true\"}', FALSE)",
            )
            .bind(policy_id)
            .bind(format!("setup-coach-{suffix}-{token}"))
            .execute(&mut *tx)
            .await
            .expect("insert setup-progress policy");
        }
        sqlx::query(
            "UPDATE deployment_policy_versions SET created_by = $2 \
             WHERE id = (SELECT current_draft_version_id FROM deployment_policies WHERE id = $1)",
        )
        .bind(attributed_policy_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .expect("attribute user-created policy version");
        sqlx::query(
            "INSERT INTO compliance_bundles (name, framework) VALUES ($1, 'Setup Coach Test')",
        )
        .bind(format!("setup-coach-{token}"))
        .execute(&mut *tx)
        .await
        .expect("insert setup-progress bundle");
        sqlx::query("INSERT INTO environments (id, name) VALUES ($1, $2)")
            .bind(environment_id)
            .bind(format!("setup-coach-{token}"))
            .execute(&mut *tx)
            .await
            .expect("insert setup-progress environment");
        sqlx::query(
            "INSERT INTO systems (id, hostname, environment_id, public_key, derivation, is_active) \
             VALUES ($1, $2, $3, $4, '', TRUE)",
        )
        .bind(system_id)
        .bind(format!("setup-coach-{token}"))
        .bind(environment_id)
        .bind(format!("ssh-ed25519 AAAA-setup-coach-{token}"))
        .execute(&mut *tx)
        .await
        .expect("insert setup-progress system");
        sqlx::query(
            "INSERT INTO poam_findings (id, system_id, policy_lineage_id) VALUES ($1, $2, $3)",
        )
        .bind(finding_id)
        .bind(system_id)
        .bind(attributed_policy_id)
        .execute(&mut *tx)
        .await
        .expect("insert setup-progress finding");
        sqlx::query("INSERT INTO poams (id, title, risk, created_by) VALUES ($1, $2, 'low', $3)")
            .bind(poam_id)
            .bind(format!("Setup coach POA&M {token}"))
            .bind(user_id)
            .execute(&mut *tx)
            .await
            .expect("insert setup-progress POA&M");
        sqlx::query(
            "INSERT INTO poam_finding_links (poam_id, finding_id, linked_by) VALUES ($1, $2, $3)",
        )
        .bind(poam_id)
        .bind(finding_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .expect("link setup-progress finding");
        tx.commit().await.expect("commit setup-progress fixture");

        let after = load_counts(&pool).await.expect("load updated counts");
        assert_eq!(after.policy, before.policy + 1);
        assert_eq!(after.bundle, before.bundle + 1);
        assert_eq!(after.poam, before.poam + 1);
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
