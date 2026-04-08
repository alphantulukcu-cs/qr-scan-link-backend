use chrono::{DateTime, Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

/// Auth configuration used by JWT service.
#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub employee_username: String,
    pub employee_password: String,
    pub jwt_secret: String,
    pub jwt_expires_minutes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JwtClaims {
    sub: String,
    exp: i64,
    iat: i64,
}

/// Handles login checks and JWT generation/validation.
#[derive(Debug, Clone)]
pub struct AuthService {
    config: AuthConfig,
}

impl AuthService {
    /// Creates a new auth service from loaded app config.
    pub fn new(config: AuthConfig) -> Self {
        Self { config }
    }

    /// Verifies employee credentials and issues a signed JWT.
    pub fn login(&self, username: &str, password: &str) -> Result<(String, DateTime<Utc>)> {
        if username.trim() != self.config.employee_username
            || password != self.config.employee_password
        {
            return Err(AppError::unauthorized("kullanici adi veya sifre hatali"));
        }

        let now = Utc::now();
        let expires_at = now + Duration::minutes(self.config.jwt_expires_minutes);
        let claims = JwtClaims {
            sub: self.config.employee_username.clone(),
            exp: expires_at.timestamp(),
            iat: now.timestamp(),
        };

        let token = encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(self.config.jwt_secret.as_bytes()),
        )
        .map_err(|error| AppError::internal(format!("jwt olusturulamadi: {error}")))?;

        Ok((token, expires_at))
    }

    /// Verifies a JWT and returns authenticated username.
    pub fn verify_token(&self, token: &str) -> Result<String> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;

        let data = decode::<JwtClaims>(
            token,
            &DecodingKey::from_secret(self.config.jwt_secret.as_bytes()),
            &validation,
        )
        .map_err(|_| AppError::unauthorized("gecersiz veya suresi dolmus token"))?;

        let _expires_at = DateTime::<Utc>::from_timestamp(data.claims.exp, 0)
            .ok_or_else(|| AppError::unauthorized("gecersiz token suresi"))?;

        Ok(data.claims.sub)
    }
}
