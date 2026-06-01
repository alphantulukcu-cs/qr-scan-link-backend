use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::Request;
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Duration, Utc};
use serde_json::Value;
use sqlx::types::Uuid;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::{instrument, warn};

use crate::error::{AppError, Result};
use crate::models::{
    ClaimInviteResponse, CreateInviteRequest, CreateInviteResponse, EmployeeLoginRequest,
    EmployeeLoginResponse, HealthResponse, InviteDetailResponse, InviteSummaryResponse,
    SubmitSessionRequest, SubmitSessionResponse, SubmittedCheckResponse,
};
use crate::services::token::{build_invite_link, derive_session_token, generate_token, hash_token};
use crate::state::AppState;

const MAX_INVITE_LIST_SIZE: i64 = 200;
const SESSION_TOKEN_HEADER: &str = "x-session-token";
const MAX_REQUEST_BODY_BYTES: usize = 120 * 1024 * 1024;

/// Builds the application router and middleware stack.
pub fn router(state: AppState) -> Result<Router> {
    let cors = build_cors_layer(&state.config.cors_allowed_origins)?;

    let branch_routes = Router::new()
        .route("/api/branch/invites", post(create_invite).get(list_invites))
        .route("/api/branch/invites/{invite_id}", get(get_invite_detail))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_branch_auth,
        ));

    let app = Router::new()
        .route("/api/health", get(health))
        .route("/api/auth/login", post(login_employee))
        .route("/api/public/invites/{invite_token}/claim", get(claim_invite))
        .route("/api/public/sessions/{invite_id}/submit", post(submit_session))
        .merge(branch_routes)
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BODY_BYTES))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state);

    Ok(app)
}

#[derive(Debug, sqlx::FromRow)]
struct InviteSummaryRow {
    invite_id: Uuid,
    status: String,
    customer_national_id: String,
    customer_email: String,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    claimed_at: Option<DateTime<Utc>>,
    submitted_at: Option<DateTime<Utc>>,
    check_count: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct CheckRow {
    sequence_no: i32,
    qr_value: String,
    image_data_url: String,
    captured_at: DateTime<Utc>,
    metadata: Option<Value>,
}

#[derive(Debug, sqlx::FromRow)]
struct ClaimRow {
    invite_id: Uuid,
    status: String,
    customer_national_id: String,
    customer_email: String,
    expires_at: DateTime<Utc>,
    submitted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, sqlx::FromRow)]
struct SubmissionAuthRow {
    status: String,
    expires_at: DateTime<Utc>,
    session_token_hash: Option<String>,
    submitted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, sqlx::FromRow)]
struct BatchImageRow {
    batch_image_data_url: Option<String>,
    session_metadata: Option<Value>,
}

/// Health check endpoint.
#[instrument(skip(_state))]
async fn health(State(_state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        now: Utc::now(),
    })
}

/// Authenticates branch employee and returns JWT.
#[instrument(skip(state, payload))]
async fn login_employee(
    State(state): State<AppState>,
    Json(payload): Json<EmployeeLoginRequest>,
) -> Result<Json<EmployeeLoginResponse>> {
    let username = validate_non_empty("username", &payload.username)?;
    let password = validate_non_empty("password", &payload.password)?;
    let (token, expires_at) = state.auth_service.login(&username, &password)?;

    Ok(Json(EmployeeLoginResponse {
        token,
        username,
        expires_at,
    }))
}

/// Creates invite, stores it in PostgreSQL, and attempts to send email.
#[instrument(skip(state, payload))]
async fn create_invite(
    State(state): State<AppState>,
    Json(payload): Json<CreateInviteRequest>,
) -> Result<Json<CreateInviteResponse>> {
    let customer_national_id = validate_turkish_id("customer_national_id", &payload.customer_national_id)?;
    let customer_email = validate_email("customer_email", &payload.customer_email)?;

    let one_time_token = generate_token();
    let one_time_token_hash = hash_token(&one_time_token);
    let one_time_link = build_invite_link(&state.config.invite_base_url, &one_time_token);

    let invite_id = Uuid::new_v4();
    let expires_at = Utc::now() + Duration::minutes(state.config.invite_ttl_minutes);

    sqlx::query(
        "INSERT INTO scan_invites (
            invite_id,
            one_time_token_hash,
            customer_national_id,
            customer_email,
            status,
            expires_at
        ) VALUES (
            $1,$2,$3,$4,'pending',$5
        )",
    )
    .bind(invite_id)
    .bind(one_time_token_hash)
    .bind(&customer_national_id)
    .bind(&customer_email)
    .bind(expires_at)
    .execute(&state.pool)
    .await?;

    let email_dispatched = match state
        .email_service
        .send_invite_email(
            &customer_email,
            &customer_national_id,
            &one_time_link,
            expires_at,
        )
        .await
    {
        Ok(value) => value,
        Err(error) => {
            warn!(error = %error, "invite mail gonderimi basarisiz, invite kaydi korunuyor");
            false
        }
    };

    Ok(Json(CreateInviteResponse {
        invite_id,
        one_time_link,
        expires_at,
        email_dispatched,
    }))
}

/// Lists latest invite summaries for branch dashboard.
#[instrument(skip(state))]
async fn list_invites(State(state): State<AppState>) -> Result<Json<Vec<InviteSummaryResponse>>> {
    let rows = sqlx::query_as::<_, InviteSummaryRow>(
        "SELECT
            si.invite_id,
            si.status,
            si.customer_national_id,
            si.customer_email,
            si.created_at,
            si.expires_at,
            si.claimed_at,
            si.submitted_at,
            COALESCE(COUNT(sc.id), 0)::BIGINT AS check_count
        FROM scan_invites si
        LEFT JOIN scan_checks sc ON sc.invite_id = si.invite_id
        GROUP BY si.invite_id
        ORDER BY si.created_at DESC
        LIMIT $1",
    )
    .bind(MAX_INVITE_LIST_SIZE)
    .fetch_all(&state.pool)
    .await?;

    let now = Utc::now();
    let response = rows
        .into_iter()
        .map(|row| map_invite_summary_row(row, now))
        .collect::<Vec<_>>();

    Ok(Json(response))
}

/// Fetches detail including cheque images/metadata for one invite.
#[instrument(skip(state))]
async fn get_invite_detail(
    State(state): State<AppState>,
    Path(invite_id): Path<Uuid>,
) -> Result<Json<InviteDetailResponse>> {
    let maybe_row = sqlx::query_as::<_, InviteSummaryRow>(
        "SELECT
            si.invite_id,
            si.status,
            si.customer_national_id,
            si.customer_email,
            si.created_at,
            si.expires_at,
            si.claimed_at,
            si.submitted_at,
            COALESCE(COUNT(sc.id), 0)::BIGINT AS check_count
        FROM scan_invites si
        LEFT JOIN scan_checks sc ON sc.invite_id = si.invite_id
        WHERE si.invite_id = $1
        GROUP BY si.invite_id",
    )
    .bind(invite_id)
    .fetch_optional(&state.pool)
    .await?;

    let summary_row = maybe_row.ok_or_else(|| AppError::not_found("invite bulunamadi"))?;

    let checks = sqlx::query_as::<_, CheckRow>(
        "SELECT
            sequence_no,
            qr_value,
            image_data_url,
            captured_at,
            metadata
        FROM scan_checks
        WHERE invite_id = $1
        ORDER BY sequence_no ASC",
    )
    .bind(invite_id)
    .fetch_all(&state.pool)
    .await?
    .into_iter()
    .map(|row| SubmittedCheckResponse {
        sequence_no: row.sequence_no,
        qr_value: row.qr_value,
        image_data_url: row.image_data_url,
        captured_at: row.captured_at,
        metadata: row.metadata,
    })
    .collect::<Vec<_>>();

    let batch_image_row = sqlx::query_as::<_, BatchImageRow>(
        "SELECT batch_image_data_url, session_metadata FROM scan_invites WHERE invite_id = $1",
    )
    .bind(invite_id)
    .fetch_one(&state.pool)
    .await?;

    let now = Utc::now();
    let summary = map_invite_summary_row(summary_row, now);

    Ok(Json(InviteDetailResponse {
        invite: summary,
        batch_image_data_url: batch_image_row.batch_image_data_url,
        session_metadata: batch_image_row.session_metadata,
        checks,
    }))
}

/// Claims invite token and returns session token for scanner submission.
#[instrument(skip(state))]
async fn claim_invite(
    State(state): State<AppState>,
    Path(invite_token): Path<String>,
) -> Result<Json<ClaimInviteResponse>> {
    let token = invite_token.trim();
    if token.is_empty() {
        return Err(AppError::invalid_input("invite token bos olamaz"));
    }

    let invite_token_hash = hash_token(token);
    let mut transaction = state.pool.begin().await?;

    let row = sqlx::query_as::<_, ClaimRow>(
        "SELECT
            invite_id,
            status,
            customer_national_id,
            customer_email,
            expires_at,
            submitted_at
        FROM scan_invites
        WHERE one_time_token_hash = $1
        FOR UPDATE",
    )
    .bind(invite_token_hash)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or_else(|| AppError::not_found("gecersiz veya bulunamayan davet linki"))?;

    if row.submitted_at.is_some() || row.status == "submitted" {
        return Err(AppError::conflict("bu linkle islem zaten tamamlandi"));
    }

    if row.expires_at <= Utc::now() {
        sqlx::query("UPDATE scan_invites SET status = 'expired' WHERE invite_id = $1")
            .bind(row.invite_id)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        return Err(AppError::expired("linkin suresi dolmus"));
    }

    let session_token = derive_session_token(token);
    let session_token_hash = hash_token(&session_token);

    sqlx::query(
        "UPDATE scan_invites
        SET
            session_token_hash = $1,
            status = 'claimed',
            claimed_at = COALESCE(claimed_at, NOW()),
            claim_count = claim_count + 1
        WHERE invite_id = $2",
    )
    .bind(session_token_hash)
    .bind(row.invite_id)
    .execute(&mut *transaction)
    .await?;

    transaction.commit().await?;

    let response = ClaimInviteResponse {
        invite_id: row.invite_id,
        session_token,
        customer_national_id: row.customer_national_id,
        customer_email: row.customer_email,
        expires_at: row.expires_at,
    };

    Ok(Json(response))
}

/// Accepts scanned cheque batch and finalizes the invite.
#[instrument(skip(state, headers, payload))]
async fn submit_session(
    State(state): State<AppState>,
    Path(invite_id): Path<Uuid>,
    headers: HeaderMap,
    Json(payload): Json<SubmitSessionRequest>,
) -> Result<Json<SubmitSessionResponse>> {
    const MAX_CHECKS_PER_SESSION: usize = 50;

    if payload.checks.is_empty() {
        return Err(AppError::invalid_input("en az bir cek gonderilmelidir"));
    }

    if payload.checks.len() > MAX_CHECKS_PER_SESSION {
        return Err(AppError::invalid_input(
            "oturum basina maksimum 50 cek gonderilebilir",
        ));
    }

    for item in &payload.checks {
        if item.sequence_no <= 0 {
            return Err(AppError::invalid_input("sequence_no sifirdan buyuk olmali"));
        }

        if item.qr_value.trim().is_empty() {
            return Err(AppError::invalid_input("qr_value bos olamaz"));
        }

        if item.image_data_url.trim().is_empty() {
            return Err(AppError::invalid_input("image_data_url bos olamaz"));
        }

        let validation_image = item
            .original_image_data_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| item.image_data_url.trim());

        let validation = crate::services::image_validation::validate_check_image(
            validation_image,
            item.qr_value.trim(),
        )?;

        tracing::debug!(
            sequence_no = item.sequence_no,
            validation_source = if item
                .original_image_data_url
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_some()
            {
                "original"
            } else {
                "processed"
            },
            qr_match = validation.qr_match,
            decoded_qr_present = validation.decoded_qr.is_some(),
            "cek gorseli dogrulandi"
        );
    }

    let session_token = extract_session_token(&headers)?;
    let session_token_hash = hash_token(&session_token);

    let mut transaction = state.pool.begin().await?;

    let auth_row = sqlx::query_as::<_, SubmissionAuthRow>(
        "SELECT
            status,
            expires_at,
            session_token_hash,
            submitted_at
        FROM scan_invites
        WHERE invite_id = $1
        FOR UPDATE",
    )
    .bind(invite_id)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or_else(|| AppError::not_found("invite bulunamadi"))?;

    if auth_row.submitted_at.is_some() || auth_row.status == "submitted" {
        return Err(AppError::conflict("bu oturum zaten gonderilmis"));
    }

    if auth_row.expires_at <= Utc::now() {
        sqlx::query("UPDATE scan_invites SET status = 'expired' WHERE invite_id = $1")
            .bind(invite_id)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        return Err(AppError::expired("linkin suresi dolmus"));
    }

    let stored_hash = auth_row
        .session_token_hash
        .ok_or_else(|| AppError::unauthorized("oturum tokeni bulunamadi"))?;

    if stored_hash != session_token_hash {
        return Err(AppError::unauthorized("gecersiz oturum tokeni"));
    }

    sqlx::query("DELETE FROM scan_checks WHERE invite_id = $1")
        .bind(invite_id)
        .execute(&mut *transaction)
        .await?;

    for item in &payload.checks {
        sqlx::query(
            "INSERT INTO scan_checks (
                invite_id,
                sequence_no,
                qr_value,
                image_data_url,
                captured_at,
                metadata
            ) VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(invite_id)
        .bind(item.sequence_no)
        .bind(item.qr_value.trim())
        .bind(item.image_data_url.trim())
        .bind(item.captured_at)
        .bind(item.metadata.as_ref())
        .execute(&mut *transaction)
        .await?;
    }

    let submitted_at = payload.completed_at.unwrap_or_else(Utc::now);

    sqlx::query(
        "UPDATE scan_invites
        SET
            status = 'submitted',
            submitted_at = $1,
            batch_image_data_url = $2,
            session_metadata = $3,
            session_token_hash = NULL
        WHERE invite_id = $4",
    )
    .bind(submitted_at)
    .bind(payload.batch_image_data_url.as_deref())
    .bind(payload.session_metadata.as_ref())
    .bind(invite_id)
    .execute(&mut *transaction)
    .await?;

    transaction.commit().await?;

    Ok(Json(SubmitSessionResponse {
        invite_id,
        submitted_at,
        check_count: payload.checks.len() as i64,
    }))
}

async fn require_branch_auth(
    State(state): State<AppState>,
    request: Request<axum::body::Body>,
    next: Next,
) -> std::result::Result<Response, AppError> {
    let token = extract_bearer_token(request.headers())?;
    let _username = state.auth_service.verify_token(&token)?;
    Ok(next.run(request).await)
}

fn extract_session_token(headers: &HeaderMap) -> Result<String> {
    let header_value = headers
        .get(SESSION_TOKEN_HEADER)
        .ok_or_else(|| AppError::unauthorized("x-session-token header gerekli"))?;

    let token = header_value
        .to_str()
        .map_err(|error| AppError::unauthorized(format!("x-session-token gecersiz: {error}")))?
        .trim()
        .to_string();

    if token.is_empty() {
        return Err(AppError::unauthorized("x-session-token bos olamaz"));
    }

    Ok(token)
}

fn extract_bearer_token(headers: &HeaderMap) -> Result<String> {
    let header_value = headers
        .get(axum::http::header::AUTHORIZATION)
        .ok_or_else(|| AppError::unauthorized("authorization header gerekli"))?;
    let raw = header_value
        .to_str()
        .map_err(|error| AppError::unauthorized(format!("authorization gecersiz: {error}")))?;
    let token = raw
        .strip_prefix("Bearer ")
        .ok_or_else(|| AppError::unauthorized("authorization Bearer token formatinda olmali"))?
        .trim();

    if token.is_empty() {
        return Err(AppError::unauthorized("authorization token bos olamaz"));
    }

    Ok(token.to_string())
}

fn validate_non_empty(field_name: &str, value: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::invalid_input(format!("{field_name} bos olamaz")));
    }

    Ok(trimmed.to_string())
}

fn validate_turkish_id(field_name: &str, value: &str) -> Result<String> {
    let normalized = validate_non_empty(field_name, value)?;
    if normalized.len() != 11 || !normalized.chars().all(|ch| ch.is_ascii_digit()) {
        return Err(AppError::invalid_input(format!(
            "{field_name} 11 haneli sayisal bir deger olmali"
        )));
    }

    Ok(normalized)
}

fn validate_email(field_name: &str, value: &str) -> Result<String> {
    let normalized = validate_non_empty(field_name, value)?;
    if !normalized.contains('@') || normalized.starts_with('@') || normalized.ends_with('@') {
        return Err(AppError::invalid_input(format!(
            "{field_name} gecersiz email formatinda"
        )));
    }

    Ok(normalized)
}

fn map_invite_summary_row(row: InviteSummaryRow, now: DateTime<Utc>) -> InviteSummaryResponse {
    InviteSummaryResponse {
        invite_id: row.invite_id,
        status: resolve_status(
            &row.status,
            row.expires_at,
            row.claimed_at,
            row.submitted_at,
            now,
        ),
        customer_national_id: row.customer_national_id,
        customer_email: row.customer_email,
        check_count: row.check_count,
        created_at: row.created_at,
        expires_at: row.expires_at,
        claimed_at: row.claimed_at,
        submitted_at: row.submitted_at,
    }
}

fn resolve_status(
    status: &str,
    expires_at: DateTime<Utc>,
    claimed_at: Option<DateTime<Utc>>,
    submitted_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> String {
    if submitted_at.is_some() || status == "submitted" {
        return "submitted".to_string();
    }

    if expires_at <= now {
        return "expired".to_string();
    }

    if claimed_at.is_some() || status == "claimed" {
        return "claimed".to_string();
    }

    "pending".to_string()
}

fn build_cors_layer(origins: &[String]) -> Result<CorsLayer> {
    if origins.iter().any(|origin| origin == "*") {
        return Ok(CorsLayer::new()
            .allow_origin(Any)
            .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
            .allow_headers([
                CONTENT_TYPE,
                HeaderName::from_static("authorization"),
                HeaderName::from_static(SESSION_TOKEN_HEADER),
            ]));
    }

    let parsed_origins = origins
        .iter()
        .map(|origin| {
            HeaderValue::from_str(origin).map_err(|error| {
                AppError::config(format!("CORS origin gecersiz ({origin}): {error}"))
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(CorsLayer::new()
        .allow_origin(parsed_origins)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([
            CONTENT_TYPE,
            HeaderName::from_static("authorization"),
            HeaderName::from_static(SESSION_TOKEN_HEADER),
        ]))
}
