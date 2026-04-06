use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Generates a URL-safe token used for invite/session flows.
pub fn generate_token() -> String {
    let left = Uuid::new_v4().simple().to_string();
    let right = Uuid::new_v4().simple().to_string();
    format!("{left}{right}")
}

/// Hashes a token with SHA-256 for database storage.
pub fn hash_token(raw_token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw_token.as_bytes());
    let digest = hasher.finalize();
    format!("{digest:x}")
}

/// Derives a stable session token from invite token so repeated claims stay usable.
pub fn derive_session_token(invite_token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"scan-session:");
    hasher.update(invite_token.as_bytes());
    let digest = hasher.finalize();
    format!("{digest:x}")
}

/// Builds a public invite link by appending token to base capture URL.
pub fn build_invite_link(base_url: &str, token: &str) -> String {
    let normalized = base_url.trim_end_matches('/');
    format!("{normalized}/{token}")
}
