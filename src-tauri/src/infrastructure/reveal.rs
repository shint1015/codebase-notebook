//! Open a source file at a line in the user's editor. Prefers VS Code
//! (`code -g file:line`), falls back to revealing the file in the OS file
//! manager.

use crate::domain::error::{DomainError, DomainResult};

pub fn open_in_editor(path: &str, line: i64) -> DomainResult<()> {
    if !std::path::Path::new(path).exists() {
        return Err(DomainError::NotFound(format!("file not found: {path}")));
    }
    let vscode = std::process::Command::new("code")
        .arg("-g")
        .arg(format!("{path}:{line}"))
        .status();
    if matches!(vscode, Ok(status) if status.success()) {
        return Ok(());
    }
    let fallback = if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg("-R").arg(path).status()
    } else if cfg!(target_os = "windows") {
        std::process::Command::new("explorer")
            .arg(format!("/select,{path}"))
            .status()
    } else {
        std::process::Command::new("xdg-open").arg(path).status()
    };
    match fallback {
        Ok(status) if status.success() => Ok(()),
        _ => Err(DomainError::Indexing(
            "could not open the file — install the VS Code `code` CLI for best results".into(),
        )),
    }
}
