use keyring::Entry;

use crate::domain::entities::provider::ProviderKind;
use crate::domain::error::{DomainError, DomainResult};
use crate::domain::services::SecretStore;

const SERVICE: &str = "com.spcsft.codebase-notebook";

/// Stores provider API keys in the OS keychain (macOS Keychain, Windows
/// Credential Manager). Keys never touch SQLite or log output.
pub struct KeyringSecretStore;

impl KeyringSecretStore {
    fn entry(kind: ProviderKind) -> DomainResult<Entry> {
        Entry::new(SERVICE, &format!("api-key-{}", kind.as_str()))
            .map_err(|e| DomainError::SecretStore(format!("open keychain entry: {e}")))
    }
}

impl SecretStore for KeyringSecretStore {
    fn set_api_key(&self, kind: ProviderKind, api_key: &str) -> DomainResult<()> {
        Self::entry(kind)?
            .set_password(api_key)
            .map_err(|e| DomainError::SecretStore(format!("store key: {e}")))
    }

    fn get_api_key(&self, kind: ProviderKind) -> DomainResult<Option<String>> {
        match Self::entry(kind)?.get_password() {
            Ok(key) => Ok(Some(key)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(DomainError::SecretStore(format!("read key: {e}"))),
        }
    }

    fn delete_api_key(&self, kind: ProviderKind) -> DomainResult<()> {
        match Self::entry(kind)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(DomainError::SecretStore(format!("delete key: {e}"))),
        }
    }
}
