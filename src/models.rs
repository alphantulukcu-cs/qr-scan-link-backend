use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// Request body for creating a new customer scan invite.
#[derive(Debug, Deserialize)]
pub struct CreateInviteRequest {
    pub customer_national_id: String,
    pub customer_email: String,
}

/// Response body for a created invite.
#[derive(Debug, Serialize)]
pub struct CreateInviteResponse {
    pub invite_id: Uuid,
    pub one_time_link: String,
    pub expires_at: DateTime<Utc>,
    pub email_dispatched: bool,
}

/// Lightweight invite info returned to branch UI.
#[derive(Debug, Serialize)]
pub struct InviteSummaryResponse {
    pub invite_id: Uuid,
    pub status: String,
    pub customer_national_id: String,
    pub customer_email: String,
    pub check_count: i64,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub submitted_at: Option<DateTime<Utc>>,
}

/// Single submitted cheque row.
#[derive(Debug, Serialize)]
pub struct SubmittedCheckResponse {
    pub sequence_no: i32,
    pub qr_value: String,
    pub image_data_url: String,
    pub captured_at: DateTime<Utc>,
    pub metadata: Option<Value>,
}

/// Branch detail payload with all submitted cheque data.
#[derive(Debug, Serialize)]
pub struct InviteDetailResponse {
    pub invite: InviteSummaryResponse,
    pub batch_image_data_url: Option<String>,
    pub session_metadata: Option<Value>,
    pub checks: Vec<SubmittedCheckResponse>,
}

/// Public response after an invite is claimed on customer side.
#[derive(Debug, Serialize)]
pub struct ClaimInviteResponse {
    pub invite_id: Uuid,
    pub session_token: String,
    pub customer_national_id: String,
    pub customer_email: String,
    pub expires_at: DateTime<Utc>,
}

/// Single cheque payload sent from qr-scanner-ui.
#[derive(Debug, Deserialize)]
pub struct SubmitCheckItemRequest {
    pub sequence_no: i32,
    pub qr_value: String,
    pub image_data_url: String,
    pub captured_at: DateTime<Utc>,
    pub metadata: Option<Value>,
}

/// Submit payload for full scan session.
#[derive(Debug, Deserialize)]
pub struct SubmitSessionRequest {
    pub checks: Vec<SubmitCheckItemRequest>,
    pub batch_image_data_url: Option<String>,
    pub completed_at: Option<DateTime<Utc>>,
    pub session_metadata: Option<Value>,
}

/// Public response after a session submission.
#[derive(Debug, Serialize)]
pub struct SubmitSessionResponse {
    pub invite_id: Uuid,
    pub submitted_at: DateTime<Utc>,
    pub check_count: i64,
}

/// Health endpoint payload.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub now: DateTime<Utc>,
}
