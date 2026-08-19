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
    // 1. Create a real framework release to satisfy the FK, then create a
    //    bundle with a published version that points at it.
    let bundle_id = Uuid::new_v4();
    let published_version_id = Uuid::new_v4();

    let framework_id = Uuid::new_v4();
    let test_framework_version_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO compliance_frameworks (id, name, canonical_source_key)
           VALUES ($1, 'NIST CSF', $2) RETURNING id"#,
    )
    .bind(framework_id)
    .bind(format!("canonical-{}", Uuid::new_v4()))
    .fetch_one(&pool)
    .await
    .expect("create framework");

    let test_framework_version_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO compliance_framework_versions
           (framework_id, version, canonical_release_key, title)
           VALUES ($1, '1.0', $2, 'NIST CSF 1.0') RETURNING id"#,
    )
    .bind(framework_id)
    .bind(format!("canonical-release-{}", Uuid::new_v4()))
    .fetch_one(&pool)
    .await
    .expect("create framework version");

    // Create the bundle
    sqlx::query(
        r#"INSERT INTO compliance_bundles (id, name, framework, version, layer, owner)
           VALUES ($1, $2, 'NIST CSF', '1.0', 'fleet', 'Platform Security')"#,
    )
    .bind(bundle_id)
    .bind(format!("framework-test-{}", Uuid::new_v4()))
    .execute(&pool)
    .await
    .expect("create bundle");

    // Create published version with framework_version_id in one tx with the
    // pointer update (deferred lineage constraint validates at commit).
    {
        let mut tx = pool.begin().await.expect("begin tx");
        sqlx::query(
            r#"INSERT INTO compliance_bundle_versions
               (id, bundle_id, version, publication_state, name, framework, framework_version,
                framework_version_id, description, layer, owner, semantic_digest)
               VALUES ($1, $2, '1.0', 'draft', $3, 'NIST CSF', '1.0',
                       $4, 'Published version', 'fleet', 'Platform Security', 'sha256-test')"#,
        )
        .bind(published_version_id)
        .bind(bundle_id)
        .bind(format!("framework-test-{}", Uuid::new_v4()))
        .bind(test_framework_version_id)
        .execute(&mut *tx)
        .await
        .expect("create published version");
        sqlx::query(
            "UPDATE compliance_bundle_versions SET publication_state = 'accepted', \
             published_at = CURRENT_TIMESTAMP WHERE id = $1",
        )
        .bind(published_version_id)
        .execute(&mut *tx)
        .await
        .expect("accept version");
        sqlx::query(
            "UPDATE compliance_bundles SET current_published_version_id = $1 WHERE id = $2",
        )
        .bind(published_version_id)
        .bind(bundle_id)
        .execute(&mut *tx)
        .await
        .expect("set current published");
        tx.commit().await.expect("commit publish");
    }

    // 3. The bundle insert trigger auto-creates a mutable 0.1.0 draft; remove it
    //    so ensure_bundle_draft() must actually derive from the published version.
    let auto_draft_id: Option<Uuid> =
        sqlx::query_scalar("SELECT current_draft_version_id FROM compliance_bundles WHERE id = $1")
            .bind(bundle_id)
            .fetch_one(&pool)
            .await
            .expect("query auto draft");

    if let Some(auto_draft_id) = auto_draft_id {
        sqlx::query("UPDATE compliance_bundles SET current_draft_version_id = NULL WHERE id = $1")
            .bind(bundle_id)
            .execute(&pool)
            .await
            .expect("clear auto draft pointer");
        sqlx::query("DELETE FROM compliance_bundle_versions WHERE id = $1")
            .bind(auto_draft_id)
            .execute(&pool)
            .await
            .expect("delete auto draft");
    }

    // 4. Call production ensure_bundle_draft() to derive a mutable draft
    let mut tx = pool.begin().await.expect("begin transaction");
    let _draft_version_id = ensure_bundle_draft(
        &mut tx,
        bundle_id,
        None,
        None,
        BundleDraftIntent::EnsureMutable,
    )
    .await
    .expect("ensure_bundle_draft must succeed");
    tx.commit().await.expect("commit transaction");

    // 5. Load draft from PostgreSQL
    let draft_version_id: Option<Uuid> =
        sqlx::query_scalar("SELECT current_draft_version_id FROM compliance_bundles WHERE id = $1")
            .bind(bundle_id)
            .fetch_optional(&pool)
            .await
            .expect("query draft version")
            .flatten();

    let draft_version_id =
        draft_version_id.expect("draft version must exist after ensure_bundle_draft");

    // 6. Verify framework_version_id is preserved in new draft
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
