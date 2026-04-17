use uuid::Uuid;

use crate::api::{
    self,
    models::{
        BuilderStatus, UpdateBuilderEnvironmentsRequest, UpdateBuilderPublicKeyRequest,
        UpdateBuilderRequest,
    },
};

pub fn build_update_request(
    name: &str,
    status: BuilderStatus,
    max_cpu_cores: &str,
    max_memory_mb: &str,
    max_concurrent_jobs: &str,
) -> UpdateBuilderRequest {
    UpdateBuilderRequest {
        name: if name.trim().is_empty() {
            None
        } else {
            Some(name.trim().to_string())
        },
        status: Some(status),
        max_cpu_cores: max_cpu_cores.trim().parse::<i32>().ok(),
        max_memory_mb: max_memory_mb.trim().parse::<i32>().ok(),
        max_concurrent_jobs: max_concurrent_jobs.trim().parse::<i32>().ok(),
    }
}

pub async fn submit_builder_update(
    builder_id: &Uuid,
    update_request: &UpdateBuilderRequest,
    environment_ids: Vec<Uuid>,
) -> Result<(), String> {
    api::client::update_builder(builder_id, update_request)
        .await
        .map_err(|e| format!("Failed to update builder: {e}"))?;

    let env_request = UpdateBuilderEnvironmentsRequest { environment_ids };
    api::client::update_builder_environments(builder_id, &env_request)
        .await
        .map_err(|e| format!("Failed to update environments: {e}"))
}

pub async fn apply_builder_public_key(builder_id: &Uuid, public_key: String) -> Result<(), String> {
    let request = UpdateBuilderPublicKeyRequest { public_key };
    api::client::update_builder_public_key(builder_id, &request)
        .await
        .map(|_| ())
        .map_err(|e| format!("Failed to update builder key: {e}"))
}

pub async fn deactivate_builder(builder_id: &Uuid) -> Result<(), String> {
    api::client::deactivate_builder(builder_id)
        .await
        .map_err(|e| format!("Failed to deactivate builder: {e}"))
}

pub async fn delete_builder_permanently(builder_id: &Uuid) -> Result<(), String> {
    api::client::delete_builder_permanently(builder_id)
        .await
        .map_err(|e| format!("Failed to delete builder: {e}"))
}
