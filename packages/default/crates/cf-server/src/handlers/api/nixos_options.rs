use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};

use crate::api::models::{ApiError, NixosOptionsSearchQuery};
use crate::auth::extractors::RequireAuth;
use crate::handlers::agent_request::CFState;
use crate::nixos_options_metadata::{
    DEFAULT_SEARCH_LIMIT, MetadataProviderError, NixosOptionsMetadataProvider,
};

pub async fn search_nixos_options(
    State(state): State<CFState>,
    Query(query): Query<NixosOptionsSearchQuery>,
    _user: RequireAuth,
) -> Response {
    search_response(
        &state.nixos_options_metadata,
        &query.query,
        query.limit.unwrap_or(DEFAULT_SEARCH_LIMIT),
    )
}

fn search_response(provider: &NixosOptionsMetadataProvider, query: &str, limit: usize) -> Response {
    match provider.search(query, limit) {
        Ok(options) => (StatusCode::OK, Json(options)).into_response(),
        Err(MetadataProviderError::Unavailable(message)) => metadata_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "metadata_unavailable",
            message,
        ),
        Err(MetadataProviderError::Corrupt(message)) => metadata_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "metadata_corrupt",
            message,
        ),
    }
}

fn metadata_error(status: StatusCode, error: &str, message: String) -> Response {
    (
        status,
        Json(ApiError {
            error: error.to_string(),
            message,
            details: None,
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::NixosOptionMetadata;
    use axum::body::to_bytes;

    fn provider() -> NixosOptionsMetadataProvider {
        NixosOptionsMetadataProvider::from_json_bytes(
            br#"[{"path":"services.demo.enable","value_type":"boolean"}]"#,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn returns_injected_fixture_results() {
        let response = search_response(&provider(), "demo", 10);
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let entries: Vec<NixosOptionMetadata> = serde_json::from_slice(&body).unwrap();
        assert_eq!(entries[0].path, "services.demo.enable");
    }

    #[tokio::test]
    async fn reports_unavailable_and_corrupt_distinctly() {
        let unavailable = NixosOptionsMetadataProvider::from_path("/missing/options.json");
        assert_eq!(
            search_response(&unavailable, "", 10).status(),
            StatusCode::SERVICE_UNAVAILABLE
        );

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("options.json");
        std::fs::write(&path, b"{").unwrap();
        let corrupt = NixosOptionsMetadataProvider::from_path(path);
        assert_eq!(
            search_response(&corrupt, "", 10).status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
