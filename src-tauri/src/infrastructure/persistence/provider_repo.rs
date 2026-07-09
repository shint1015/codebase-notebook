use rusqlite::params;

use super::{storage_err, Db};
use crate::domain::entities::provider::{ProviderConfig, ProviderKind};
use crate::domain::error::{DomainError, DomainResult};
use crate::domain::repositories::ProviderConfigRepository;

pub struct SqliteProviderConfigRepository {
    db: Db,
}

impl SqliteProviderConfigRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

fn row_to_config(row: &rusqlite::Row<'_>) -> rusqlite::Result<Option<ProviderConfig>> {
    let kind_str: String = row.get(0)?;
    let Some(kind) = ProviderKind::parse(&kind_str) else {
        return Ok(None);
    };
    Ok(Some(ProviderConfig {
        kind,
        enabled: row.get::<_, i64>(1)? != 0,
        base_url: row.get(2)?,
        default_model: row.get(3)?,
        allow_send_code: row.get::<_, i64>(4)? != 0,
        monthly_budget_usd: row.get(5)?,
        has_api_key: false,
    }))
}

impl ProviderConfigRepository for SqliteProviderConfigRepository {
    fn upsert(&self, config: &ProviderConfig) -> DomainResult<()> {
        self.db
            .lock()
            .execute(
                "INSERT INTO provider_configs (kind, enabled, base_url, default_model, allow_send_code, monthly_budget_usd)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(kind) DO UPDATE SET
                   enabled = excluded.enabled,
                   base_url = excluded.base_url,
                   default_model = excluded.default_model,
                   allow_send_code = excluded.allow_send_code,
                   monthly_budget_usd = excluded.monthly_budget_usd",
                params![
                    config.kind.as_str(),
                    config.enabled as i64,
                    config.base_url,
                    config.default_model,
                    config.allow_send_code as i64,
                    config.monthly_budget_usd,
                ],
            )
            .map_err(storage_err("upsert provider config"))?;
        Ok(())
    }

    fn find(&self, kind: ProviderKind) -> DomainResult<Option<ProviderConfig>> {
        self.db
            .lock()
            .query_row(
                "SELECT kind, enabled, base_url, default_model, allow_send_code, monthly_budget_usd
                 FROM provider_configs WHERE kind = ?1",
                params![kind.as_str()],
                row_to_config,
            )
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(DomainError::Storage(format!("find provider: {other}"))),
            })
    }

    fn list(&self) -> DomainResult<Vec<ProviderConfig>> {
        let conn = self.db.lock();
        let mut stmt = conn
            .prepare(
                "SELECT kind, enabled, base_url, default_model, allow_send_code, monthly_budget_usd
                 FROM provider_configs",
            )
            .map_err(storage_err("prepare list providers"))?;
        let rows = stmt
            .query_map([], row_to_config)
            .map_err(storage_err("list providers"))?;
        let mut configs = Vec::new();
        for row in rows {
            if let Some(config) = row.map_err(storage_err("read provider"))? {
                configs.push(config);
            }
        }
        Ok(configs)
    }
}
