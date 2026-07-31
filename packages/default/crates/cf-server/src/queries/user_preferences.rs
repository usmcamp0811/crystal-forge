use anyhow::Result;
use sqlx::PgPool;
use uuid::Uuid;

use crate::api::models::UpdateUserPreferences;
use crate::models::user_preferences::UserPreferences;

pub async fn get_user_preferences(pool: &PgPool, user_id: Uuid) -> Result<Option<UserPreferences>> {
    let preferences = sqlx::query_as::<_, UserPreferences>(
        r#"
        SELECT user_id, theme, density, sidebar_collapsed, default_systems_view, updated_at
        FROM user_preferences
        WHERE user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    Ok(preferences)
}

pub async fn update_user_preferences(
    pool: &PgPool,
    user_id: Uuid,
    update: &UpdateUserPreferences,
) -> Result<UserPreferences> {
    let theme = update.theme.map(|value| value.as_str());
    let density = update.density.map(|value| value.as_str());
    let sidebar_collapsed = update.sidebar_collapsed;
    let default_systems_view = update.default_systems_view.map(|value| value.as_str());

    let preferences = sqlx::query_as::<_, UserPreferences>(
        r#"
        INSERT INTO user_preferences (
            user_id,
            theme,
            density,
            sidebar_collapsed,
            default_systems_view
        )
        VALUES (
            $1,
            COALESCE($2, 'dark'),
            COALESCE($3, 'comfortable'),
            COALESCE($4, FALSE),
            COALESCE($5, 'cards')
        )
        ON CONFLICT (user_id)
        DO UPDATE SET
            theme = COALESCE($2, user_preferences.theme),
            density = COALESCE($3, user_preferences.density),
            sidebar_collapsed = COALESCE($4, user_preferences.sidebar_collapsed),
            default_systems_view = COALESCE($5, user_preferences.default_systems_view),
            updated_at = NOW()
        RETURNING user_id, theme, density, sidebar_collapsed, default_systems_view, updated_at
        "#,
    )
    .bind(user_id)
    .bind(theme)
    .bind(density)
    .bind(sidebar_collapsed)
    .bind(default_systems_view)
    .fetch_one(pool)
    .await?;

    Ok(preferences)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::{SystemsViewPreference, UiDensityPreference, UiThemePreference};
    use crate::auth::repository::normalize_tenant_discriminator;
    use crate::queries::auth_identity::{AuthIdentityRepository, NewExternalIdentity};

    async fn live_pool() -> PgPool {
        let database_url = std::env::var("DATABASE_URL")
            .expect("DATABASE_URL must be set for live user preference tests");
        PgPool::connect(&database_url)
            .await
            .expect("failed to connect to DATABASE_URL")
    }

    async fn insert_test_user(pool: &PgPool, label: &str) -> Uuid {
        let id = Uuid::new_v4();
        let username = format!("pref-test-{label}-{id}");
        let email = format!("{username}@example.invalid");

        sqlx::query(
            r#"
            INSERT INTO users (
                id, username, first_name, last_name, email, user_type, is_active
            )
            VALUES ($1, $2, 'Preference', 'Test', $3, 'human', true)
            "#,
        )
        .bind(id)
        .bind(username)
        .bind(email)
        .execute(pool)
        .await
        .expect("insert test user");

        id
    }

    #[test]
    fn preference_enum_strings_match_database_constraints() {
        assert_eq!(UiThemePreference::Dark.as_str(), "dark");
        assert_eq!(UiThemePreference::Light.as_str(), "light");
        assert_eq!(UiDensityPreference::Comfortable.as_str(), "comfortable");
        assert_eq!(UiDensityPreference::Compact.as_str(), "compact");
        assert_eq!(SystemsViewPreference::Cards.as_str(), "cards");
        assert_eq!(SystemsViewPreference::Table.as_str(), "table");
    }

    #[test]
    fn partial_update_can_represent_one_field_without_user_id() {
        let update = UpdateUserPreferences {
            theme: Some(UiThemePreference::Light),
            density: None,
            sidebar_collapsed: None,
            default_systems_view: None,
        };

        assert_eq!(update.theme.unwrap().as_str(), "light");
        assert!(update.density.is_none());
        assert!(update.sidebar_collapsed.is_none());
        assert!(update.default_systems_view.is_none());
    }

    #[test]
    fn update_request_serializes_without_user_id() {
        let update = UpdateUserPreferences {
            theme: Some(UiThemePreference::Light),
            density: None,
            sidebar_collapsed: Some(true),
            default_systems_view: None,
        };

        let json = serde_json::to_value(update).expect("serialize update request");

        assert!(json.get("user_id").is_none());
        assert_eq!(json.get("theme").and_then(|v| v.as_str()), Some("light"));
        assert_eq!(
            json.get("sidebar_collapsed").and_then(|v| v.as_bool()),
            Some(true)
        );
    }

    #[tokio::test]
    #[ignore = "requires a migrated live database; run with DATABASE_URL=... cargo test user_preferences -- --ignored"]
    async fn same_user_preferences_persist_across_sessions() {
        let pool = live_pool().await;
        let user_id = insert_test_user(&pool, "same-user").await;

        update_user_preferences(
            &pool,
            user_id,
            &UpdateUserPreferences {
                theme: Some(UiThemePreference::Light),
                ..UpdateUserPreferences::default()
            },
        )
        .await
        .expect("save preferences");

        let loaded = get_user_preferences(&pool, user_id)
            .await
            .expect("load preferences")
            .expect("row exists");

        assert_eq!(loaded.theme, "light");
    }

    #[tokio::test]
    #[ignore = "requires a migrated live database; run with DATABASE_URL=... cargo test user_preferences -- --ignored"]
    async fn different_users_have_isolated_preferences() {
        let pool = live_pool().await;
        let user_a = insert_test_user(&pool, "user-a").await;
        let user_b = insert_test_user(&pool, "user-b").await;

        update_user_preferences(
            &pool,
            user_a,
            &UpdateUserPreferences {
                theme: Some(UiThemePreference::Light),
                density: Some(UiDensityPreference::Compact),
                ..UpdateUserPreferences::default()
            },
        )
        .await
        .expect("save preferences for user A");
        update_user_preferences(
            &pool,
            user_b,
            &UpdateUserPreferences {
                theme: Some(UiThemePreference::Dark),
                density: Some(UiDensityPreference::Comfortable),
                ..UpdateUserPreferences::default()
            },
        )
        .await
        .expect("save preferences for user B");

        let prefs_a = get_user_preferences(&pool, user_a)
            .await
            .expect("load user A")
            .expect("user A row");
        let prefs_b = get_user_preferences(&pool, user_b)
            .await
            .expect("load user B")
            .expect("user B row");

        assert_eq!(prefs_a.theme, "light");
        assert_eq!(prefs_a.density, "compact");
        assert_eq!(prefs_b.theme, "dark");
        assert_eq!(prefs_b.density, "comfortable");
    }

    #[tokio::test]
    #[ignore = "requires a migrated live database; run with DATABASE_URL=... cargo test user_preferences -- --ignored"]
    async fn partial_patch_preserves_unsupplied_fields() {
        let pool = live_pool().await;
        let user_id = insert_test_user(&pool, "partial").await;

        update_user_preferences(
            &pool,
            user_id,
            &UpdateUserPreferences {
                theme: Some(UiThemePreference::Light),
                density: Some(UiDensityPreference::Compact),
                sidebar_collapsed: Some(true),
                default_systems_view: Some(SystemsViewPreference::Table),
            },
        )
        .await
        .expect("seed preferences");

        let updated = update_user_preferences(
            &pool,
            user_id,
            &UpdateUserPreferences {
                theme: Some(UiThemePreference::Dark),
                ..UpdateUserPreferences::default()
            },
        )
        .await
        .expect("partial update");

        assert_eq!(updated.theme, "dark");
        assert_eq!(updated.density, "compact");
        assert!(updated.sidebar_collapsed);
        assert_eq!(updated.default_systems_view, "table");
    }

    #[tokio::test]
    #[ignore = "requires a migrated live database; run with DATABASE_URL=... cargo test user_preferences -- --ignored"]
    async fn same_oidc_issuer_subject_resolves_same_preferences() {
        let pool = live_pool().await;
        let user_id = insert_test_user(&pool, "oidc").await;
        let repo = AuthIdentityRepository::new(&pool);
        let provider_key = "https://issuer.example.invalid";
        let subject = format!("subject-{}", Uuid::new_v4());
        let tenant = normalize_tenant_discriminator(Some(provider_key));

        repo.upsert_external_identity(&NewExternalIdentity {
            user_id,
            provider_key: provider_key.to_string(),
            subject: subject.clone(),
            tenant_discriminator: Some(provider_key.to_string()),
            claims: serde_json::json!({"sub": subject}),
        })
        .await
        .expect("insert external identity");

        update_user_preferences(
            &pool,
            user_id,
            &UpdateUserPreferences {
                theme: Some(UiThemePreference::Light),
                ..UpdateUserPreferences::default()
            },
        )
        .await
        .expect("save preferences");

        let identity = repo
            .find_external_identity(provider_key, &subject, Some(provider_key))
            .await
            .expect("find external identity")
            .expect("identity exists");
        assert_eq!(identity.tenant_discriminator, tenant);

        let loaded = get_user_preferences(&pool, identity.user_id)
            .await
            .expect("load preferences by resolved user")
            .expect("preferences exist");

        assert_eq!(loaded.theme, "light");
    }
}
