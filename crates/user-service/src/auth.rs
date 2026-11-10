// SPDX-License-Identifier: MIT

//! `POST /auth/signup` and `POST /auth/login` (RUST-2.5), as the Scala service
//! answers them (capture 20261109 http/signup-*.txt, login-*.txt).

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::domain::{AuthError, Credentials, Password};
use crate::events;
use crate::hashing::PasswordCheck;
use crate::responses::{JsonBody, TextError, json_error};
use crate::server::AppState;
use crate::store::CreateOutcome;
use crate::tokens::{self, TokenKind};

/// Credentials for a new account.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SignupRequest {
    /// Email address; trimmed and lower-cased before storage.
    #[schemars(example = &"user@example.com")]
    pub email: String,
    /// Plaintext password, at least 8 characters. Never stored or logged.
    #[schemars(example = &"correct-horse-battery-staple")]
    pub password: String,
}

/// Credentials for an existing account.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct LoginRequest {
    /// Email address; trimmed and lower-cased before lookup.
    #[schemars(example = &"user@example.com")]
    pub email: String,
    /// Plaintext password; must be non-empty. Never stored or logged.
    #[schemars(example = &"correct-horse-battery-staple")]
    pub password: String,
}

/// A new account and its first access token.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignupResponse {
    /// New user's id (UUID).
    #[schemars(example = &"5b6c7d8e-9f0a-4b1c-8d2e-3f4a5b6c7d8e")]
    pub user_id: String,
    /// Opaque access token (1-hour TTL). Shown once; store it client-side, never log it.
    #[schemars(example = &"example-access-token-not-valid")]
    pub token: String,
}

/// A successful login and a fresh token pair.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoginResponse {
    /// User's id (UUID).
    #[schemars(example = &"5b6c7d8e-9f0a-4b1c-8d2e-3f4a5b6c7d8e")]
    pub user_id: String,
    /// Opaque access token (1-hour TTL). Shown once; never log it.
    #[schemars(example = &"example-access-token-not-valid")]
    pub token: String,
    /// Opaque refresh token (30-day TTL). Only its hash is stored; never log it.
    #[schemars(example = &"example-refresh-token-not-valid")]
    pub refresh_token: String,
}

/// What each endpoint logs with every line (user-service `AuthRoutes`'
/// context maps).
const SIGNUP: [(&str, &str); 2] = [("method", "POST"), ("path", "/auth/signup")];
const LOGIN: [(&str, &str); 2] = [("method", "POST"), ("path", "/auth/login")];

fn with<'a>(
    context: &[(&'a str, &'a str)],
    name: &'a str,
    value: &'a str,
) -> Vec<(&'a str, &'a str)> {
    let mut fields = context.to_vec();
    fields.push((name, value));
    fields
}

/// A 500 for an unexpected failure, logged as an ERROR line.
pub(crate) fn internal(
    state: &AppState,
    context: &[(&str, &str)],
    error: impl std::fmt::Display,
) -> Response {
    let text = error.to_string();
    state
        .logger
        .error(&with(context, "error", &text), "Request failed");
    TextError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "Internal Server Error".to_owned(),
    }
    .into_response()
}

/// `POST /auth/signup`.
pub async fn signup(
    State(state): State<AppState>,
    JsonBody(request): JsonBody<SignupRequest>,
) -> Response {
    state.logger.info(&SIGNUP, "Received request");
    let credentials = match Credentials::signup(&request.email, &request.password) {
        Ok(credentials) => credentials,
        Err(error) => {
            let text = error.to_string();
            state
                .logger
                .warn(&with(&SIGNUP, "error", &text), "Invalid signup request");
            return json_error(StatusCode::BAD_REQUEST, error);
        }
    };
    let hasher = state.hasher.clone();
    let password = credentials.password.clone();
    let password_hash = match tokio::task::spawn_blocking(move || hasher.hash(&password)).await {
        Ok(hash) => hash,
        Err(error) => return internal(&state, &SIGNUP, error),
    };
    let user_id = Uuid::new_v4();
    let now = OffsetDateTime::now_utc();
    let token = tokens::issue(TokenKind::Access, state.auth.access_token_ttl, now);
    let event = events::user_registered(user_id, credentials.email.as_str(), now);
    match state
        .store
        .create_account(user_id, &credentials.email, &password_hash, &token, &event)
        .await
    {
        Ok(CreateOutcome::Created { .. }) => {
            let user_id = user_id.to_string();
            state
                .logger
                .info(&with(&SIGNUP, "user_id", &user_id), "Account created");
            (
                StatusCode::CREATED,
                Json(SignupResponse {
                    user_id,
                    token: token.value,
                }),
            )
                .into_response()
        }
        Ok(CreateOutcome::Duplicate) => {
            state.logger.warn(&SIGNUP, "Duplicate signup");
            json_error(StatusCode::CONFLICT, AuthError::DuplicateEmail)
        }
        Err(error) => internal(&state, &SIGNUP, error),
    }
}

/// `POST /auth/login`.
pub async fn login(
    State(state): State<AppState>,
    JsonBody(request): JsonBody<LoginRequest>,
) -> Response {
    state.logger.info(&LOGIN, "Received request");
    let credentials = match Credentials::login(&request.email, &request.password) {
        Ok(credentials) => credentials,
        Err(error) => {
            let text = error.to_string();
            state
                .logger
                .warn(&with(&LOGIN, "error", &text), "Invalid login request");
            return json_error(StatusCode::BAD_REQUEST, error);
        }
    };
    let account = match state.store.find_by_email(&credentials.email).await {
        Ok(account) => account,
        Err(error) => return internal(&state, &LOGIN, error),
    };
    let check = verify(
        &state,
        credentials.password,
        account.as_ref().map(|a| a.password_hash.clone()),
    )
    .await;
    let account = match (account, check) {
        (Some(account), Ok(PasswordCheck::Match)) => account,
        (Some(account), Ok(PasswordCheck::Malformed)) => {
            let user_id = account.id.to_string();
            state.logger.error(
                &with(&LOGIN, "user_id", &user_id),
                "Stored password hash is malformed",
            );
            return json_error(StatusCode::UNAUTHORIZED, AuthError::InvalidCredentials);
        }
        (_, Err(error)) => return internal(&state, &LOGIN, error),
        _ => {
            state.logger.warn(&LOGIN, "Login failed");
            return json_error(StatusCode::UNAUTHORIZED, AuthError::InvalidCredentials);
        }
    };
    let now = OffsetDateTime::now_utc();
    let access = tokens::issue(TokenKind::Access, state.auth.access_token_ttl, now);
    let refresh = tokens::issue(TokenKind::Refresh, state.auth.refresh_token_ttl, now);
    if let Err(error) = state
        .store
        .record_login_tokens(account.id, &access, &refresh)
        .await
    {
        return internal(&state, &LOGIN, error);
    }
    let user_id = account.id.to_string();
    state
        .logger
        .info(&with(&LOGIN, "user_id", &user_id), "Login succeeded");
    Json(LoginResponse {
        user_id,
        token: access.value,
        refresh_token: refresh.value,
    })
    .into_response()
}

/// Runs the (CPU-heavy) check off the async runtime; always one derivation.
async fn verify(
    state: &AppState,
    password: Password,
    stored: Option<String>,
) -> Result<PasswordCheck, tokio::task::JoinError> {
    let hasher = state.hasher.clone();
    tokio::task::spawn_blocking(move || hasher.verify(&password, stored.as_deref())).await
}
