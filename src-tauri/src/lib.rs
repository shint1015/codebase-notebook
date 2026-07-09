pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod presentation;

use tauri::Manager;

use presentation::commands;
use presentation::state::AppState;

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
            commands::delete_repository,
            commands::workspace_stats,
            commands::index_workspace,
            commands::search_workspace,
            commands::list_providers,
            commands::configure_provider,
            commands::test_provider,
            commands::create_chat_session,
            commands::list_chat_sessions,
            commands::list_chat_messages,
            commands::prepare_ask,
            commands::ask,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
