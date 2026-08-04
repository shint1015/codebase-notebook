use std::sync::Arc;

use serde::Serialize;

use crate::application::usecases::search::SearchUseCase;
use crate::domain::entities::chat::{Citation, Grounding, Message, Role};
use crate::domain::entities::chunk::SearchHit;
use crate::domain::entities::provider::ProviderKind;
use crate::domain::error::{DomainError, DomainResult};
use crate::domain::entities::provider::estimate_cost_usd;
use crate::domain::entities::repository::Repository;
use crate::domain::entities::usage::UsageRecord;
use crate::domain::repositories::{
    ChatRepository, DocumentRepository, ProviderConfigRepository, RepositoryRepository,
    UsageRepository, WorkspaceRepository,
};
use crate::domain::services::{ChatTurn, ProviderRouter, TokenSink};

const RETRIEVE_LIMIT: usize = 8;
/// Chunks pulled in per `@file` mention. Mentions are explicit user intent, so
/// they get their own budget on top of search results.
const MENTION_CHUNK_LIMIT: usize = 6;
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

/// Result of the local-model verification pass over an assistant answer.
#[derive(Debug, Serialize)]
pub struct VerificationReport {
    pub supported: bool,
    pub issues: Vec<String>,
    pub model: String,
}

pub struct AskUseCase {
    workspaces: Arc<dyn WorkspaceRepository>,
    repositories: Arc<dyn RepositoryRepository>,
    documents: Arc<dyn DocumentRepository>,
    chats: Arc<dyn ChatRepository>,
    providers: Arc<dyn ProviderConfigRepository>,
    usage: Arc<dyn UsageRepository>,
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
        usage: Arc<dyn UsageRepository>,
        router: Arc<dyn ProviderRouter>,
        search: Arc<SearchUseCase>,
    ) -> Self {
        Self {
            workspaces,
            repositories,
            documents,
            chats,
            providers,
            usage,
            router,
            search,
        }
    }

    /// Hard stop when this month's estimated spend already exceeds the
    /// provider's budget.
    fn ensure_within_budget(
        &self,
        provider: ProviderKind,
        budget: Option<f64>,
    ) -> DomainResult<()> {
        let Some(budget) = budget else { return Ok(()) };
        let month: String = chrono::Utc::now().format("%Y-%m").to_string();
        let spent = self.usage.month_total_usd(provider.as_str(), &month)?;
        if spent >= budget {
            return Err(DomainError::Validation(format!(
                "monthly budget for {} reached (${spent:.2} of ${budget:.2}) — raise the \
                 budget in settings or use the local model",
                provider.as_str()
            )));
        }
        Ok(())
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

    /// Retrieve sources for a question: chunks of every `@mentioned` file
    /// (explicit user intent, always included) followed by search results,
    /// de-duplicated. When `provider` is external, chunks from sources
    /// classified confidential/secret are physically excluded so they can
    /// never appear in an external prompt.
    async fn collect_hits(
        &self,
        workspace_id: &str,
        question: &str,
        query: &str,
        provider: ProviderKind,
    ) -> DomainResult<Vec<SearchHit>> {
        let mut hits: Vec<SearchHit> = Vec::new();
        for path in mentioned_paths(question) {
            hits.extend(
                self.documents
                    .hits_by_rel_path(workspace_id, &path, MENTION_CHUNK_LIMIT)?,
            );
        }
        let seen: std::collections::HashSet<String> =
            hits.iter().map(|h| h.chunk.id.clone()).collect();
        let remaining = RETRIEVE_LIMIT.saturating_sub(hits.len()).max(2);
        for hit in self.search.execute(workspace_id, query, remaining).await? {
            if !seen.contains(&hit.chunk.id) {
                hits.push(hit);
            }
        }
        if provider.is_external() {
            let blocked = self.confidential_prefixes(workspace_id)?;
            hits.retain(|h| !blocked.iter().any(|p| h.rel_path.starts_with(p)));
        }
        Ok(hits)
    }

    /// Path prefixes ("<repo>/") of sources that must not reach an external
    /// provider, plus bare single-file source names.
    fn confidential_prefixes(&self, workspace_id: &str) -> DomainResult<Vec<String>> {
        Ok(self
            .repositories
            .list_by_workspace(workspace_id)?
            .into_iter()
            .filter(|r| r.classification.is_external_forbidden())
            .flat_map(|r| [format!("{}/", r.name), r.name])
            .collect())
    }

    /// Rewrite a follow-up question into a standalone retrieval query using
    /// the conversation. Always runs on the LOCAL model — history must never
    /// reach an external provider before the consent gate. Any failure falls
    /// back to the raw question.
    async fn retrieval_query(&self, session_id: Option<&str>, question: &str) -> String {
        let Some(session_id) = session_id else {
            return question.to_string();
        };
        let Ok(history) = self.recent_history(session_id) else {
            return question.to_string();
        };
        if history.is_empty() {
            return question.to_string();
        }
        let Ok(config) = self.resolve_config(ProviderKind::Ollama) else {
            return question.to_string();
        };
        let Ok(llm) = self.router.resolve(ProviderKind::Ollama) else {
            return question.to_string();
        };
        let mut turns = history;
        turns.push(ChatTurn::user(question));
        let system = "Rewrite the user's latest message as ONE standalone search query that \
                      resolves references to the earlier conversation (\"it\", \"that file\"...). \
                      Keep the original language. Output ONLY the query text.";
        match llm.chat(&config.default_model, system, &turns).await {
            Ok(rewritten) => {
                let rewritten = rewritten.trim();
                if rewritten.is_empty() || rewritten.chars().count() > 400 {
                    question.to_string()
                } else {
                    rewritten.to_string()
                }
            }
            Err(_) => question.to_string(),
        }
    }

    /// Dry-run: retrieve sources and report whether user consent is needed
    /// before anything leaves the machine.
    pub async fn prepare(
        &self,
        workspace_id: &str,
        question: &str,
        provider: ProviderKind,
        session_id: Option<&str>,
    ) -> DomainResult<AskPreparation> {
        let workspace = self.workspaces.find_by_id(workspace_id)?;
        self.ensure_indexed(workspace_id)?;
        let config = self.resolve_config(provider)?;
        let query = self.retrieval_query(session_id, question).await;
        let hits = self.collect_hits(workspace_id, question, &query, provider).await?;
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
            self.ensure_within_budget(provider, config.monthly_budget_usd)?;
        }

        let query = self.retrieval_query(Some(session_id), question).await;
        let hits = self.collect_hits(workspace_id, question, &query, provider).await?;

        let history = self.recent_history(session_id)?;

        let user_message = Message {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            role: Role::User,
            content: question.to_string(),
            citations: Vec::new(),
            grounding: None,
            provider: None,
            model: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        self.chats.append_message(&user_message)?;

        let repositories = self.repositories.list_by_workspace(workspace_id)?;
        let system = build_system_prompt(&workspace.name, &workspace.instructions, &repositories, &hits);
        let mut turns = history;
        turns.push(ChatTurn::user(question));

        let llm = self.router.resolve(provider)?;
        let answer = llm
            .chat_stream(&config.default_model, &system, &turns, on_token)
            .await?;
        let citations = extract_citations(&answer, &hits);
        let grounding = Some(assess_grounding(&answer, &hits));

        // Usage accounting + audit trail (which sources were in the prompt).
        let prompt_chars =
            system.chars().count() + turns.iter().map(|t| t.content.chars().count()).sum::<usize>();
        let completion_chars = answer.chars().count();
        let record = UsageRecord {
            id: uuid::Uuid::new_v4().to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            provider: provider.as_str().to_string(),
            model: config.default_model.clone(),
            workspace_id: Some(workspace_id.to_string()),
            prompt_chars: prompt_chars as i64,
            completion_chars: completion_chars as i64,
            est_cost_usd: estimate_cost_usd(
                provider,
                &config.default_model,
                prompt_chars,
                completion_chars,
            ),
            sources: hits.iter().map(|h| h.rel_path.clone()).collect(),
        };
        self.usage.append(&record).ok();

        let assistant_message = Message {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            role: Role::Assistant,
            content: answer,
            citations,
            grounding,
            provider: Some(provider.as_str().to_string()),
            model: Some(config.default_model),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        self.chats.append_message(&assistant_message)?;
        Ok(assistant_message)
    }

    /// Second-opinion hallucination check: re-read the cited chunks and ask
    /// the LOCAL model whether the answer's claims are actually supported by
    /// them. Never uses an external provider — verification is free and the
    /// answer text stays on-machine.
    pub async fn verify_answer(
        &self,
        session_id: &str,
        message_id: &str,
    ) -> DomainResult<VerificationReport> {
        let messages = self.chats.list_messages(session_id)?;
        let message = messages
            .iter()
            .find(|m| m.id == message_id)
            .ok_or_else(|| DomainError::NotFound(format!("message {message_id}")))?;
        if message.role != Role::Assistant {
            return Err(DomainError::Validation(
                "only assistant answers can be verified".into(),
            ));
        }
        if message.citations.is_empty() {
            return Ok(VerificationReport {
                supported: false,
                issues: vec!["the answer cites no sources — nothing to check it against".into()],
                model: String::new(),
            });
        }

        let config = self.resolve_config(ProviderKind::Ollama)?;
        let mut sources = String::new();
        for citation in &message.citations {
            // Full chunk content when still indexed; stored snippet otherwise.
            let content = self
                .documents
                .get_chunk(&citation.chunk_id)
                .map(|c| c.content)
                .unwrap_or_else(|_| citation.snippet.clone());
            sources.push_str(&format!(
                "[{}] {} (lines {}-{})\n{}\n\n",
                citation.marker, citation.rel_path, citation.start_line, citation.end_line, content
            ));
        }

        let system = "You are a strict verification assistant. You are given numbered \
                      sources and an answer that cites them. Check every factual claim in \
                      the answer against the sources. Respond with ONLY a JSON object: \
                      {\"supported\": true|false, \"issues\": [\"...\"]}. \
                      `supported` is true only when every claim is backed by the sources. \
                      Each issue is one short sentence naming an unsupported or \
                      contradicted claim. No prose outside the JSON.";
        let prompt = format!(
            "Sources:\n{sources}Answer to verify:\n{}",
            message.content
        );
        let llm = self.router.resolve(ProviderKind::Ollama)?;
        let raw = llm
            .chat(&config.default_model, system, &[ChatTurn::user(prompt)])
            .await?;
        let parsed = parse_verification(&raw);
        Ok(VerificationReport {
            supported: parsed.0,
            issues: parsed.1,
            model: config.default_model,
        })
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
                ..Default::default()
            })
            .collect())
    }
}

/// Source-grounded system prompt: workspace overview, numbered sources,
/// mandatory citations, and an explicit instruction not to answer beyond the
/// sources (NotebookLM-style).
fn build_system_prompt(
    workspace_name: &str,
    instructions: &str,
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
    // The user's own instructions come last so they can override defaults.
    if !instructions.trim().is_empty() {
        prompt.push_str(&format!(
            "Workspace instructions from the user (follow these):\n{}\n\n",
            instructions.trim()
        ));
    }
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

/// Extract `@path/to/file` mentions from a question. A mention runs until
/// whitespace; trailing punctuation is trimmed so "@src/a.rs?" still resolves.
fn mentioned_paths(question: &str) -> Vec<String> {
    let mut paths = Vec::new();
    for token in question.split_whitespace() {
        let Some(rest) = token.strip_prefix('@') else {
            continue;
        };
        let path = rest.trim_end_matches(|c: char| matches!(c, '?' | '!' | ',' | '.' | ':' | ';' | ')' | '、' | '。'));
        if !path.is_empty() && !paths.iter().any(|p| p == path) {
            paths.push(path.to_string());
        }
    }
    paths
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

/// Deterministic grounding check: split the answer into prose claim units
/// (paragraphs and list items outside fenced code blocks) and count how many
/// carry a valid citation marker. Also collects markers that point at no
/// retrieved source — a strong hallucination signal.
fn assess_grounding(answer: &str, hits: &[SearchHit]) -> Grounding {
    /// Markers referenced in one claim unit: (valid, invalid).
    fn markers(text: &str, max: usize) -> (bool, Vec<i64>) {
        let mut valid = false;
        let mut invalid = Vec::new();
        let bytes = text.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'[' {
                if let Some(close) = text[i + 1..].find(']') {
                    let inner = &text[i + 1..i + 1 + close];
                    if !inner.is_empty() && inner.chars().all(|c| c.is_ascii_digit()) {
                        if let Ok(n) = inner.parse::<usize>() {
                            if n >= 1 && n <= max {
                                valid = true;
                            } else if !invalid.contains(&(n as i64)) {
                                invalid.push(n as i64);
                            }
                        }
                    }
                    i += close + 2;
                    continue;
                }
            }
            i += 1;
        }
        (valid, invalid)
    }

    let mut total = 0i64;
    let mut cited = 0i64;
    let mut invalid_markers: Vec<i64> = Vec::new();
    let mut in_code = false;
    // A paragraph accumulates consecutive prose lines; list items count alone.
    let mut current = String::new();
    let mut flush = |unit: &mut String, total: &mut i64, cited: &mut i64, invalid: &mut Vec<i64>| {
        let text = unit.trim();
        // Ignore trivial fragments (greetings, headings, one-word lines).
        if text.chars().count() >= 30 {
            let (has_valid, mut bad) = markers(text, hits.len());
            *total += 1;
            if has_valid {
                *cited += 1;
            }
            for m in bad.drain(..) {
                if !invalid.contains(&m) {
                    invalid.push(m);
                }
            }
        }
        unit.clear();
    };
    for line in answer.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_code = !in_code;
            flush(&mut current, &mut total, &mut cited, &mut invalid_markers);
            continue;
        }
        if in_code {
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            flush(&mut current, &mut total, &mut cited, &mut invalid_markers);
            continue;
        }
        let is_list_item = trimmed.starts_with("- ")
            || trimmed.starts_with("* ")
            || trimmed
                .split_once('.')
                .is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
        if is_list_item {
            flush(&mut current, &mut total, &mut cited, &mut invalid_markers);
            current.push_str(trimmed);
            flush(&mut current, &mut total, &mut cited, &mut invalid_markers);
        } else {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(trimmed);
        }
    }
    flush(&mut current, &mut total, &mut cited, &mut invalid_markers);
    Grounding {
        total_claims: total,
        cited_claims: cited,
        invalid_markers,
    }
}

/// Parse the verifier's JSON leniently: local models sometimes wrap it in
/// code fences or prepend prose. Unparseable output counts as NOT supported —
/// a guard must fail closed.
fn parse_verification(raw: &str) -> (bool, Vec<String>) {
    let start = raw.find('{');
    let end = raw.rfind('}');
    let json = match (start, end) {
        (Some(s), Some(e)) if e > s => &raw[s..=e],
        _ => {
            return (
                false,
                vec!["verifier returned no parseable result".to_string()],
            )
        }
    };
    match serde_json::from_str::<serde_json::Value>(json) {
        Ok(value) => {
            let supported = value["supported"].as_bool().unwrap_or(false);
            let issues = value["issues"]
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|i| i.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            (supported, issues)
        }
        Err(_) => (
            false,
            vec!["verifier returned no parseable result".to_string()],
        ),
    }
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
    fn parses_at_mentions() {
        assert_eq!(
            mentioned_paths("@src/auth.rs について教えて"),
            vec!["src/auth.rs"]
        );
        // Trailing punctuation is not part of the path.
        assert_eq!(mentioned_paths("what does @app/main.go do?"), vec!["app/main.go"]);
        assert_eq!(mentioned_paths("@a.rs and @b.rs and @a.rs"), vec!["a.rs", "b.rs"]);
        assert!(mentioned_paths("no mentions here").is_empty());
        assert!(mentioned_paths("email me@example.com").is_empty());
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
    fn grounding_counts_cited_and_uncited_claims() {
        let hits = vec![hit("a", "src/a.rs"), hit("b", "src/b.rs")];
        let answer = "The parser lives in module a and is called from main [1].\n\n\
                      It also caches results between runs for faster startup times.\n\n\
                      ```rust\nfn main() {}\n```\n\
                      - the cache is cleared on schema change [2]\n\
                      - eviction happens after ten minutes of idle time";
        let g = assess_grounding(answer, &hits);
        assert_eq!(g.total_claims, 4);
        assert_eq!(g.cited_claims, 2);
        assert!(g.invalid_markers.is_empty());
        assert_eq!(g.verdict(), "partial");
    }

    #[test]
    fn grounding_flags_invalid_markers_and_full_citation() {
        let hits = vec![hit("a", "src/a.rs")];
        let good = assess_grounding(
            "Everything about the parser is defined in the grammar module [1].",
            &hits,
        );
        assert_eq!(good.verdict(), "grounded");
        let bad = assess_grounding(
            "The scheduler retries failed jobs three times before giving up [7].",
            &hits,
        );
        assert_eq!(bad.invalid_markers, vec![7]);
        assert_eq!(bad.verdict(), "ungrounded");
    }

    #[test]
    fn grounding_ignores_code_and_headings() {
        let hits = vec![hit("a", "src/a.rs")];
        let g = assess_grounding("# Title\n\n```\nsome code that is long enough to count\n```\n", &hits);
        assert_eq!(g.total_claims, 0);
        assert_eq!(g.verdict(), "grounded");
    }

    #[test]
    fn verification_parses_json_and_fails_closed() {
        let (ok, issues) = parse_verification(
            "```json\n{\"supported\": true, \"issues\": []}\n```",
        );
        assert!(ok);
        assert!(issues.is_empty());
        let (ok, issues) = parse_verification("I think it looks fine!");
        assert!(!ok);
        assert_eq!(issues.len(), 1);
        let (ok, issues) =
            parse_verification("{\"supported\": false, \"issues\": [\"claim X is not in sources\"]}");
        assert!(!ok);
        assert_eq!(issues, vec!["claim X is not in sources".to_string()]);
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
            classification: crate::domain::entities::repository::Classification::Internal,
            created_at: "2026-01-01T00:00:00Z".into(),
        }];
        let prompt = build_system_prompt("demo", "Always answer in Japanese.", &repositories, &hits);
        assert!(prompt.contains("Workspace: demo"));
        assert!(prompt.contains("- backend (cloned from https://github.com/org/backend.git)"));
        assert!(prompt.contains("[1] src/a.rs"));
        assert!(prompt.contains("fn main() {}"));
        assert!(prompt.contains("Always answer in Japanese."));
    }
}
