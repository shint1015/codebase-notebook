use serde::{Deserialize, Serialize};
use tauri::State;

use crate::application::usecases::ask::AskPreparation;
use crate::application::usecases::index::IndexReport;
use crate::domain::entities::chat::{ChatSession, Message};
use crate::domain::entities::chunk::SearchHit;
use crate::domain::entities::provider::{ProviderConfig, ProviderKind};
use crate::domain::entities::repository::Repository;
use crate::domain::entities::workspace::Workspace;
use crate::domain::error::DomainError;

use super::state::AppState;

/// Structured command error so the frontend can branch on `code`
/// (e.g. `consent_required` opens the confirmation dialog).
#[derive(Debug, Serialize)]
pub struct CommandError {
    pub code: String,
    pub message: String,
}

impl From<DomainError> for CommandError {
    fn from(error: DomainError) -> Self {
        let code = match &error {
            DomainError::NotFound(_) => "not_found",
            DomainError::Validation(_) => "validation",
            DomainError::Storage(_) => "storage",
            DomainError::Provider(_) => "provider",
            DomainError::ProviderNotConfigured(_) => "provider_not_configured",
            DomainError::ConsentRequired => "consent_required",
            DomainError::SecretStore(_) => "secret_store",
            DomainError::Indexing(_) => "indexing",
        };
        Self {
            code: code.to_string(),
            message: error.to_string(),
        }
    }
}

type CommandResult<T> = Result<T, CommandError>;

fn parse_kind(provider: &str) -> Result<ProviderKind, CommandError> {
    ProviderKind::parse(provider).ok_or_else(|| CommandError {
        code: "validation".into(),
        message: format!("unknown provider: {provider}"),
    })
}

// ---- workspaces ----

#[tauri::command]
pub fn list_workspaces(state: State<'_, AppState>) -> CommandResult<Vec<Workspace>> {
    Ok(state.workspaces.list()?)
}

#[tauri::command]
pub fn create_workspace(state: State<'_, AppState>, name: String) -> CommandResult<Workspace> {
    Ok(state.workspaces.create(&name)?)
}

#[tauri::command]
pub fn delete_workspace(state: State<'_, AppState>, workspace_id: String) -> CommandResult<()> {
    state.workspaces.delete(&workspace_id)?;
    // Cloned repositories owned by the app go away with their workspace.
    state.repositories.remove_workspace_clones(&workspace_id)?;
    Ok(())
}

// ---- repositories ----

#[tauri::command]
pub fn list_repositories(
    state: State<'_, AppState>,
    workspace_id: String,
) -> CommandResult<Vec<Repository>> {
    Ok(state.repositories.list(&workspace_id)?)
}

#[tauri::command]
pub fn add_local_repository(
    state: State<'_, AppState>,
    workspace_id: String,
    root_path: String,
) -> CommandResult<Repository> {
    Ok(state.repositories.add_local(&workspace_id, &root_path)?)
}

#[tauri::command]
pub async fn add_git_repository(
    state: State<'_, AppState>,
    workspace_id: String,
    url: String,
) -> CommandResult<Repository> {
    Ok(state.repositories.add_from_git(&workspace_id, &url).await?)
}

#[tauri::command]
pub async fn add_github_issues_repository(
    state: State<'_, AppState>,
    workspace_id: String,
    spec: String,
) -> CommandResult<Repository> {
    Ok(state
        .repositories
        .add_github_issues(&workspace_id, &spec)
        .await?)
}

#[tauri::command]
pub fn delete_repository(
    state: State<'_, AppState>,
    repository_id: String,
) -> CommandResult<()> {
    Ok(state.repositories.remove(&repository_id)?)
}

#[tauri::command]
pub async fn sync_repository(
    state: State<'_, AppState>,
    repository_id: String,
) -> CommandResult<Repository> {
    Ok(state.repositories.sync(&repository_id).await?)
}

#[tauri::command]
pub fn rebuild_watchers(
    state: State<'_, AppState>,
    watcher: State<'_, super::state::WatcherHandle>,
) -> CommandResult<()> {
    let targets = state.watch_targets()?;
    Ok(watcher.0.rebuild(targets)?)
}

// ---- publishing ----

#[tauri::command]
pub async fn create_github_issue(
    state: State<'_, AppState>,
    spec: String,
    title: String,
    body: String,
) -> CommandResult<String> {
    Ok(state.publish.create_issue(&spec, &title, &body).await?)
}

#[tauri::command]
pub fn list_wiki_repositories(
    state: State<'_, AppState>,
    workspace_id: String,
) -> CommandResult<Vec<Repository>> {
    Ok(state.publish.wiki_repositories(&workspace_id)?)
}

#[tauri::command]
pub fn write_wiki_page(
    state: State<'_, AppState>,
    repository_id: String,
    title: String,
    content: String,
) -> CommandResult<String> {
    Ok(state.publish.write_wiki_page(&repository_id, &title, &content)?)
}

#[tauri::command]
pub fn set_workspace_allow_external(
    state: State<'_, AppState>,
    workspace_id: String,
    allow: bool,
) -> CommandResult<()> {
    Ok(state.workspaces.set_allow_external(&workspace_id, allow)?)
}

#[derive(Debug, Serialize)]
pub struct WorkspaceStats {
    pub documents: i64,
    pub chunks: i64,
}

#[tauri::command]
pub fn workspace_stats(
    state: State<'_, AppState>,
    workspace_id: String,
) -> CommandResult<WorkspaceStats> {
    Ok(WorkspaceStats {
        documents: state.documents.count_by_workspace(&workspace_id)?,
        chunks: state.documents.count_chunks(&workspace_id)?,
    })
}

// ---- indexing & search ----

#[tauri::command]
pub async fn index_workspace(
    state: State<'_, AppState>,
    workspace_id: String,
) -> CommandResult<IndexReport> {
    Ok(state.index.execute(&workspace_id).await?)
}

#[tauri::command]
pub async fn search_workspace(
    state: State<'_, AppState>,
    workspace_id: String,
    query: String,
    limit: Option<usize>,
) -> CommandResult<Vec<SearchHit>> {
    Ok(state
        .search
        .execute(&workspace_id, &query, limit.unwrap_or(20))
        .await?)
}

// ---- providers ----

#[tauri::command]
pub fn list_providers(state: State<'_, AppState>) -> CommandResult<Vec<ProviderConfig>> {
    Ok(state.providers.list()?)
}

#[derive(Debug, Deserialize)]
pub struct ConfigureProviderInput {
    pub kind: String,
    pub enabled: bool,
    pub base_url: String,
    pub default_model: String,
    pub allow_send_code: bool,
    pub monthly_budget_usd: Option<f64>,
    /// Some("") clears the stored key; None leaves it untouched.
    pub api_key: Option<String>,
}

#[tauri::command]
pub fn configure_provider(
    state: State<'_, AppState>,
    input: ConfigureProviderInput,
) -> CommandResult<ProviderConfig> {
    let kind = parse_kind(&input.kind)?;
    let config = ProviderConfig {
        kind,
        enabled: input.enabled,
        base_url: input.base_url,
        default_model: input.default_model,
        allow_send_code: input.allow_send_code,
        monthly_budget_usd: input.monthly_budget_usd,
        has_api_key: false,
    };
    Ok(state.providers.configure(config, input.api_key)?)
}

#[tauri::command]
pub async fn test_provider(
    state: State<'_, AppState>,
    provider: String,
) -> CommandResult<String> {
    let kind = parse_kind(&provider)?;
    Ok(state.providers.test_connection(kind).await?)
}

// ---- chat ----

#[tauri::command]
pub fn create_chat_session(
    state: State<'_, AppState>,
    workspace_id: String,
    title: String,
) -> CommandResult<ChatSession> {
    Ok(state.chats.create_session(&workspace_id, &title)?)
}

#[tauri::command]
pub fn list_chat_sessions(
    state: State<'_, AppState>,
    workspace_id: String,
) -> CommandResult<Vec<ChatSession>> {
    Ok(state.chats.list_sessions(&workspace_id)?)
}

#[tauri::command]
pub fn list_chat_messages(
    state: State<'_, AppState>,
    session_id: String,
) -> CommandResult<Vec<Message>> {
    Ok(state.chats.list_messages(&session_id)?)
}

#[tauri::command]
pub fn rename_chat_session(
    state: State<'_, AppState>,
    session_id: String,
    title: String,
) -> CommandResult<()> {
    Ok(state.chats.rename_session(&session_id, &title)?)
}

#[tauri::command]
pub fn delete_chat_session(
    state: State<'_, AppState>,
    session_id: String,
) -> CommandResult<()> {
    Ok(state.chats.delete_session(&session_id)?)
}

#[tauri::command]
pub fn export_chat(
    state: State<'_, AppState>,
    session_id: String,
    dest_path: String,
) -> CommandResult<()> {
    let markdown = state.chats.export_markdown(&session_id)?;
    std::fs::write(&dest_path, markdown).map_err(|e| CommandError {
        code: "storage".into(),
        message: format!("write export: {e}"),
    })?;
    Ok(())
}

#[tauri::command]
pub fn fork_chat_session(
    state: State<'_, AppState>,
    session_id: String,
    up_to_message_id: Option<String>,
) -> CommandResult<ChatSession> {
    Ok(state
        .chats
        .fork_session(&session_id, up_to_message_id.as_deref())?)
}

/// Chat transcript as markdown (for "Copy").
#[tauri::command]
pub fn chat_markdown(
    state: State<'_, AppState>,
    session_id: String,
) -> CommandResult<String> {
    Ok(state.chats.export_markdown(&session_id)?)
}

/// Save a chat as an in-app markdown document; returns the note file name.
#[tauri::command]
pub fn chat_to_document(
    state: State<'_, AppState>,
    workspace_id: String,
    session_id: String,
    title: String,
) -> CommandResult<String> {
    let markdown = state.chats.export_markdown(&session_id)?;
    Ok(state.notes.save(&workspace_id, &title, &markdown)?)
}

#[tauri::command]
pub fn reveal_source(
    state: State<'_, AppState>,
    workspace_id: String,
    rel_path: String,
    line: i64,
) -> CommandResult<()> {
    let path = state
        .repositories
        .resolve_source_path(&workspace_id, &rel_path)?;
    Ok(crate::infrastructure::reveal::open_in_editor(&path, line)?)
}

#[tauri::command]
pub async fn prepare_ask(
    state: State<'_, AppState>,
    workspace_id: String,
    question: String,
    provider: String,
    session_id: Option<String>,
) -> CommandResult<AskPreparation> {
    let kind = parse_kind(&provider)?;
    Ok(state
        .ask
        .prepare(&workspace_id, &question, kind, session_id.as_deref())
        .await?)
}

// ---- usage & budgets ----

#[tauri::command]
pub fn list_usage(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> CommandResult<Vec<crate::domain::entities::usage::UsageRecord>> {
    Ok(state.usage.list_recent(limit.unwrap_or(50))?)
}

#[derive(Debug, Serialize)]
pub struct ProviderUsageSummary {
    pub provider: String,
    pub month_total_usd: f64,
    pub monthly_budget_usd: Option<f64>,
}

#[tauri::command]
pub fn usage_summary(state: State<'_, AppState>) -> CommandResult<Vec<ProviderUsageSummary>> {
    let month = chrono::Utc::now().format("%Y-%m").to_string();
    let mut summary = Vec::new();
    for config in state.providers.list()? {
        if !config.kind.is_external() {
            continue;
        }
        summary.push(ProviderUsageSummary {
            provider: config.kind.as_str().to_string(),
            month_total_usd: state.usage.month_total_usd(config.kind.as_str(), &month)?,
            monthly_budget_usd: config.monthly_budget_usd,
        });
    }
    Ok(summary)
}

// ---- ollama onboarding ----

#[derive(Debug, Serialize)]
pub struct OllamaStatus {
    pub reachable: bool,
    pub chat_model: String,
    pub chat_model_present: bool,
    pub embedding_model: String,
    pub embedding_model_present: bool,
}

#[tauri::command]
pub async fn ollama_status(state: State<'_, AppState>) -> CommandResult<OllamaStatus> {
    use crate::infrastructure::providers::ollama::OllamaAdmin;
    let config = state
        .providers
        .list()?
        .into_iter()
        .find(|c| c.kind == ProviderKind::Ollama)
        .unwrap_or_else(|| ProviderConfig::default_for(ProviderKind::Ollama));
    let embedding_model = state
        .settings
        .get("embedding_model")?
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| super::state::EMBEDDING_MODEL.to_string());
    let admin = OllamaAdmin::new(&config.base_url);
    let (reachable, installed) = admin.installed_models().await;
    Ok(OllamaStatus {
        reachable,
        chat_model_present: OllamaAdmin::has_model(&installed, &config.default_model),
        chat_model: config.default_model,
        embedding_model_present: OllamaAdmin::has_model(&installed, &embedding_model),
        embedding_model,
    })
}

#[tauri::command]
pub async fn pull_ollama_model(
    state: State<'_, AppState>,
    model: String,
    on_progress: tauri::ipc::Channel<String>,
) -> CommandResult<()> {
    use crate::infrastructure::providers::ollama::OllamaAdmin;
    let config = state
        .providers
        .list()?
        .into_iter()
        .find(|c| c.kind == ProviderKind::Ollama)
        .unwrap_or_else(|| ProviderConfig::default_for(ProviderKind::Ollama));
    let admin = OllamaAdmin::new(&config.base_url);
    let sink = move |line: &str| {
        on_progress.send(line.to_string()).ok();
    };
    Ok(admin.pull(&model, &sink).await?)
}

// ---- source files (in-app code editor) ----

#[tauri::command]
pub fn read_source_file(
    state: State<'_, AppState>,
    workspace_id: String,
    rel_path: String,
) -> CommandResult<String> {
    Ok(state.repositories.read_source_file(&workspace_id, &rel_path)?)
}

#[tauri::command]
pub fn write_source_file(
    state: State<'_, AppState>,
    workspace_id: String,
    rel_path: String,
    content: String,
) -> CommandResult<()> {
    Ok(state
        .repositories
        .write_source_file(&workspace_id, &rel_path, &content)?)
}

// ---- notes / in-app documents ----

#[tauri::command]
pub fn list_notes(
    state: State<'_, AppState>,
    workspace_id: String,
) -> CommandResult<Vec<crate::application::usecases::notes::NoteFile>> {
    Ok(state.notes.list(&workspace_id)?)
}

#[tauri::command]
pub fn read_note(
    state: State<'_, AppState>,
    workspace_id: String,
    name: String,
) -> CommandResult<String> {
    Ok(state.notes.read(&workspace_id, &name)?)
}

#[tauri::command]
pub fn save_note(
    state: State<'_, AppState>,
    workspace_id: String,
    name: String,
    content: String,
) -> CommandResult<String> {
    Ok(state.notes.save(&workspace_id, &name, &content)?)
}

#[tauri::command]
pub fn delete_note(
    state: State<'_, AppState>,
    workspace_id: String,
    name: String,
) -> CommandResult<()> {
    Ok(state.notes.delete(&workspace_id, &name)?)
}

// ---- connectors ----

#[derive(Debug, Serialize)]
pub struct ConnectorStatus {
    pub name: String,
    pub connected: bool,
}

#[tauri::command]
pub fn list_connectors(state: State<'_, AppState>) -> CommandResult<Vec<ConnectorStatus>> {
    use crate::infrastructure::connectors::{secret_key, CONNECTOR_KINDS};
    let mut out = Vec::new();
    for name in CONNECTOR_KINDS {
        let connected = state.secrets.get_secret(&secret_key(name))?.is_some();
        out.push(ConnectorStatus {
            name: (*name).to_string(),
            connected,
        });
    }
    Ok(out)
}

#[tauri::command]
pub fn set_connector_token(
    state: State<'_, AppState>,
    connector: String,
    token: String,
) -> CommandResult<()> {
    use crate::infrastructure::connectors::{secret_key, CONNECTOR_KINDS};
    if !CONNECTOR_KINDS.contains(&connector.as_str()) {
        return Err(CommandError {
            code: "validation".into(),
            message: format!("unknown connector: {connector}"),
        });
    }
    let key = secret_key(&connector);
    if token.trim().is_empty() {
        state.secrets.delete_secret(&key)?;
    } else {
        state.secrets.set_secret(&key, token.trim())?;
    }
    Ok(())
}

// ---- agent ----

#[tauri::command]
pub async fn agent_ask(
    state: State<'_, AppState>,
    session_id: String,
    workspace_id: String,
    question: String,
    provider: String,
    allow_writes: bool,
) -> CommandResult<crate::application::usecases::agent::AgentOutcome> {
    let kind = parse_kind(&provider)?;
    Ok(state
        .agent
        .run(&session_id, &workspace_id, &question, kind, allow_writes)
        .await?)
}

// ---- search settings ----

#[derive(Debug, Serialize)]
pub struct SearchSettings {
    pub embedding_model: String,
    pub rerank_enabled: bool,
}

#[tauri::command]
pub fn get_search_settings(state: State<'_, AppState>) -> CommandResult<SearchSettings> {
    Ok(SearchSettings {
        embedding_model: state
            .settings
            .get("embedding_model")?
            .unwrap_or_else(|| super::state::EMBEDDING_MODEL.to_string()),
        rerank_enabled: state
            .settings
            .get("rerank_enabled")?
            .map(|v| v == "true")
            .unwrap_or(false),
    })
}

#[tauri::command]
pub fn set_search_settings(
    state: State<'_, AppState>,
    embedding_model: String,
    rerank_enabled: bool,
) -> CommandResult<()> {
    state.settings.set("embedding_model", embedding_model.trim())?;
    state
        .settings
        .set("rerank_enabled", if rerank_enabled { "true" } else { "false" })?;
    Ok(())
}

#[tauri::command]
pub async fn ask(
    state: State<'_, AppState>,
    session_id: String,
    workspace_id: String,
    question: String,
    provider: String,
    consent_granted: bool,
    on_token: tauri::ipc::Channel<String>,
) -> CommandResult<Message> {
    let kind = parse_kind(&provider)?;
    let sink = move |token: &str| {
        on_token.send(token.to_string()).ok();
    };
    Ok(state
        .ask
        .execute_stream(
            &session_id,
            &workspace_id,
            &question,
            kind,
            consent_granted,
            &sink,
        )
        .await?)
}
