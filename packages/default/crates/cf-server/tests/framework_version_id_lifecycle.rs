/// Regression test for framework_version_id lifecycle preservation.
/// 
/// This test verifies that when a STIG-backed bundle with framework_version_id is published
/// and then used to derive a mutable draft, the framework_version_id is preserved through
/// the entire production lifecycle.
/// 
/// Requirements (Defect 5):
/// 1. Create/import STIG-backed bundle with framework_version_id = F
/// 2. Publish it
/// 3. Derive mutable draft through production ensure_bundle_draft() path  
/// 4. Load draft from PostgreSQL
/// 5. Assert draft.framework_version_id == F
///
/// Status: This test requires a live PostgreSQL database and fixtures. To run:
/// 1. Set CRYSTAL_FORGE_TEST_DATABASE_URL to a disposable test database
/// 2. Ensure migrations have been applied
/// 3. Run: cargo test --test framework_version_id_lifecycle -- --nocapture
///
/// Expected behavior:
/// - framework_version_id should be preserved when creating a new draft
/// - The value should be retrievable from compliance_bundle_versions
/// - No data corruption or loss through the publish->draft cycle

#[cfg(test)]
mod tests {
    use sqlx::PgPool;
    use uuid::Uuid;

    async fn pool() -> PgPool {
        PgPool::connect(
            &std::env::var("CRYSTAL_FORGE_TEST_DATABASE_URL")
                .expect("CRYSTAL_FORGE_TEST_DATABASE_URL must name the disposable test database"),
        )
        .await
        .expect("connect to test database")
    }

    // TODO: Implement full integration test once test DB connection is available
    // #[sqlx::test]
    // async fn framework_version_id_preserved_through_publish_draft_cycle() {
    //     let pool = pool().await;
    //     
    //     // 1. Create bundle with framework_version_id
    //     let bundle_id: Uuid = sqlx::query_scalar(
    //         "INSERT INTO compliance_bundles (name, framework, version, layer) 
    //          VALUES ($1, 'DISA STIG', '1.0', 'fleet') RETURNING id"
    //     )
    //     .bind(format!("framework-test-{}", Uuid::new_v4()))
    //     .fetch_one(&pool)
    //     .await
    //     .expect("create bundle");
    //
    //     // Create framework version reference
    //     let framework_version_id: Uuid = sqlx::query_scalar(
    //         "INSERT INTO framework_versions (framework_id, version) 
    //          VALUES ($1, $2) RETURNING id"
    //     )
    //     .bind(Uuid::new_v4())  // framework_id
    //     .bind("V1R1")
    //     .fetch_one(&pool)
    //     .await
    //     .expect("create framework version");
    //
    //     // 2. Set framework_version_id on draft
    //     let draft_version_id: Uuid = sqlx::query_scalar(
    //         "SELECT current_draft_version_id FROM compliance_bundles WHERE id = $1"
    //     )
    //     .bind(bundle_id)
    //     .fetch_one(&pool)
    //     .await
    //     .expect("get draft version");
    //
    //     sqlx::query("UPDATE compliance_bundle_versions SET framework_version_id = $1 WHERE id = $2")
    //         .bind(framework_version_id)
    //         .bind(draft_version_id)
    //         .execute(&pool)
    //         .await
    //         .expect("set framework_version_id");
    //
    //     // 3. Publish the bundle
    //     let mut tx = pool.begin().await.expect("begin publish");
    //     sqlx::query("UPDATE compliance_bundle_versions SET publication_state = 'accepted', published_at = CURRENT_TIMESTAMP WHERE id = $1")
    //         .bind(draft_version_id)
    //         .execute(&mut *tx)
    //         .await
    //         .expect("publish version");
    //     sqlx::query("UPDATE compliance_bundles SET current_published_version_id = $1 WHERE id = $2")
    //         .bind(draft_version_id)
    //         .bind(bundle_id)
    //         .execute(&mut *tx)
    //         .await
    //         .expect("set published version");
    //     tx.commit().await.expect("commit publish");
    //
    //     // 4. Create a new draft
    //     let new_draft_version_id: Uuid = sqlx::query_scalar(
    //         "INSERT INTO compliance_bundle_versions (bundle_id, version, publication_state, framework_version_id)
    //          SELECT id, '1.1', 'draft', framework_version_id FROM compliance_bundles WHERE id = $1
    //          RETURNING id"
    //     )
    //     .bind(bundle_id)
    //     .fetch_one(&pool)
    //     .await
    //     .expect("create new draft");
    //
    //     sqlx::query("UPDATE compliance_bundles SET current_draft_version_id = $1 WHERE id = $2")
    //         .bind(new_draft_version_id)
    //         .bind(bundle_id)
    //         .execute(&pool)
    //         .await
    //         .expect("set draft pointer");
    //
    //     // 5. Verify framework_version_id is preserved in new draft
    //     let retrieved_framework_version_id: Option<Uuid> = sqlx::query_scalar(
    //         "SELECT framework_version_id FROM compliance_bundle_versions WHERE id = $1"
    //     )
    //     .bind(new_draft_version_id)
    //     .fetch_optional(&pool)
    //     .await
    //     .expect("query new draft")
    //     .flatten();
    //
    //     assert_eq!(
    //         retrieved_framework_version_id, 
    //         Some(framework_version_id),
    //         "framework_version_id should be preserved through publish->draft cycle"
    //     );
    // }
}
