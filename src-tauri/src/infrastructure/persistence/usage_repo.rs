use rusqlite::params;

use super::{storage_err, Db};
use crate::domain::entities::usage::UsageRecord;
use crate::domain::error::DomainResult;
use crate::domain::repositories::UsageRepository;

pub struct SqliteUsageRepository {
    db: Db,
}

impl SqliteUsageRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

impl UsageRepository for SqliteUsageRepository {
    fn append(&self, record: &UsageRecord) -> DomainResult<()> {
        let sources_json = serde_json::to_string(&record.sources).unwrap_or_else(|_| "[]".into());
        self.db
            .lock()
            .execute(
                "INSERT INTO usage_log
                 (id, created_at, provider, model, workspace_id, prompt_chars,
                  completion_chars, est_cost_usd, sources_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    record.id,
                    record.created_at,
                    record.provider,
                    record.model,
                    record.workspace_id,
                    record.prompt_chars,
                    record.completion_chars,
                    record.est_cost_usd,
                    sources_json,
                ],
            )
            .map_err(storage_err("insert usage"))?;
        Ok(())
    }

    fn list_recent(&self, limit: usize) -> DomainResult<Vec<UsageRecord>> {
        let conn = self.db.lock();
        let mut stmt = conn
            .prepare(
                "SELECT id, created_at, provider, model, workspace_id, prompt_chars,
                        completion_chars, est_cost_usd, sources_json
                 FROM usage_log ORDER BY created_at DESC LIMIT ?1",
            )
            .map_err(storage_err("prepare list usage"))?;
        let rows = stmt
            .query_map(params![limit as i64], |row| {
                let sources_json: String = row.get(8)?;
                Ok(UsageRecord {
                    id: row.get(0)?,
                    created_at: row.get(1)?,
                    provider: row.get(2)?,
                    model: row.get(3)?,
                    workspace_id: row.get(4)?,
                    prompt_chars: row.get(5)?,
                    completion_chars: row.get(6)?,
                    est_cost_usd: row.get(7)?,
                    sources: serde_json::from_str(&sources_json).unwrap_or_default(),
                })
            })
            .map_err(storage_err("list usage"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(storage_err("read usage"))
    }

    fn month_total_usd(&self, provider: &str, month: &str) -> DomainResult<f64> {
        self.db
            .lock()
            .query_row(
                "SELECT COALESCE(SUM(est_cost_usd), 0.0) FROM usage_log
                 WHERE provider = ?1 AND created_at LIKE ?2 || '%'",
                params![provider, month],
                |row| row.get(0),
            )
            .map_err(storage_err("sum usage"))
    }
}
