use crate::models::system_states::SystemState;
use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

pub async fn insert_system_state(
    pool: &PgPool,
    state: &SystemState,
    version_compatible: bool,
) -> Result<()> {
    let change_reason = match state.change_reason.as_str() {
        "heartbeat" => "startup",
        other => other,
    };
    sqlx::query(
        r#"INSERT INTO system_states (
            hostname, 
            change_reason,
            store_path,
            generation,
            generation_matches_current_store_path,
            os, 
            kernel,
            memory_gb, 
            uptime_secs, 
            cpu_brand, 
            cpu_cores,
            board_serial, 
            product_uuid, 
            rootfs_uuid,
            chassis_serial,
            bios_version,
            cpu_microcode,
            network_interfaces,
            primary_mac_address,
            primary_ip_address,
            gateway_ip,
            selinux_status,
            tpm_present,
            secure_boot_enabled,
            fips_mode,
            agent_version,
            agent_build_hash,
            nixos_version,
            agent_compatible,
            partial_data
        ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25,$26,$27,$28,$29,$30)"#,
    )
    .bind(&state.hostname)
    .bind(change_reason)
    .bind(&state.store_path)
    .bind(state.generation)
    .bind(state.generation_matches_current_store_path)
    .bind(&state.os)
    .bind(&state.kernel)
    .bind(state.memory_gb)
    .bind(state.uptime_secs)
    .bind(&state.cpu_brand)
    .bind(state.cpu_cores)
    .bind(&state.board_serial)
    .bind(&state.product_uuid)
    .bind(&state.rootfs_uuid)
    .bind(&state.chassis_serial)
    .bind(&state.bios_version)
    .bind(&state.cpu_microcode)
    .bind(&state.network_interfaces)
    .bind(&state.primary_mac_address)
    .bind(&state.primary_ip_address)
    .bind(&state.gateway_ip)
    .bind(&state.selinux_status)
    .bind(state.tpm_present)
    .bind(state.secure_boot_enabled)
    .bind(state.fips_mode)
    .bind(&state.agent_version)
    .bind(&state.agent_build_hash)
    .bind(&state.nixos_version)
    .bind(version_compatible)  // $29
    .bind(!version_compatible) // $30 - partial_data flag
    .execute(pool)
    .await
    .map_err(|e| anyhow::anyhow!("SQL error: {e:?}"))?;

    // Optionally log incompatible agents for monitoring
    if !version_compatible {
        tracing::warn!(
            "Agent version incompatibility detected: host={} version={} - agent should be upgraded",
            state.hostname,
            state.agent_version.as_deref().unwrap_or("unknown")
        );
    }
    Ok(())
}
pub async fn get_last_system_state_by_hostname(
    pool: &PgPool,
    hostname: &str,
) -> Result<Option<SystemState>> {
    let row = sqlx::query_as::<_, SystemState>(
        r#"
        SELECT *
        FROM system_states
        WHERE hostname = $1
        ORDER BY timestamp DESC
        LIMIT 1
        "#,
    )
    .bind(hostname)
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

pub async fn get_latest_system_state_id(pool: &PgPool, hostname: &str) -> Result<Option<i32>> {
    let row = sqlx::query_scalar::<_, i32>(
        r#"
        SELECT id
        FROM system_states
        WHERE hostname = $1
        ORDER BY timestamp DESC
        LIMIT 1
        "#,
    )
    .bind(hostname)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Row type for system generation history
#[derive(Debug, sqlx::FromRow)]
pub struct SystemGenerationRow {
    pub generation: i32,
    pub store_path: Option<String>,
    pub commit_hash: Option<String>,
    pub timestamp: DateTime<Utc>,
}

/// Fetch historical generations for a system by its UUID
/// Returns generations in descending order (newest first)
pub async fn fetch_system_generations(
    pool: &PgPool,
    system_id: Uuid,
) -> Result<Vec<SystemGenerationRow>> {
    let rows = sqlx::query_as::<_, SystemGenerationRow>(
        r#"
        SELECT DISTINCT ON (ss.generation)
            ss.generation,
            ss.store_path,
            commit_link.commit_hash,
            ss.timestamp
        FROM system_states ss
        JOIN systems s ON s.hostname = ss.hostname
        LEFT JOIN LATERAL (
          SELECT c.git_commit_hash AS commit_hash
          FROM derivations d
          JOIN commits c ON c.id = d.commit_id
          WHERE ss.store_path IS NOT NULL
            AND ss.store_path = COALESCE(d.store_path, d.expected_store_path)
            AND d.derivation_type = 'nixos'
          ORDER BY d.id DESC
          LIMIT 1
        ) commit_link ON TRUE
        WHERE s.id = $1
          AND ss.generation IS NOT NULL
        ORDER BY ss.generation DESC, ss.timestamp DESC
        "#,
    )
    .bind(system_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn find_generation_store_path_last_seen(
    pool: &PgPool,
    system_id: Uuid,
    store_path: &str,
) -> Result<Option<DateTime<Utc>>> {
    let trimmed = store_path.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let row = sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
        r#"
        SELECT MAX(ss.timestamp)
        FROM system_states ss
        JOIN systems s ON s.hostname = ss.hostname
        WHERE s.id = $1
          AND ss.store_path = $2
        "#,
    )
    .bind(system_id)
    .bind(trimmed)
    .fetch_one(pool)
    .await?;

    Ok(row)
}

#[cfg(test)]
mod tests {
    use super::{fetch_system_generations, insert_system_state};

    use crate::handlers::agent_request::deserialize_system_state_versioned;
    use crate::models::public_key::PublicKey;
    use crate::models::system_states::{SystemState, SystemStateV1};
    use crate::models::systems::System;
    use crate::queries::commits::{get_commit_by_hash, insert_commit_with_metadata};
    use crate::queries::derivations::insert_derivation;
    use crate::queries::flakes::insert_flake;
    use crate::queries::systems::insert_system;
    use crate::test_utils::builders::SystemStateBuilder;

    use chrono::Utc;
    use ed25519_dalek::SigningKey;
    use sqlx::PgPool;
    use uuid::Uuid;

    async fn test_pool_from_env() -> PgPool {
        let db_url = std::env::var("DATABASE_URL")
            .expect("DATABASE_URL must be set for db-backed generation query tests");

        PgPool::connect(&db_url)
            .await
            .expect("failed to connect to DATABASE_URL")
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn fetch_system_generations_includes_all_generations_and_links_commit_when_available() {
        let pool = test_pool_from_env().await;
        let suffix = Uuid::new_v4().simple().to_string();
        let hostname = format!("task294-gen-test-{suffix}");
        let repo_url = format!("https://example.com/task294-{suffix}.git");
        let commit_hash = format!("task294{suffix}");

        let key = SigningKey::from_bytes(&[7u8; 32]);
        let public_key = PublicKey::from_verifying_key(key.verifying_key());

        let flake = insert_flake(&pool, &format!("flake-{suffix}"), &repo_url, "main", "full")
            .await
            .expect("insert_flake should succeed");

        let system = System {
            id: Uuid::new_v4(),
            hostname: hostname.clone(),
            environment_id: None,
            is_active: true,
            public_key,
            flake_id: Some(flake.id),
            derivation: String::new(),
            system_configuration_name: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            desired_target: None,
            deployment_policy: "manual".to_string(),
        };

        insert_system(&pool, &system)
            .await
            .expect("insert_system should succeed");

        let mut state_with_commit = SystemStateBuilder::new().build();
        state_with_commit.hostname = hostname.clone();
        state_with_commit.generation = Some(101);
        state_with_commit.store_path = Some(format!("/nix/store/{suffix}-gen101-system"));
        insert_system_state(&pool, &state_with_commit, true)
            .await
            .expect("insert_system_state with store_path should succeed");

        let mut state_without_commit = SystemStateBuilder::new().build();
        state_without_commit.hostname = hostname.clone();
        state_without_commit.generation = Some(100);
        state_without_commit.store_path = None;
        insert_system_state(&pool, &state_without_commit, true)
            .await
            .expect("insert_system_state without store_path should succeed");

        insert_commit_with_metadata(
            &pool,
            &commit_hash,
            &repo_url,
            Utc::now(),
            Some("TASK-294 test commit"),
            Some("test"),
        )
        .await
        .expect("insert_commit_with_metadata should succeed");

        let commit = get_commit_by_hash(&pool, &commit_hash)
            .await
            .expect("get_commit_by_hash should succeed");

        let derivation = insert_derivation(&pool, Some(&commit), &hostname, "nixos")
            .await
            .expect("insert_derivation should succeed");

        sqlx::query("UPDATE derivations SET store_path = $1 WHERE id = $2")
            .bind(state_with_commit.store_path.clone())
            .bind(derivation.id)
            .execute(&pool)
            .await
            .expect("updating derivation store_path should succeed");

        let generations = fetch_system_generations(&pool, system.id)
            .await
            .expect("fetch_system_generations should succeed");

        let gen_101 = generations
            .iter()
            .find(|g| g.generation == 101)
            .expect("generation 101 should exist");
        assert_eq!(gen_101.commit_hash.as_deref(), Some(commit_hash.as_str()));

        let gen_100 = generations
            .iter()
            .find(|g| g.generation == 100)
            .expect("generation 100 should exist");
        assert!(gen_100.store_path.is_none());
        assert!(gen_100.commit_hash.is_none());
    }

    #[test]
    fn test_try_deserialize_current_version() {
        let current_state = SystemState {
            id: None,
            hostname: "test-host".to_string(),
            change_reason: "test-context".to_string(),
            store_path: Some("/nix/store/test".to_string()),
            generation: Some(74),
            generation_matches_current_store_path: Some(true),
            os: Some("NixOS".to_string()),
            kernel: Some("6.1.0".to_string()),
            memory_gb: Some(16.0),
            uptime_secs: Some(3600),
            cpu_brand: Some("Test CPU".to_string()),
            cpu_cores: Some(8),
            board_serial: Some("TEST123".to_string()),
            product_uuid: Some("test-uuid".to_string()),
            rootfs_uuid: Some("root-uuid".to_string()),
            timestamp: None,
            chassis_serial: Some("chassis-123".to_string()),
            bios_version: Some("1.0".to_string()),
            cpu_microcode: Some("microcode-1".to_string()),
            network_interfaces: Some(
                serde_json::json!([{"name":"eth0","mac":"00:11:22:33:44:55"}]),
            ),
            primary_mac_address: Some("00:11:22:33:44:55".to_string()),
            primary_ip_address: Some("192.168.1.100".to_string()),
            gateway_ip: Some("192.168.1.1".to_string()),
            selinux_status: Some("disabled".to_string()),
            tpm_present: Some(true),
            secure_boot_enabled: Some(false),
            fips_mode: Some(false),
            agent_version: Some("1.0.0".to_string()),
            agent_build_hash: Some("abc123".to_string()),
            nixos_version: Some("23.05".to_string()),
            agent_compatible: Some(true),
            partial_data: Some(false),
        };

        let json = serde_json::to_vec(&current_state).unwrap();

        // Create a mock VerifiedAgentRequest for testing
        use crate::handlers::agent_request::VerifiedAgentRequest;
        use crate::models::systems::System;
        use ed25519_dalek::Signature;
        use uuid::Uuid;

        let mock_system = System {
            id: Uuid::new_v4(),
            hostname: "test".to_string(),
            environment_id: None,
            is_active: true,
            public_key: crate::models::public_key::PublicKey::from_base64(
                "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
                "test",
            )
            .unwrap(),
            flake_id: None,
            derivation: "test".to_string(),
            system_configuration_name: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            desired_target: None,
            deployment_policy: "manual".to_string(),
        };

        let mock_request = VerifiedAgentRequest {
            key_id: "test".to_string(),
            signature: Signature::from_bytes(&[0; 64]),
            system: mock_system,
            body: json.into(),
        };

        let (parsed, compatible) = deserialize_system_state_versioned(&mock_request).unwrap();

        assert!(compatible, "Current version should be compatible");
        assert_eq!(parsed.hostname, "test-host");
        assert_eq!(parsed.agent_version, Some("1.0.0".to_string()));
    }

    #[test]
    fn test_try_deserialize_v1_fallback() {
        let v1_state = SystemStateV1 {
            id: None,
            hostname: "test-host-v1".to_string(),
            context: "test-context".to_string(),
            store_path: Some("/nix/store/test".to_string()),
            os: Some("NixOS".to_string()),
            kernel: Some("6.1.0".to_string()),
            memory_gb: Some(16.0),
            uptime_secs: Some(3600),
            cpu_brand: Some("Test CPU".to_string()),
            cpu_cores: Some(8),
            board_serial: Some("TEST123".to_string()),
            product_uuid: Some("test-uuid".to_string()),
            rootfs_uuid: Some("root-uuid".to_string()),
            timestamp: None,
            chassis_serial: Some("chassis-123".to_string()),
            bios_version: Some("1.0".to_string()),
            cpu_microcode: Some("microcode-1".to_string()),
            network_interfaces: Some(
                serde_json::json!([{"name":"eth0","mac":"00:11:22:33:44:55"}]),
            ),
            primary_mac_address: Some("00:11:22:33:44:55".to_string()),
            primary_ip_address: Some("192.168.1.100".to_string()),
            gateway_ip: Some("192.168.1.1".to_string()),
            selinux_status: Some("disabled".to_string()),
            tpm_present: Some(true),
            secure_boot_enabled: Some(false),
            fips_mode: Some(false),
            agent_version: Some("1.0.0".to_string()),
            agent_build_hash: Some("abc123".to_string()),
            nixos_version: Some("23.05".to_string()),
        };

        let json = serde_json::to_vec(&v1_state).unwrap();

        // Create mock request
        use crate::handlers::agent_request::VerifiedAgentRequest;
        use crate::models::systems::System;
        use ed25519_dalek::Signature;
        use uuid::Uuid;

        let mock_system = System {
            id: Uuid::new_v4(),
            hostname: "test".to_string(),
            environment_id: None,
            is_active: true,
            public_key: crate::models::public_key::PublicKey::from_base64(
                "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
                "test",
            )
            .unwrap(),
            flake_id: None,
            derivation: "test".to_string(),
            system_configuration_name: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            desired_target: None,
            deployment_policy: "manual".to_string(),
        };

        let mock_request = VerifiedAgentRequest {
            key_id: "test".to_string(),
            signature: Signature::from_bytes(&[0; 64]),
            system: mock_system,
            body: json.into(),
        };

        let (parsed, compatible) = deserialize_system_state_versioned(&mock_request).unwrap();

        assert_eq!(parsed.hostname, "test-host-v1");
    }

    #[test]
    fn test_system_state_from_v1_conversion() {
        let v1 = SystemStateV1 {
            id: Some(1),
            hostname: "test".to_string(),
            context: "agent-startup".to_string(),
            store_path: None,
            os: Some("NixOS".to_string()),
            kernel: Some("6.1".to_string()),
            memory_gb: Some(8.0),
            uptime_secs: Some(1000),
            cpu_brand: Some("Intel".to_string()),
            cpu_cores: Some(4),
            board_serial: None,
            product_uuid: None,
            rootfs_uuid: None,
            timestamp: None,
            chassis_serial: None,
            bios_version: None,
            cpu_microcode: None,
            network_interfaces: None,
            primary_mac_address: None,
            primary_ip_address: None,
            gateway_ip: None,
            selinux_status: None,
            tpm_present: None,
            secure_boot_enabled: None,
            fips_mode: None,
            agent_version: None,
            agent_build_hash: None,
            nixos_version: None,
        };

        let current = SystemState::from_v1(v1);

        assert_eq!(current.hostname, "test");
        assert_eq!(current.change_reason, "startup");
        assert_eq!(current.os, Some("NixOS".to_string()));
        assert_eq!(current.agent_version, None);
        assert_eq!(current.nixos_version, None);
        assert_eq!(current.chassis_serial, None);
    }
}
