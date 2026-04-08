use std::env;
use std::net::SocketAddr;

use crate::error::{AppError, Result};

const DEFAULT_APP_ADDR: &str = "0.0.0.0:8095";
const DEFAULT_INVITE_BASE_URL: &str = "http://127.0.0.1:5174/capture";
const DEFAULT_INVITE_TTL_MINUTES: i64 = 120;
const DEFAULT_CORS_ALLOWED_ORIGINS: &str =
    "http://127.0.0.1:5173,http://localhost:5173,http://127.0.0.1:5174,http://localhost:5174";
const DEFAULT_SERVICE_NAME: &str = "scan-link-backend";
const DEFAULT_JWT_EXPIRES_MINUTES: i64 = 480;
const DEFAULT_SMTP_PORT: u16 = 587;
const DEFAULT_SMTP_FROM_NAME: &str = "Sekerbank Sube Operasyon";

/// SMTP configuration.
#[derive(Debug, Clone)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub from_address: String,
    pub from_name: String,
    pub logo_url: Option<String>,
}

/// Runtime application configuration.
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub app_addr: SocketAddr,
    pub database_url: String,
    pub invite_base_url: String,
    pub invite_ttl_minutes: i64,
    pub cors_allowed_origins: Vec<String>,
    pub service_name: String,
    pub employee_username: String,
    pub employee_password: String,
    pub jwt_secret: String,
    pub jwt_expires_minutes: i64,
    pub smtp: Option<SmtpConfig>,
}

impl AppConfig {
    /// Loads the backend configuration from environment variables.
    pub fn from_env() -> Result<Self> {
        let app_addr_raw = get_env_or_default("APP_ADDR", DEFAULT_APP_ADDR)?;
        let app_addr = app_addr_raw
            .parse::<SocketAddr>()
            .map_err(|error| AppError::config(format!("APP_ADDR gecersiz: {error}")))?;

        let database_url = get_required_env("DATABASE_URL")?;

        let invite_base_url = get_env_or_default("INVITE_BASE_URL", DEFAULT_INVITE_BASE_URL)?;
        if url::Url::parse(&invite_base_url).is_err() {
            return Err(AppError::config(
                "INVITE_BASE_URL gecersiz bir URL olmali".to_string(),
            ));
        }

        let invite_ttl_minutes = get_env_or_default(
            "INVITE_TTL_MINUTES",
            &DEFAULT_INVITE_TTL_MINUTES.to_string(),
        )?
        .parse::<i64>()
        .map_err(|error| {
            AppError::config(format!("INVITE_TTL_MINUTES gecersiz bir sayi olmali: {error}"))
        })?;

        if invite_ttl_minutes <= 0 {
            return Err(AppError::config(
                "INVITE_TTL_MINUTES sifirdan buyuk olmali".to_string(),
            ));
        }

        let cors_allowed_origins = get_env_or_default(
            "CORS_ALLOWED_ORIGINS",
            DEFAULT_CORS_ALLOWED_ORIGINS,
        )?
        .split(',')
        .map(str::trim)
        .filter(|origin| !origin.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

        if cors_allowed_origins.is_empty() {
            return Err(AppError::config(
                "CORS_ALLOWED_ORIGINS en az bir origin icermeli".to_string(),
            ));
        }

        let service_name = get_env_or_default("SERVICE_NAME", DEFAULT_SERVICE_NAME)?;
        let employee_username = get_required_env("EMPLOYEE_USERNAME")?;
        let employee_password = get_required_env("EMPLOYEE_PASSWORD")?;
        let jwt_secret = get_required_env("JWT_SECRET")?;
        let jwt_expires_minutes = get_env_or_default(
            "JWT_EXPIRES_MINUTES",
            &DEFAULT_JWT_EXPIRES_MINUTES.to_string(),
        )?
        .parse::<i64>()
        .map_err(|error| {
            AppError::config(format!(
                "JWT_EXPIRES_MINUTES gecersiz bir sayi olmali: {error}"
            ))
        })?;

        if jwt_expires_minutes <= 0 {
            return Err(AppError::config(
                "JWT_EXPIRES_MINUTES sifirdan buyuk olmali".to_string(),
            ));
        }

        if jwt_secret.len() < 16 {
            return Err(AppError::config(
                "JWT_SECRET en az 16 karakter olmali".to_string(),
            ));
        }

        let smtp = load_smtp_config()?;

        Ok(Self {
            app_addr,
            database_url,
            invite_base_url,
            invite_ttl_minutes,
            cors_allowed_origins,
            service_name,
            employee_username,
            employee_password,
            jwt_secret,
            jwt_expires_minutes,
            smtp,
        })
    }
}

fn get_required_env(key: &str) -> Result<String> {
    match env::var(key) {
        Ok(value) => {
            let trimmed = value.trim().to_string();
            if trimmed.is_empty() {
                Err(AppError::config(format!("{key} bos birakilamaz")))
            } else {
                Ok(trimmed)
            }
        }
        Err(_) => Err(AppError::config(format!("{key} tanimli degil"))),
    }
}

fn get_env_or_default(key: &str, default: &str) -> Result<String> {
    match env::var(key) {
        Ok(value) => {
            let trimmed = value.trim().to_string();
            if trimmed.is_empty() {
                Err(AppError::config(format!("{key} bos birakilamaz")))
            } else {
                Ok(trimmed)
            }
        }
        Err(_) => Ok(default.to_string()),
    }
}

fn load_smtp_config() -> Result<Option<SmtpConfig>> {
    let host = env::var("SMTP_HOST").ok().map(|value| value.trim().to_string());
    let username = env::var("SMTP_USERNAME")
        .ok()
        .map(|value| value.trim().to_string());
    let password = env::var("SMTP_PASSWORD")
        .ok()
        .map(|value| value.trim().to_string());
    let from_address = env::var("SMTP_FROM_ADDRESS")
        .ok()
        .map(|value| value.trim().to_string());

    let smtp_defined = host.is_some() || username.is_some() || password.is_some() || from_address.is_some();

    if !smtp_defined {
        return Ok(None);
    }

    let resolved_host = host
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::config("SMTP_HOST tanimli olmali".to_string()))?;
    let resolved_username = username
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::config("SMTP_USERNAME tanimli olmali".to_string()))?;
    let resolved_password = password
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::config("SMTP_PASSWORD tanimli olmali".to_string()))?;
    let resolved_from_address = from_address
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::config("SMTP_FROM_ADDRESS tanimli olmali".to_string()))?;

    let port = match env::var("SMTP_PORT") {
        Ok(value) => value
            .trim()
            .parse::<u16>()
            .map_err(|error| AppError::config(format!("SMTP_PORT gecersiz: {error}")))?,
        Err(_) => DEFAULT_SMTP_PORT,
    };

    let from_name = match env::var("SMTP_FROM_NAME") {
        Ok(value) => {
            let trimmed = value.trim().to_string();
            if trimmed.is_empty() {
                DEFAULT_SMTP_FROM_NAME.to_string()
            } else {
                trimmed
            }
        }
        Err(_) => DEFAULT_SMTP_FROM_NAME.to_string(),
    };

    let logo_url = match env::var("SMTP_LOGO_URL") {
        Ok(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                url::Url::parse(trimmed)
                    .map_err(|error| AppError::config(format!("SMTP_LOGO_URL gecersiz: {error}")))?;
                Some(trimmed.to_string())
            }
        }
        Err(_) => None,
    };

    Ok(Some(SmtpConfig {
        host: resolved_host,
        port,
        username: resolved_username,
        password: resolved_password,
        from_address: resolved_from_address,
        from_name,
        logo_url,
    }))
}
