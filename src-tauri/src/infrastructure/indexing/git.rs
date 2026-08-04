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

impl crate::domain::services::GitHistory for GitCliCloner {
    fn recent_history(&self, repo_path: &str, limit: usize) -> Option<(String, String)> {
        let head = run_git(repo_path, &["rev-parse", "HEAD"]).ok()?;
        if !head.status.success() {
            return None;
        }
        let head = String::from_utf8_lossy(&head.stdout).trim().to_string();
        let count = format!("-{limit}");
        let log = run_git(
            repo_path,
            &[
                "log",
                &count,
                "--date=short",
                "--pretty=format:## %h %ad — %an%n%n%s%n",
                "--name-only",
            ],
        )
        .ok()?;
        if !log.status.success() {
            return None;
        }
        let body = String::from_utf8_lossy(&log.stdout);
        let markdown = format!(
            "# Recent commits\n\nAuto-generated from git history at index time. Newest first.\n\n{}\n",
            body.trim()
        );
        Some((head, markdown))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::GitHistory;

    #[test]
    fn non_git_directory_has_no_history() {
        let dir = std::env::temp_dir().join(format!("cbnb-no-git-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(GitCliCloner
            .recent_history(dir.to_str().unwrap(), 10)
            .is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn git_repo_history_lists_commits() {
        let dir = std::env::temp_dir().join(format!("cbnb-git-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sh = |args: &[&str]| {
            assert!(Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(args)
                .env("GIT_AUTHOR_NAME", "Test")
                .env("GIT_AUTHOR_EMAIL", "t@example.com")
                .env("GIT_COMMITTER_NAME", "Test")
                .env("GIT_COMMITTER_EMAIL", "t@example.com")
                .output()
                .unwrap()
                .status
                .success());
        };
        sh(&["init", "-q"]);
        std::fs::write(dir.join("a.txt"), "hello").unwrap();
        sh(&["add", "."]);
        sh(&["commit", "-q", "-m", "Add greeting file"]);
        let (head, markdown) = GitCliCloner
            .recent_history(dir.to_str().unwrap(), 10)
            .expect("history");
        assert_eq!(head.len(), 40);
        assert!(markdown.contains("Add greeting file"));
        assert!(markdown.contains("a.txt"));
        assert!(markdown.starts_with("# Recent commits"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
