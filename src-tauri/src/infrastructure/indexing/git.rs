use std::process::Command;

use crate::domain::error::{DomainError, DomainResult};
use crate::domain::services::RepoCloner;

/// Clones repositories by shelling out to the user's own `git`, so existing
/// credentials (SSH keys, credential helpers) work unchanged and nothing
/// custom ever handles authentication.
pub struct GitCliCloner;

impl RepoCloner for GitCliCloner {
    fn clone_repo(&self, url: &str, dest: &str) -> DomainResult<()> {
        let output = Command::new("git")
            .args(["clone", "--", url, dest])
            .output()
            .map_err(|e| {
                DomainError::Indexing(format!("failed to run git (is it installed?): {e}"))
            })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // A failed clone may leave a partial directory behind.
            std::fs::remove_dir_all(dest).ok();
            return Err(DomainError::Indexing(format!(
                "git clone failed: {}",
                stderr.trim()
            )));
        }
        Ok(())
    }

    fn remove_clone(&self, path: &str) -> DomainResult<()> {
        if std::path::Path::new(path).exists() {
            std::fs::remove_dir_all(path)
                .map_err(|e| DomainError::Indexing(format!("remove clone: {e}")))?;
        }
        Ok(())
    }
}
