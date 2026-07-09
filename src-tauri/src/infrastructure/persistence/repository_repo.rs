use rusqlite::params;

use super::{storage_err, Db};
use crate::domain::entities::repository::Repository;
use crate::domain::error::{DomainError, DomainResult};
use crate::domain::repositories::RepositoryRepository;

pub struct SqliteRepositoryRepository {
    db: Db,
}

impl SqliteRepositoryRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

fn row_to_repository(row: &rusqlite::Row<'_>) -> rusqlite::Result<Repository> {
    Ok(Repository {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        name: row.get(2)?,
        root_path: row.get(3)?,
        remote_url: row.get(4)?,
        created_at: row.get(5)?,
    })
}

impl RepositoryRepository for SqliteRepositoryRepository {
    fn create(&self, repository: &Repository) -> DomainResult<()> {
        self.db
            .lock()
            .execute(
                "INSERT INTO repositories (id, workspace_id, name, root_path, remote_url, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    repository.id,
                    repository.workspace_id,
                    repository.name,
                    repository.root_path,
                    repository.remote_url,
                    repository.created_at,
                ],
            )
            .map_err(|e| match e {
                rusqlite::Error::SqliteFailure(err, _)
                    if err.code == rusqlite::ErrorCode::ConstraintViolation =>
                {
                    DomainError::Validation(format!(
                        "a repository named \"{}\" already exists in this workspace",
                        repository.name
                    ))
                }
                other => DomainError::Storage(format!("insert repository: {other}")),
            })?;
        Ok(())
    }

    fn find_by_id(&self, id: &str) -> DomainResult<Repository> {
        self.db
            .lock()
            .query_row(
                "SELECT id, workspace_id, name, root_path, remote_url, created_at
                 FROM repositories WHERE id = ?1",
                params![id],
                row_to_repository,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    DomainError::NotFound(format!("repository {id}"))
                }
                other => DomainError::Storage(format!("find repository: {other}")),
            })
    }

    fn list_by_workspace(&self, workspace_id: &str) -> DomainResult<Vec<Repository>> {
        let conn = self.db.lock();
        let mut stmt = conn
            .prepare(
                "SELECT id, workspace_id, name, root_path, remote_url, created_at
                 FROM repositories WHERE workspace_id = ?1 ORDER BY created_at",
            )
            .map_err(storage_err("prepare list repositories"))?;
        let rows = stmt
            .query_map(params![workspace_id], row_to_repository)
            .map_err(storage_err("list repositories"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(storage_err("read repositories"))
    }

    fn delete(&self, id: &str) -> DomainResult<()> {
        self.db
            .lock()
            .execute("DELETE FROM repositories WHERE id = ?1", params![id])
            .map_err(storage_err("delete repository"))?;
        Ok(())
    }
}
