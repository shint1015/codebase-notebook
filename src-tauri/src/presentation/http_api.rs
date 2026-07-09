//! Local HTTP API for editor integrations (VS Code extension, CLI).
//!
//! Security model:
//! - Binds to 127.0.0.1 only — never reachable from the network.
//! - Every request needs a bearer token, generated per install and written to
//!   `<app data>/api-token` (only local processes of the same user can read it).
//! - Answers use the LOCAL provider exclusively: the API can never trigger an
//!   external send, so the in-app consent flow cannot be bypassed.

use std::path::Path;

use serde::Deserialize;
use serde_json::json;
use tauri::Manager;

use crate::domain::entities::provider::ProviderKind;
use crate::presentation::state::AppState;

pub const API_PORT: u16 = 43110;

#[derive(Deserialize)]
struct AskRequest {
    workspace_id: String,
    question: String,
    #[serde(default)]
    session_id: Option<String>,
}

#[derive(Deserialize)]
struct SearchRequest {
    workspace_id: String,
    query: String,
    #[serde(default)]
    limit: Option<usize>,
}

/// Load (or create) the API token under the app data dir.
pub fn ensure_token(data_dir: &Path) -> std::io::Result<String> {
    let path = data_dir.join("api-token");
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let existing = existing.trim().to_string();
        if !existing.is_empty() {
            return Ok(existing);
        }
    }
    let token = format!("cbnb-{}{}", uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
    std::fs::create_dir_all(data_dir)?;
    std::fs::write(&path, &token)?;
    Ok(token)
}

/// Start the API server on a background thread.
pub fn start(handle: tauri::AppHandle, token: String) {
    std::thread::spawn(move || {
        let server = match tiny_http::Server::http(("127.0.0.1", API_PORT)) {
            Ok(server) => server,
            Err(error) => {
                eprintln!("local api: failed to bind 127.0.0.1:{API_PORT}: {error}");
                return;
            }
        };
        for request in server.incoming_requests() {
            let handle = handle.clone();
            let token = token.clone();
            // One request at a time is fine for a single-user editor bridge.
            respond(handle, &token, request);
        }
    });
}

fn respond(handle: tauri::AppHandle, token: &str, mut request: tiny_http::Request) {
    let url = request.url().to_string();
    let method = request.method().to_string();

    if url == "/health" {
        reply_json(request, 200, json!({"ok": true}));
        return;
    }

    let authorized = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("Authorization"))
        .map(|h| h.value.as_str() == format!("Bearer {token}"))
        .unwrap_or(false);
    if !authorized {
        reply_json(request, 401, json!({"error": "missing or invalid token"}));
        return;
    }

    let mut body = String::new();
    request.as_reader().read_to_string(&mut body).ok();
    let state = handle.state::<AppState>();

    let result: Result<serde_json::Value, (u16, String)> = match (method.as_str(), url.as_str()) {
        ("GET", "/api/workspaces") => state
            .workspaces
            .list()
            .map(|list| serde_json::to_value(list).unwrap_or_default())
            .map_err(internal),
        ("POST", "/api/search") => match serde_json::from_str::<SearchRequest>(&body) {
            Ok(input) => tauri::async_runtime::block_on(state.search.execute(
                &input.workspace_id,
                &input.query,
                input.limit.unwrap_or(10),
            ))
            .map(|hits| serde_json::to_value(hits).unwrap_or_default())
            .map_err(internal),
            Err(error) => Err((400, format!("bad request: {error}"))),
        },
        ("POST", "/api/ask") => match serde_json::from_str::<AskRequest>(&body) {
            Ok(input) => tauri::async_runtime::block_on(async {
                let session_id = match &input.session_id {
                    Some(session_id) => session_id.clone(),
                    None => {
                        state
                            .chats
                            .create_session(&input.workspace_id, &input.question)?
                            .id
                    }
                };
                let message = state
                    .ask
                    .execute(
                        &session_id,
                        &input.workspace_id,
                        &input.question,
                        ProviderKind::Ollama,
                        false,
                    )
                    .await?;
                Ok(json!({"session_id": session_id, "message": message}))
            })
            .map_err(internal),
            Err(error) => Err((400, format!("bad request: {error}"))),
        },
        _ => Err((404, "not found".to_string())),
    };

    match result {
        Ok(value) => reply_json(request, 200, value),
        Err((status, message)) => reply_json(request, status, json!({"error": message})),
    }
}

fn internal(error: crate::domain::error::DomainError) -> (u16, String) {
    (500, error.to_string())
}

fn reply_json(request: tiny_http::Request, status: u16, value: serde_json::Value) {
    let payload = value.to_string();
    let response = tiny_http::Response::from_string(payload)
        .with_status_code(status)
        .with_header(
            tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                .expect("static header"),
        );
    request.respond(response).ok();
}
