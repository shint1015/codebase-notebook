use rusqlite::params;

use super::{storage_err, Db};
use crate::domain::entities::workspace::Workspace;
use crate::domain::error::{DomainError, DomainResult};
use crate::domain::repositories::WorkspaceRepository;

pub struct SqliteWorkspaceRepository {
    db: Db,
}

impl SqliteWorkspaceRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

fn row_to_workspace(row: &rusqlite::Row<'_>) -> rusqlite::Result<Workspace> {
    Ok(Workspace {
        id: row.get(0)?,
        name: row.get(1)?,
        allow_external: row.get::<_, i64>(2)? != 0,
        created_at: row.get(3)?,
    })
}

impl WorkspaceRepository for SqliteWorkspaceRepository {
    fn create(&self, workspace: &Workspace) -> DomainResult<()> {
        self.db
            .lock()
            .execute(
                "INSERT INTO workspaces (id, name, allow_external, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    workspace.id,
                    workspace.name,
                    workspace.allow_external as i64,
                    workspace.created_at,
                ],
            )
            .map_err(storage_err("insert workspace"))?;
        Ok(())
    }

    fn find_by_id(&self, id: &str) -> DomainResult<Workspace> {
        self.db
            .lock()
            .query_row(
                "SELECT id, name, allow_external, created_at
                 FROM workspaces WHERE id = ?1",
                params![id],
                row_to_workspace,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    DomainError::NotFound(format!("workspace {id}"))
                }
                other => DomainError::Storage(format!("find workspace: {other}")),
            })
    }

    fn list(&self) -> DomainResult<Vec<Workspace>> {
        let conn = self.db.lock();
        let mut stmt = conn
            .prepare(
                "SELECT id, name, allow_external, created_at
                 FROM workspaces ORDER BY created_at DESC",
            )
            .map_err(storage_err("prepare list workspaces"))?;
        let rows = stmt
            .query_map([], row_to_workspace)
            .map_err(storage_err("list workspaces"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(storage_err("read workspaces"))
    }

    fn set_allow_external(&self, id: &str, allow: bool) -> DomainResult<()> {
        let changed = self
            .db
            .lock()
            .execute(
                "UPDATE workspaces SET allow_external = ?2 WHERE id = ?1",
                params![id, allow as i64],
            )
            .map_err(storage_err("update workspace"))?;
        if changed == 0 {
            return Err(DomainError::NotFound(format!("workspace {id}")));
        }
        Ok(())
    }

    fn delete(&self, id: &str) -> DomainResult<()> {
        let conn = self.db.lock();
        // FTS rows are not covered by foreign-key cascades; clean them first.
        conn.execute(
            "DELETE FROM chunks_fts WHERE chunk_id IN
             (SELECT id FROM chunks WHERE workspace_id = ?1)",
            params![id],
        )
        .map_err(storage_err("delete fts rows"))?;
        conn.execute("DELETE FROM workspaces WHERE id = ?1", params![id])
            .map_err(storage_err("delete workspace"))?;
        Ok(())
    }
}
