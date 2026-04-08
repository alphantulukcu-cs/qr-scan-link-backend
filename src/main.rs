mod config;
mod error;
mod handlers;
mod models;
mod services;
mod state;
mod telemetry;

use tokio::net::TcpListener;
use tracing::info;

use crate::config::AppConfig;
use crate::error::{AppError, Result};
use crate::handlers::router;
use crate::services::auth::{AuthConfig, AuthService};
use crate::services::email::EmailService;
use crate::state::AppState;

/// Program entrypoint.
#[tokio::main]
async fn main() {
    if run().await.is_err() {
        std::process::exit(1);
    }
}

#[tracing::instrument]
async fn run() -> Result<()> {
    dotenvy::dotenv().ok();

    let config = AppConfig::from_env()?;
    telemetry::init_telemetry(&config.service_name)?;

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.database_url)
        .await?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|error| AppError::internal(format!("migration calismadi: {error}")))?;

    let email_service = EmailService::new(config.smtp.clone())?;
    let auth_service = AuthService::new(AuthConfig {
        employee_username: config.employee_username.clone(),
        employee_password: config.employee_password.clone(),
        jwt_secret: config.jwt_secret.clone(),
        jwt_expires_minutes: config.jwt_expires_minutes,
    });
    let state = AppState {
        config,
        pool,
        email_service,
        auth_service,
    };

    let bind_addr = state.config.app_addr;
    let app = router(state)?;
    let listener = TcpListener::bind(bind_addr)
        .await
        .map_err(|error| AppError::internal(format!("port bind edilemedi: {error}")))?;

    info!(%bind_addr, "scan-link-backend ayaga kalkti");

    axum::serve(listener, app)
        .await
        .map_err(|error| AppError::internal(format!("http sunucusu hata verdi: {error}")))
}
