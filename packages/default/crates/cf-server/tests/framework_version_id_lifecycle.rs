use crystal_forge::queries::compliance::{BundleDraftIntent, ensure_bundle_draft};
/// Regression test for framework_version_id lifecycle preservation.
///
/// This test verifies that when a STIG-backed bundle with framework_version_id is published
/// and then used to derive a mutable draft, the framework_version_id is preserved through
/// the entire production lifecycle using the real ensure_bundle_draft() handler.
///
/// Requirements (Defect 5):
/// 1. Create/import STIG-backed bundle with framework_version_id = F
/// 2. Publish it as current_published_version
/// 3. Call production ensure_bundle_draft() to derive mutable draft
/// 4. Load draft from PostgreSQL
/// 5. Assert draft.framework_version_id == F
use sqlx::PgPool;
use uuid::Uuid;

#[sqlx::test]
async fn framework_version_id_preserved_through_publish_draft_cycle(pool: PgPool) {
    // 1. Create bundle with a published version that has framework_version_id
    let bundle_id = Uuid::new_v4();
    let published_version_id = Uuid::new_v4();
    let test_framework_version_id = Uuid::new_v4();

    // Create the bundle
    sqlx::query(
        r#"INSERT INTO compliance_bundles (id, name, description, created_at, updated_at)
           VALUES ($1, $2, 'Framework version test', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(bundle_id)
    .bind(format!("framework-test-{}", Uuid::new_v4()))
    .execute(&pool)
    .await
    .expect("create bundle");

    // Create published version with framework_version_id
    sqlx::query(
        r#"INSERT INTO compliance_bundle_versions 
           (id, bundle_id, version, publication_state, framework, framework_version_id, description, created_at, updated_at)
           VALUES ($1, $2, '1.0', 'accepted', 'NIST CSF', $3, 'Published version', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(published_version_id)
    .bind(bundle_id)
    .bind(test_framework_version_id)
    .execute(&pool)
    .await
    .expect("create published version");

    // 2. Set as current published version
    sqlx::query("UPDATE compliance_bundles SET current_published_version_id = $1 WHERE id = $2")
        .bind(published_version_id)
        .bind(bundle_id)
        .execute(&pool)
        .await
        .expect("set current published");

    // 3. Call production ensure_bundle_draft() to derive a mutable draft
    let actor_id = Some(Uuid::new_v4());
    let mut tx = pool.begin().await.expect("begin transaction");
    let _draft_version_id = ensure_bundle_draft(
        &mut tx,
        bundle_id,
        actor_id,
        None,
        BundleDraftIntent::EnsureMutable,
    )
    .await
    .expect("ensure_bundle_draft must succeed");
    tx.commit().await.expect("commit transaction");

    // 4. Load draft from PostgreSQL
    let draft_version_id: Option<Uuid> =
        sqlx::query_scalar("SELECT current_draft_version_id FROM compliance_bundles WHERE id = $1")
            .bind(bundle_id)
            .fetch_optional(&pool)
            .await
            .expect("query draft version")
            .flatten();

    let draft_version_id =
        draft_version_id.expect("draft version must exist after ensure_bundle_draft");

    // 5. Verify framework_version_id is preserved in new draft
    let retrieved_framework_version_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT framework_version_id FROM compliance_bundle_versions WHERE id = $1",
    )
    .bind(draft_version_id)
    .fetch_optional(&pool)
    .await
    .expect("query draft framework_version_id")
    .flatten();

    assert_eq!(
        retrieved_framework_version_id,
        Some(test_framework_version_id),
        "framework_version_id should be preserved through ensure_bundle_draft() lifecycle"
    );
}
