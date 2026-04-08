use sqlx::PgPool;

use crate::config::AppConfig;
use crate::services::auth::AuthService;
use crate::services::email::EmailService;

/// Shared application state for all handlers.
#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub pool: PgPool,
    pub email_service: EmailService,
    pub auth_service: AuthService,
}
