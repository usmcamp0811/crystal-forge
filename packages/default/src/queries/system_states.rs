pub async fn insert_host_snapshot(
    pool: &PgPool,
    hostname: &str,
    context: &str,
    system_hash: &str,
    fp: &FingerprintParts,
) -> Result<()> {
    sqlx::query(
        r#"INSERT INTO tbl_system_states (
            hostname, 
            system_derivation_id,
            context, 
            os, 
            kernel,
            memory_gb, 
            uptime_secs, 
            cpu_brand, 
            cpu_cores,
            board_serial, 
            product_uuid, 
            rootfs_uuid
        ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
        ON CONFLICT DO NOTHING"#,
    )
    .bind(hostname)
    .bind(system_hash)
    .bind(context)
    .bind(&fp.os)
    .bind(&fp.kernel)
    .bind(fp.memory_gb)
    .bind(fp.uptime_secs as i64)
    .bind(&fp.cpu_brand)
    .bind(fp.cpu_cores as i32)
    .bind(&fp.board_serial)
    .bind(&fp.product_uuid)
    .bind(&fp.rootfs_uuid)
    .execute(pool)
    .await?;

    Ok(())
}
