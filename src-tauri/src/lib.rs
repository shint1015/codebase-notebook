pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod presentation;

use std::sync::Arc;

use tauri::{Emitter, Manager};

use infrastructure::indexing::watch::SourceWatcher;
use presentation::commands;
use presentation::state::{AppState, WatcherHandle};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir().expect("app data dir");
            let db_path = data_dir.join("codebase-notebook.sqlite");
            let clones_dir = data_dir.join("repos");
            let state = AppState::new(&db_path, clones_dir)
                .map_err(|e| format!("failed to initialize app state: {e}"))?;
            app.manage(state);

            // File watcher: a quiet period after local source changes
            // triggers a background re-index of the owning workspace.
            let handle = app.handle().clone();
            let watcher = SourceWatcher::new(Arc::new(move |workspace_id: String| {
                let handle = handle.clone();
                tauri::async_runtime::spawn(async move {
                    let state = handle.state::<AppState>();
                    if let Ok(report) = state.index.execute(&workspace_id).await {
                        handle
                            .emit(
                                "workspace-indexed",
                                serde_json::json!({
                                    "workspaceId": workspace_id,
                                    "report": report,
                                }),
                            )
                            .ok();
                    }
                });
            }));
            if let Ok(targets) = app.state::<AppState>().watch_targets() {
                watcher.rebuild(targets).ok();
            }
            app.manage(WatcherHandle(watcher));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_workspaces,
            commands::create_workspace,
            commands::delete_workspace,
            commands::set_workspace_allow_external,
            commands::list_repositories,
            commands::add_local_repository,
            commands::add_git_repository,
            commands::add_github_issues_repository,
            commands::delete_repository,
            commands::sync_repository,
            commands::rebuild_watchers,
            commands::create_github_issue,
            commands::list_wiki_repositories,
            commands::write_wiki_page,
            commands::workspace_stats,
            commands::index_workspace,
            commands::search_workspace,
            commands::list_providers,
            commands::configure_provider,
            commands::test_provider,
            commands::create_chat_session,
            commands::list_chat_sessions,
            commands::list_chat_messages,
            commands::rename_chat_session,
            commands::delete_chat_session,
            commands::export_chat,
            commands::reveal_source,
            commands::prepare_ask,
            commands::ask,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
