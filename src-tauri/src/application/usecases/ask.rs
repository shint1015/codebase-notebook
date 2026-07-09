use std::sync::Arc;

use serde::Serialize;

use crate::application::usecases::search::SearchUseCase;
use crate::domain::entities::chat::{Citation, Message, Role};
use crate::domain::entities::chunk::SearchHit;
use crate::domain::entities::provider::ProviderKind;
use crate::domain::error::{DomainError, DomainResult};
use crate::domain::entities::repository::Repository;
use crate::domain::repositories::{
    ChatRepository, DocumentRepository, ProviderConfigRepository, RepositoryRepository,
    WorkspaceRepository,
};
use crate::domain::services::{ChatTurn, ProviderRouter, TokenSink};

const RETRIEVE_LIMIT: usize = 8;
/// Recent turns replayed to the model for conversational context.
const HISTORY_TURNS: usize = 6;

/// What would be sent where — shown to the user before any external call.
#[derive(Debug, Serialize)]
pub struct AskPreparation {
    pub provider: ProviderKind,
    pub model: String,
    pub is_external: bool,
    /// True when the user must explicitly approve this request.
    pub requires_consent: bool,
    /// The exact sources that would be included in the prompt.
    pub sources: Vec<SourcePreview>,
}

#[derive(Debug, Serialize)]
pub struct SourcePreview {
    pub rel_path: String,
    pub start_line: i64,
    pub end_line: i64,
}

pub struct AskUseCase {
    workspaces: Arc<dyn WorkspaceRepository>,
    repositories: Arc<dyn RepositoryRepository>,
    documents: Arc<dyn DocumentRepository>,
    chats: Arc<dyn ChatRepository>,
    providers: Arc<dyn ProviderConfigRepository>,
    router: Arc<dyn ProviderRouter>,
    search: Arc<SearchUseCase>,
}

impl AskUseCase {
    pub fn new(
        workspaces: Arc<dyn WorkspaceRepository>,
        repositories: Arc<dyn RepositoryRepository>,
        documents: Arc<dyn DocumentRepository>,
        chats: Arc<dyn ChatRepository>,
        providers: Arc<dyn ProviderConfigRepository>,
        router: Arc<dyn ProviderRouter>,
        search: Arc<SearchUseCase>,
    ) -> Self {
        Self {
            workspaces,
            repositories,
            documents,
            chats,
            providers,
            router,
            search,
        }
    }

    /// A question can only be grounded if something is indexed. Guides the
    /// user to run indexing instead of letting the model answer "the sources
    /// do not cover this" for every question.
    fn ensure_indexed(&self, workspace_id: &str) -> DomainResult<()> {
        if self.documents.count_chunks(workspace_id)? == 0 {
            return Err(DomainError::Validation(
                "this workspace has no indexed sources yet — add a repository and run \
                 \"Index all repositories\" first"
                    .into(),
            ));
        }
        Ok(())
    }

    /// Dry-run: retrieve sources and report whether user consent is needed
    /// before anything leaves the machine.
    pub async fn prepare(
        &self,
        workspace_id: &str,
        question: &str,
        provider: ProviderKind,
    ) -> DomainResult<AskPreparation> {
        let workspace = self.workspaces.find_by_id(workspace_id)?;
        self.ensure_indexed(workspace_id)?;
        let config = self.resolve_config(provider)?;
        let hits = self
            .search
            .execute(workspace_id, question, RETRIEVE_LIMIT)
            .await?;
        let is_external = provider.is_external();
        Ok(AskPreparation {
            provider,
            model: config.default_model,
            is_external,
            requires_consent: is_external && !workspace.allow_external,
            sources: hits
                .iter()
                .map(|h| SourcePreview {
                    rel_path: h.rel_path.clone(),
                    start_line: h.chunk.start_line,
                    end_line: h.chunk.end_line,
                })
                .collect(),
        })
    }

    /// Answer a question grounded in workspace sources, persisting both the
    /// user message and the cited assistant reply.
    pub async fn execute(
        &self,
        session_id: &str,
        workspace_id: &str,
        question: &str,
        provider: ProviderKind,
        consent_granted: bool,
    ) -> DomainResult<Message> {
        self.execute_stream(
            session_id,
            workspace_id,
            question,
            provider,
            consent_granted,
            &|_| {},
        )
        .await
    }

    /// Streaming variant of [`execute`]: forwards answer fragments to
    /// `on_token` while the model generates.
    pub async fn execute_stream(
        &self,
        session_id: &str,
        workspace_id: &str,
        question: &str,
        provider: ProviderKind,
        consent_granted: bool,
        on_token: &TokenSink,
    ) -> DomainResult<Message> {
        let workspace = self.workspaces.find_by_id(workspace_id)?;
        self.ensure_indexed(workspace_id)?;
        let config = self.resolve_config(provider)?;

        // Safety gate: nothing leaves the machine without explicit consent.
        if provider.is_external() {
            if !config.allow_send_code {
                return Err(DomainError::Validation(format!(
                    "provider {} is not allowed to receive code (allow_send_code is off)",
                    provider.as_str()
                )));
            }
            if !workspace.allow_external && !consent_granted {
                return Err(DomainError::ConsentRequired);
            }
        }

        let hits = self
            .search
            .execute(workspace_id, question, RETRIEVE_LIMIT)
            .await?;

        let history = self.recent_history(session_id)?;

        let user_message = Message {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            role: Role::User,
            content: question.to_string(),
            citations: Vec::new(),
            provider: None,
            model: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        self.chats.append_message(&user_message)?;

        let repositories = self.repositories.list_by_workspace(workspace_id)?;
        let system = build_system_prompt(&workspace.name, &repositories, &hits);
        let mut turns = history;
        turns.push(ChatTurn {
            role: "user".to_string(),
            content: question.to_string(),
        });

        let llm = self.router.resolve(provider)?;
        let answer = llm
            .chat_stream(&config.default_model, &system, &turns, on_token)
            .await?;
        let citations = extract_citations(&answer, &hits);

        let assistant_message = Message {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            role: Role::Assistant,
            content: answer,
            citations,
            provider: Some(provider.as_str().to_string()),
            model: Some(config.default_model),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        self.chats.append_message(&assistant_message)?;
        Ok(assistant_message)
    }

    fn resolve_config(
        &self,
        provider: ProviderKind,
    ) -> DomainResult<crate::domain::entities::provider::ProviderConfig> {
        let config = self
            .providers
            .find(provider)?
            .unwrap_or_else(|| crate::domain::entities::provider::ProviderConfig::default_for(provider));
        if !config.enabled {
            return Err(DomainError::ProviderNotConfigured(format!(
                "{} is not enabled",
                provider.as_str()
            )));
        }
        if config.default_model.trim().is_empty() {
            return Err(DomainError::ProviderNotConfigured(format!(
                "{} has no default model",
                provider.as_str()
            )));
        }
        Ok(config)
    }

    fn recent_history(&self, session_id: &str) -> DomainResult<Vec<ChatTurn>> {
        let messages = self.chats.list_messages(session_id)?;
        Ok(messages
            .iter()
            .rev()
            .take(HISTORY_TURNS)
            .rev()
            .map(|m| ChatTurn {
                role: match m.role {
                    Role::User => "user".to_string(),
                    Role::Assistant => "assistant".to_string(),
                },
                content: m.content.clone(),
            })
            .collect())
    }
}

/// Source-grounded system prompt: workspace overview, numbered sources,
/// mandatory citations, and an explicit instruction not to answer beyond the
/// sources (NotebookLM-style).
fn build_system_prompt(
    workspace_name: &str,
    repositories: &[Repository],
    hits: &[SearchHit],
) -> String {
    let mut prompt = String::from(
        "You are Codebase Notebook, an engineering assistant grounded in the user's \
         indexed sources.\n\
         Rules:\n\
         1. Answer ONLY from the workspace overview and the numbered sources below. Never \
         invent facts, APIs or code that are not in them.\n\
         2. Cite sources inline with their bracket number, e.g. [1] or [2][3], every time \
         you rely on one.\n\
         3. If the sources do not contain the answer, say clearly that the indexed sources \
         do not cover it — do not guess.\n\
         4. Answer in the same language as the user's question.\n\n",
    );
    // Overview lets the model answer meta questions ("what repositories are
    // in this workspace?") that chunk retrieval alone cannot ground.
    prompt.push_str(&format!("Workspace: {workspace_name}\nRepositories:\n"));
    for repository in repositories {
        match &repository.remote_url {
            Some(url) => prompt.push_str(&format!("- {} (cloned from {url})\n", repository.name)),
            None => prompt.push_str(&format!("- {} (local folder)\n", repository.name)),
        }
    }
    prompt.push('\n');
    if hits.is_empty() {
        prompt.push_str("No sources were retrieved for this question.\n");
        return prompt;
    }
    prompt.push_str("Sources:\n");
    for (i, hit) in hits.iter().enumerate() {
        prompt.push_str(&format!(
            "[{}] {} (lines {}-{})\n```\n{}\n```\n\n",
            i + 1,
            hit.rel_path,
            hit.chunk.start_line,
            hit.chunk.end_line,
            hit.chunk.content
        ));
    }
    prompt
}

/// Map bracket markers like [1] in the answer back to the retrieved chunks.
fn extract_citations(answer: &str, hits: &[SearchHit]) -> Vec<Citation> {
    let mut seen = std::collections::BTreeSet::new();
    let bytes = answer.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            if let Some(close) = answer[i + 1..].find(']') {
                let inner = &answer[i + 1..i + 1 + close];
                if !inner.is_empty() && inner.chars().all(|c| c.is_ascii_digit()) {
                    if let Ok(n) = inner.parse::<usize>() {
                        if n >= 1 && n <= hits.len() {
                            seen.insert(n);
                        }
                    }
                }
                i += close + 2;
                continue;
            }
        }
        i += 1;
    }
    seen.into_iter()
        .map(|n| {
            let hit = &hits[n - 1];
            let snippet: String = hit.chunk.content.chars().take(200).collect();
            Citation {
                marker: n as i64,
                chunk_id: hit.chunk.id.clone(),
                rel_path: hit.rel_path.clone(),
                start_line: hit.chunk.start_line,
                end_line: hit.chunk.end_line,
                snippet,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::entities::chunk::Chunk;

    fn hit(id: &str, path: &str) -> SearchHit {
        SearchHit {
            chunk: Chunk {
                id: id.to_string(),
                document_id: "d".into(),
                workspace_id: "w".into(),
                seq: 0,
                content: "fn main() {}".into(),
                start_line: 1,
                end_line: 1,
            },
            rel_path: path.to_string(),
            score: 1.0,
        }
    }

    #[test]
    fn extracts_valid_markers_once() {
        let hits = vec![hit("a", "src/a.rs"), hit("b", "src/b.rs")];
        let citations = extract_citations("See [1] and [2], also [1] again. [9] is invalid.", &hits);
        assert_eq!(citations.len(), 2);
        assert_eq!(citations[0].marker, 1);
        assert_eq!(citations[0].rel_path, "src/a.rs");
        assert_eq!(citations[1].marker, 2);
    }

    #[test]
    fn ignores_non_numeric_brackets() {
        let hits = vec![hit("a", "src/a.rs")];
        assert!(extract_citations("array[i] and [foo] are not citations", &hits).is_empty());
    }

    #[test]
    fn system_prompt_numbers_sources_and_lists_repositories() {
        let hits = vec![hit("a", "src/a.rs")];
        let repositories = vec![Repository {
            id: "r1".into(),
            workspace_id: "w".into(),
            name: "backend".into(),
            root_path: "/tmp/backend".into(),
            remote_url: Some("https://github.com/org/backend.git".into()),
            source_kind: crate::domain::entities::repository::SourceKind::Git,
            created_at: "2026-01-01T00:00:00Z".into(),
        }];
        let prompt = build_system_prompt("demo", &repositories, &hits);
        assert!(prompt.contains("Workspace: demo"));
        assert!(prompt.contains("- backend (cloned from https://github.com/org/backend.git)"));
        assert!(prompt.contains("[1] src/a.rs"));
        assert!(prompt.contains("fn main() {}"));
    }
}
