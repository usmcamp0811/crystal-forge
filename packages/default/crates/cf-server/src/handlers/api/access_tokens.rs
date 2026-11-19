//! Browser-session HTTP API for owner access tokens.
//!
//! Routes (all under `/api/v1/user`): `GET access-token-policy`,
//! `GET|POST access-tokens`, `GET|DELETE access-tokens/:id` and
//! `POST access-tokens/:id/rotate`.
//!
//! SECURITY: These endpoints accept only a browser session cookie. No Admin
//! guard applies; Viewer, Operator and Admin manage their own tokens. A
//! `cfat_` bearer credential never authenticates here, and a request that
//! carries one is rejected with 401 even when it also carries a cookie.
//! `POST` and `DELETE` also require the double-submit CSRF pair. Every
//! response that these handlers build, including their error responses, sets
//! `Cache-Control: no-store`, because the create and rotate responses carry the
//! only plaintext copy of a secret. Responses that the framework builds before
//! a handler runs or without one do not carry the header: extractor
//! rejections (for example an unreadable request body) and router 404 and 405
//! responses. None of those responses contains a secret, because a secret
//! appears only in the 201 response of a successful create or rotate. No code
//! here logs a request body or a response body.
//!
//! Kill switch: when `access_tokens_enable` is `false`, create and rotate
//! return 403 `access_tokens_disabled`. Policy, list, detail and revoke keep
//! working so an owner can still see and revoke tokens.
//!
//! Error bodies are `{"error": code, "message": text, "details": null|object}`.

use std::collections::{BTreeMap, HashMap};

use axum::{
    Json,
    body::Bytes,
    extract::{Path, RawQuery, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::api::models::{AccessTokenSecretDto, CreateAccessTokenRequest};
use crate::config::ServerConfig;
use crate::handlers::api::auth_session::require_csrf;
use crate::handlers::api::rbac::{authenticated_user_roles, has_viewer_or_above_role};
use crate::services::access_tokens::{
    self as service, AccessTokenError, AccessTokenSettings, IssuedAccessToken, ListQuery,
};

/// Sets `Cache-Control: no-store` on a response that a handler in this module
/// builds. The framework does not call it for extractor rejections or router
/// 404 and 405 responses.
fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn json_response(status: StatusCode, body: Value) -> Response {
    no_store((status, Json(body)).into_response())
}

fn error_response(
    status: StatusCode,
    code: &str,
    message: &str,
    details: Option<Value>,
) -> Response {
    json_response(
        status,
        json!({ "error": code, "message": message, "details": details }),
    )
}

fn respond<T: Serialize>(status: StatusCode, value: &T) -> Response {
    match serde_json::to_value(value) {
        Ok(body) => json_response(status, body),
        Err(err) => {
            tracing::error!(error = %err, "access token response serialization failed");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "Could not complete the access token request",
                None,
            )
        }
    }
}

fn respond_issued(status: StatusCode, issued: IssuedAccessToken) -> Response {
    // SECURITY: This is the only place the plaintext leaves the service.
    let body = AccessTokenSecretDto {
        token: issued.secret.expose().to_owned(),
        replaced_token_id: issued.replaced_token_id,
        metadata: issued.metadata,
    };
    respond(status, &body)
}

fn unauthorized() -> Response {
    error_response(
        StatusCode::UNAUTHORIZED,
        "unauthorized",
        "Authentication required",
        None,
    )
}

fn validation_failed(fields: BTreeMap<String, String>) -> Response {
    error_response(
        StatusCode::BAD_REQUEST,
        "validation_failed",
        "One or more fields are invalid",
        Some(json!({ "fields": fields })),
    )
}

fn not_found() -> Response {
    error_response(
        StatusCode::NOT_FOUND,
        "resource_not_found",
        "Access token not found",
        None,
    )
}

fn service_error(err: AccessTokenError) -> Response {
    match err {
        AccessTokenError::Validation(fields) => validation_failed(fields),
        AccessTokenError::NotFound => not_found(),
        AccessTokenError::Conflict { message, details } => {
            error_response(StatusCode::CONFLICT, "conflict", &message, details)
        }
        AccessTokenError::RateLimited => error_response(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "At most 10 access tokens can be created or rotated per hour",
            None,
        ),
        AccessTokenError::Disabled => error_response(
            StatusCode::FORBIDDEN,
            "access_tokens_disabled",
            "Access tokens are disabled on this server",
            None,
        ),
        AccessTokenError::OwnerUnavailable => unauthorized(),
        AccessTokenError::Internal(err) => {
            tracing::error!(error = %err, "access token request failed");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "Could not complete the access token request",
                None,
            )
        }
    }
}

/// Reports whether the request presents an access token in any
/// `Authorization` header.
///
/// SECURITY: This check fails closed. The value is split on ASCII whitespace,
/// so repeated spaces and tabs do not hide a credential. A value counts as
/// presented when any part starts with `cfat_`, ignoring ASCII case. That
/// covers the `Bearer` scheme in any letter case and a credential that has no
/// scheme. A value that is not visible ASCII also counts as presented, because
/// the check cannot show that it holds no token. The caller answers 401.
fn bearer_access_token_presented(headers: &HeaderMap) -> bool {
    headers
        .get_all(header::AUTHORIZATION)
        .iter()
        .any(|value| match value.to_str() {
            Ok(text) => text.split_ascii_whitespace().any(starts_with_token_prefix),
            Err(_) => true,
        })
}

/// Reports whether `part` starts with the access-token prefix `cfat_`,
/// ignoring ASCII case.
fn starts_with_token_prefix(part: &str) -> bool {
    part.as_bytes()
        .get(..5)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"cfat_"))
}

/// Authenticates the browser session and returns the owner id.
///
/// Uses the session helper that checks session expiry, invalidation and the
/// active flag. Viewer, Operator and Admin pass. No Admin check exists.
async fn authenticate(pool: &PgPool, headers: &HeaderMap) -> Result<Uuid, Response> {
    if bearer_access_token_presented(headers) {
        return Err(unauthorized());
    }
    match authenticated_user_roles(pool, headers).await {
        None => Err(unauthorized()),
        Some((user_id, roles)) if has_viewer_or_above_role(&roles) => Ok(user_id),
        Some(_) => Err(error_response(
            StatusCode::FORBIDDEN,
            "forbidden",
            "Insufficient permissions",
            None,
        )),
    }
}

/// Authenticates and then requires the CSRF pair, in that order.
async fn authenticate_mutation(pool: &PgPool, headers: &HeaderMap) -> Result<Uuid, Response> {
    let owner = authenticate(pool, headers).await?;
    require_csrf(headers).map_err(no_store)?;
    Ok(owner)
}

fn settings(config: &ServerConfig) -> AccessTokenSettings {
    // SECURITY: An unreadable freshness bound becomes zero seconds, which
    // denies every OIDC-owned account and never relaxes the check.
    let max_age = crate::config::entitlement_max_age_seconds_from_env().unwrap_or(0);
    AccessTokenSettings::from_server_config(config, max_age)
}

fn token_id(raw: &str) -> Option<Uuid> {
    Uuid::parse_str(raw).ok()
}

/// Returns the owner's access-token policy: limits, selectable environments
/// and scopes, and any create block. Readable when tokens are disabled.
pub async fn get_policy(
    State(pool): State<PgPool>,
    State(config): State<ServerConfig>,
    headers: HeaderMap,
) -> Response {
    let owner = match authenticate(&pool, &headers).await {
        Ok(owner) => owner,
        Err(response) => return response,
    };
    match service::policy(&pool, owner, &settings(&config)).await {
        Ok(policy) => respond(StatusCode::OK, &policy),
        Err(err) => service_error(err),
    }
}

/// Lists the owner's tokens. Query: `status`, `q`, `limit`, `offset`. Readable
/// when tokens are disabled.
pub async fn list_access_tokens(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let owner = match authenticate(&pool, &headers).await {
        Ok(owner) => owner,
        Err(response) => return response,
    };
    let params: HashMap<String, String> =
        url::form_urlencoded::parse(raw_query.unwrap_or_default().as_bytes())
            .into_owned()
            .collect();
    let mut fields = BTreeMap::new();
    let mut integer = |key: &str| -> Option<i64> {
        let value = params.get(key)?;
        let parsed = value.trim().parse::<i64>().ok();
        if parsed.is_none() {
            fields.insert(key.to_owned(), format!("{key} must be an integer"));
        }
        parsed
    };
    let limit = integer("limit");
    let offset = integer("offset");
    if !fields.is_empty() {
        return validation_failed(fields);
    }
    let query = ListQuery {
        status: params.get("status").cloned(),
        q: params.get("q").cloned(),
        limit,
        offset,
    };
    match service::list(&pool, owner, &query).await {
        Ok(page) => respond(StatusCode::OK, &page),
        Err(err) => service_error(err),
    }
}

/// Creates a token. Returns 201 with the metadata and the plaintext `token`,
/// which no later response contains.
pub async fn create_access_token(
    State(pool): State<PgPool>,
    State(config): State<ServerConfig>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let owner = match authenticate_mutation(&pool, &headers).await {
        Ok(owner) => owner,
        Err(response) => return response,
    };
    // The error text is not echoed: it can quote request values.
    let Ok(request) = serde_json::from_slice::<CreateAccessTokenRequest>(&body) else {
        return validation_failed(BTreeMap::from([(
            "body".to_owned(),
            "body must be a JSON object that has only the documented fields with valid types"
                .to_owned(),
        )]));
    };
    match service::create(&pool, owner, &request, &settings(&config)).await {
        Ok(issued) => respond_issued(StatusCode::CREATED, issued),
        Err(err) => service_error(err),
    }
}

/// Returns one of the owner's tokens with effective access and recent events.
/// Another user's token returns 404. Readable when tokens are disabled.
pub async fn get_access_token(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let owner = match authenticate(&pool, &headers).await {
        Ok(owner) => owner,
        Err(response) => return response,
    };
    let Some(id) = token_id(&id) else {
        return not_found();
    };
    match service::detail(&pool, owner, id).await {
        Ok(detail) => respond(StatusCode::OK, &detail),
        Err(err) => service_error(err),
    }
}

/// Revokes one of the owner's tokens. Repeating the request returns 200 again.
pub async fn revoke_access_token(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let owner = match authenticate_mutation(&pool, &headers).await {
        Ok(owner) => owner,
        Err(response) => return response,
    };
    let Some(id) = token_id(&id) else {
        return not_found();
    };
    match service::revoke(&pool, owner, id).await {
        Ok(token) => respond(StatusCode::OK, &token),
        Err(err) => service_error(err),
    }
}

/// Rotates one of the owner's active tokens. Returns 201 like create, plus
/// `replaced_token_id`.
pub async fn rotate_access_token(
    State(pool): State<PgPool>,
    State(config): State<ServerConfig>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let owner = match authenticate_mutation(&pool, &headers).await {
        Ok(owner) => owner,
        Err(response) => return response,
    };
    let Some(id) = token_id(&id) else {
        return not_found();
    };
    match service::rotate(&pool, owner, id, &settings(&config)).await {
        Ok(issued) => respond_issued(StatusCode::CREATED, issued),
        Err(err) => service_error(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers_with(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, value.parse().unwrap());
        headers
    }

    #[test]
    fn detects_access_token_bearer_credentials_only() {
        assert!(bearer_access_token_presented(&headers_with(
            "Bearer cfat_v1_abc"
        )));
        assert!(bearer_access_token_presented(&headers_with(
            "  bearer cfat_v1_abc"
        )));
        assert!(!bearer_access_token_presented(&headers_with(
            "Bearer something-else"
        )));
        assert!(!bearer_access_token_presented(&HeaderMap::new()));
        assert!(!bearer_access_token_presented(&headers_with(
            "Basic dXNlcjpwYXNz"
        )));
    }

    #[test]
    fn detects_a_bearer_credential_despite_whitespace_and_case() {
        for value in [
            "Bearer  cfat_v1_abc",
            "Bearer   cfat_v1_abc",
            "Bearer\tcfat_v1_abc",
            "Bearer \t cfat_v1_abc",
            "BEARER cfat_v1_abc",
            "bEaReR cfat_v1_abc",
            "Bearer CFAT_V1_ABC",
            "cfat_v1_abc",
        ] {
            assert!(
                bearer_access_token_presented(&headers_with(value)),
                "{value:?} was not detected"
            );
        }
    }

    #[test]
    fn a_non_utf8_authorization_value_counts_as_presented() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_bytes(b"Bearer \xff\xfe").unwrap(),
        );
        assert!(bearer_access_token_presented(&headers));
    }

    #[test]
    fn any_authorization_header_value_can_present_a_token() {
        let mut headers = headers_with("Basic dXNlcjpwYXNz");
        headers.append(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer  cfat_v1_abc"),
        );
        assert!(bearer_access_token_presented(&headers));
    }

    #[test]
    fn responses_set_no_store_including_errors() {
        for response in [
            unauthorized(),
            not_found(),
            service_error(AccessTokenError::RateLimited),
            service_error(AccessTokenError::Disabled),
        ] {
            assert_eq!(
                response.headers().get(header::CACHE_CONTROL).unwrap(),
                "no-store"
            );
        }
    }
}
