//! External service connectors exposed to the agent as tools. Each connector
//! reads its token from the OS keychain (never the DB) and only acts on an
//! explicit, consented agent tool call.

pub mod asana;
pub mod backlog;
pub mod confluence;
pub mod notion;
pub mod slack;

use std::sync::Arc;
use std::time::Duration;

use crate::domain::error::{DomainError, DomainResult};
use crate::domain::services::SecretStore;

/// Keychain key for a connector's token, e.g. "connector-slack".
pub fn secret_key(connector: &str) -> String {
    format!("connector-{connector}")
}

/// Shared helper: fetch a connector token or a clear "not configured" error.
pub(crate) fn require_token(
    secrets: &Arc<dyn SecretStore>,
    connector: &str,
) -> DomainResult<String> {
    secrets
        .get_secret(&secret_key(connector))?
        .filter(|t| !t.trim().is_empty())
        .ok_or_else(|| {
            DomainError::ProviderNotConfigured(format!(
                "{connector} is not connected — add its token in settings"
            ))
        })
}

pub(crate) fn connector_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("reqwest client")
}

/// All connector kinds the app knows about (for settings UI + status).
pub const CONNECTOR_KINDS: &[&str] = &["slack", "notion", "asana", "backlog", "confluence"];
