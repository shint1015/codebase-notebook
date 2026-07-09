use rusqlite::params;

use super::{storage_err, Db};
use crate::domain::error::{DomainError, DomainResult};
use crate::domain::services::SettingsRepository;

pub struct SqliteSettingsRepository {
    db: Db,
}

impl SqliteSettingsRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

impl SettingsRepository for SqliteSettingsRepository {
    fn get(&self, key: &str) -> DomainResult<Option<String>> {
        self.db
            .lock()
            .query_row(
                "SELECT value FROM app_settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(DomainError::Storage(format!("get setting: {other}"))),
            })
    }

    fn set(&self, key: &str, value: &str) -> DomainResult<()> {
        self.db
            .lock()
            .execute(
                "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )
            .map_err(storage_err("set setting"))?;
        Ok(())
    }
}
