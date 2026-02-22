//! Local username/password authentication endpoints.
//!
//! Provides traditional username/password authentication for self-hosted deployments
//! that don't have OIDC configured.

use axum::{
    Json,
    extract::ConnectInfo,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::net::SocketAddr;

use crate::auth::password::{hash_password, verify_password};
use crate::handlers::api::auth_session::establish_user_session;
use crate::queries::users::{get_by_email, get_by_username, insert_user};

/// Request payload for user registration.
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub email: String,
    pub password: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
}

/// Response from successful registration.
#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    pub user_id: String,
    pub username: String,
    pub email: String,
}

/// Register a new local user.
///
/// Creates a user with a hashed password for local authentication.
/// If this is the first user in the system, they are automatically assigned Admin role.
pub async fn register(
    State(pool): State<PgPool>,
    Json(payload): Json<RegisterRequest>,
) -> Result<impl IntoResponse, LocalAuthError> {
    // Validate password strength
    if payload.password.len() < 8 {
        return Err(LocalAuthError::WeakPassword);
    }

    // Check if username already exists
    if let Some(_existing) = get_by_username(&pool, &payload.username)
        .await
        .map_err(|_| LocalAuthError::DatabaseError)?
    {
        return Err(LocalAuthError::UsernameTaken);
    }

    // Check if email already exists
    if let Some(_existing) = get_by_email(&pool, &payload.email)
        .await
        .map_err(|_| LocalAuthError::DatabaseError)?
    {
        return Err(LocalAuthError::EmailTaken);
    }

    // Check if this is the first user (initial setup)
    let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&pool)
        .await
        .map_err(|_| LocalAuthError::DatabaseError)?;
    
    let is_first_user = user_count == 0;

    // Hash password
    let password_hash =
        hash_password(&payload.password).map_err(|_| LocalAuthError::PasswordHashingFailed)?;

    // Create display name from first/last name if provided
    let display_name = match (&payload.first_name, &payload.last_name) {
        (Some(first), Some(last)) => Some(format!("{} {}", first, last)),
        (Some(first), None) => Some(first.clone()),
        (None, Some(last)) => Some(last.clone()),
        (None, None) => None,
    };

    // Create user
    let mut user = insert_user(&pool, &payload.email, display_name.as_deref())
        .await
        .map_err(|_| LocalAuthError::DatabaseError)?;

    // Update username and password hash
    // Note: insert_user generates username from email, but we want to use the provided username
    sqlx::query("UPDATE users SET username = $1, password_hash = $2 WHERE id = $3")
        .bind(&payload.username)
        .bind(&password_hash)
        .bind(user.id)
        .execute(&pool)
        .await
        .map_err(|_| LocalAuthError::DatabaseError)?;

    user.username = payload.username.clone();

    // Assign role: Admin if first user, otherwise require admin to assign roles
    if is_first_user {
        use crate::models::auth_identity::AuthRole;
        use crate::queries::auth_identity::assign_role_to_user;
        
        assign_role_to_user(&pool, user.id, AuthRole::Admin, None)
            .await
            .map_err(|_| LocalAuthError::DatabaseError)?;
        
        tracing::info!("Registered FIRST local user as Admin: {} ({})", user.email, user.id);
    } else {
        tracing::info!("Registered new local user: {} ({})", user.email, user.id);
    }

    Ok((
        StatusCode::CREATED,
        Json(RegisterResponse {
            user_id: user.id.to_string(),
            username: user.username,
            email: user.email,
        }),
    ))
}

/// Request payload for local login.
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    /// Username or email
    pub username: String,
    pub password: String,
}

/// Response from successful login.
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub user_id: String,
    pub username: String,
    pub email: String,
}

/// Authenticate with username/password.
///
/// Verifies credentials and creates a session.
pub async fn login(
    State(pool): State<PgPool>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(payload): Json<LoginRequest>,
) -> Result<impl IntoResponse, LocalAuthError> {
    // Try to find user by username first, then email
    let user = match get_by_username(&pool, &payload.username).await {
        Ok(Some(user)) => Some(user),
        Ok(None) => get_by_email(&pool, &payload.username)
            .await
            .map_err(|_| LocalAuthError::DatabaseError)?,
        Err(_) => return Err(LocalAuthError::DatabaseError),
    };

    let user = user.ok_or(LocalAuthError::InvalidCredentials)?;

    // Get password hash
    let password_hash =
        sqlx::query_scalar::<_, Option<String>>("SELECT password_hash FROM users WHERE id = $1")
            .bind(user.id)
            .fetch_one(&pool)
            .await
            .map_err(|_| LocalAuthError::DatabaseError)?
            .ok_or(LocalAuthError::InvalidCredentials)?; // No password hash means OIDC-only user

    // Verify password
    verify_password(&payload.password, &password_hash)
        .map_err(|_| LocalAuthError::InvalidCredentials)?;

    tracing::info!("User logged in: {} ({})", user.email, user.id);

    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(ToString::to_string);

    let ip_address = Some(addr.ip().to_string());

    let session_cookies = establish_user_session(&pool, user.id, user_agent, ip_address)
        .await
        .map_err(|_| LocalAuthError::SessionCreationFailed)?;

    let mut response = Json(LoginResponse {
        user_id: user.id.to_string(),
        username: user.username,
        email: user.email,
    })
    .into_response();

    response
        .headers_mut()
        .append(header::SET_COOKIE, session_cookies.session_cookie);
    response
        .headers_mut()
        .append(header::SET_COOKIE, session_cookies.csrf_cookie);

    Ok(response)
}

/// Local authentication error responses.
#[derive(Debug)]
pub enum LocalAuthError {
    WeakPassword,
    UsernameTaken,
    EmailTaken,
    PasswordHashingFailed,
    InvalidCredentials,
    SessionCreationFailed,
    DatabaseError,
}

impl IntoResponse for LocalAuthError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            LocalAuthError::WeakPassword => (
                StatusCode::BAD_REQUEST,
                "Password must be at least 8 characters long".to_string(),
            ),
            LocalAuthError::UsernameTaken => {
                (StatusCode::CONFLICT, "Username already taken".to_string())
            }
            LocalAuthError::EmailTaken => {
                (StatusCode::CONFLICT, "Email already registered".to_string())
            }
            LocalAuthError::PasswordHashingFailed => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to hash password".to_string(),
            ),
            LocalAuthError::InvalidCredentials => (
                StatusCode::UNAUTHORIZED,
                "Invalid username or password".to_string(),
            ),
            LocalAuthError::SessionCreationFailed => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to create authenticated session".to_string(),
            ),
            LocalAuthError::DatabaseError => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Database error".to_string(),
            ),
        };

        (status, message).into_response()
    }
}
