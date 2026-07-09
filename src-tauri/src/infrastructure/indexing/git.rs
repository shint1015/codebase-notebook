use std::process::Command;

use crate::domain::error::{DomainError, DomainResult};
use crate::domain::services::{GitSync, RepoCloner};

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

fn run_git(repo_path: &str, args: &[&str]) -> DomainResult<std::process::Output> {
    Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(args)
        .output()
        .map_err(|e| DomainError::Indexing(format!("run git: {e}")))
}

impl GitSync for GitCliCloner {
    fn pull(&self, repo_path: &str) -> DomainResult<()> {
        let output = run_git(repo_path, &["pull", "--rebase"])?;
        if !output.status.success() {
            return Err(DomainError::Indexing(format!(
                "git pull failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(())
    }

    fn commit_and_push(&self, repo_path: &str, message: &str) -> DomainResult<()> {
        let add = run_git(repo_path, &["add", "-A"])?;
        if !add.status.success() {
            return Err(DomainError::Indexing(format!(
                "git add failed: {}",
                String::from_utf8_lossy(&add.stderr).trim()
            )));
        }
        let commit = run_git(repo_path, &["commit", "-m", message])?;
        if !commit.status.success() {
            let stderr = String::from_utf8_lossy(&commit.stderr);
            let stdout = String::from_utf8_lossy(&commit.stdout);
            if stdout.contains("nothing to commit") || stderr.contains("nothing to commit") {
                return Err(DomainError::Validation(
                    "no changes to publish — the page content is unchanged".into(),
                ));
            }
            return Err(DomainError::Indexing(format!(
                "git commit failed: {}",
                stderr.trim()
            )));
        }
        let push = run_git(repo_path, &["push"])?;
        if !push.status.success() {
            return Err(DomainError::Indexing(format!(
                "git push failed: {}",
                String::from_utf8_lossy(&push.stderr).trim()
            )));
        }
        Ok(())
    }
}
