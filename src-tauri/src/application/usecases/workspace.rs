use std::sync::Arc;

use crate::domain::entities::workspace::Workspace;
use crate::domain::error::{DomainError, DomainResult};
use crate::domain::repositories::WorkspaceRepository;

pub struct WorkspaceUseCases {
    repo: Arc<dyn WorkspaceRepository>,
}

impl WorkspaceUseCases {
    pub fn new(repo: Arc<dyn WorkspaceRepository>) -> Self {
        Self { repo }
    }

    pub fn create(&self, name: &str) -> DomainResult<Workspace> {
        let name = name.trim();
        if name.is_empty() {
            return Err(DomainError::Validation("workspace name is empty".into()));
        }
        let workspace = Workspace {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            allow_external: false,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        self.repo.create(&workspace)?;
        Ok(workspace)
    }

    pub fn list(&self) -> DomainResult<Vec<Workspace>> {
        self.repo.list()
    }

    pub fn get(&self, id: &str) -> DomainResult<Workspace> {
        self.repo.find_by_id(id)
    }

    pub fn set_allow_external(&self, id: &str, allow: bool) -> DomainResult<()> {
        self.repo.set_allow_external(id, allow)
    }

    pub fn delete(&self, id: &str) -> DomainResult<()> {
        self.repo.delete(id)
    }
}
