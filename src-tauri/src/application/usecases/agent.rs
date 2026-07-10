use std::collections::HashMap;
use std::sync::Arc;

use serde::Serialize;

use crate::domain::entities::chat::{Message, Role};
use crate::domain::entities::provider::ProviderKind;
use crate::domain::error::{DomainError, DomainResult};
use crate::domain::repositories::{ChatRepository, ProviderConfigRepository, WorkspaceRepository};
use crate::domain::services::{
    AgentStep, ChatTurn, ProviderRouter, Tool, ToolCall,
};

/// Hard cap on tool-call rounds so a confused model can't loop forever.
const MAX_ROUNDS: usize = 8;
const HISTORY_TURNS: usize = 8;

/// A tool call the agent ran (or proposed), surfaced to the UI as a trace.
#[derive(Debug, Clone, Serialize)]
pub struct ToolEvent {
    pub name: String,
    pub summary: String,
    pub result: String,
    /// True when a write action was skipped because the user hadn't approved.
    pub blocked: bool,
}

/// Outcome of one agent run.
#[derive(Debug, Serialize)]
pub struct AgentOutcome {
    pub message: Message,
    pub tool_events: Vec<ToolEvent>,
}

pub struct AgentUseCase {
    workspaces: Arc<dyn WorkspaceRepository>,
    chats: Arc<dyn ChatRepository>,
    providers: Arc<dyn ProviderConfigRepository>,
    router: Arc<dyn ProviderRouter>,
    tools: Vec<Arc<dyn Tool>>,
}

impl AgentUseCase {
    pub fn new(
        workspaces: Arc<dyn WorkspaceRepository>,
        chats: Arc<dyn ChatRepository>,
        providers: Arc<dyn ProviderConfigRepository>,
        router: Arc<dyn ProviderRouter>,
        tools: Vec<Arc<dyn Tool>>,
    ) -> Self {
        Self {
            workspaces,
            chats,
            providers,
            router,
            tools,
        }
    }

    fn tool_by_name(&self, name: &str) -> Option<&Arc<dyn Tool>> {
        self.tools.iter().find(|t| t.spec().name == name)
    }

    /// Run an agentic turn. Read tools run freely; write tools run only when
    /// `allow_writes` is true, otherwise they are reported as blocked and the
    /// model is told it lacks permission (safe by default).
    pub async fn run(
        &self,
        session_id: &str,
        workspace_id: &str,
        question: &str,
        provider: ProviderKind,
        allow_writes: bool,
    ) -> DomainResult<AgentOutcome> {
        let workspace = self.workspaces.find_by_id(workspace_id)?;
        let config = self
            .providers
            .find(provider)?
            .unwrap_or_else(|| {
                crate::domain::entities::provider::ProviderConfig::default_for(provider)
            });
        if !config.enabled {
            return Err(DomainError::ProviderNotConfigured(format!(
                "{} is not enabled",
                provider.as_str()
            )));
        }
        let llm = self.router.resolve(provider)?;
        let specs: Vec<_> = self.tools.iter().map(|t| t.spec()).collect();

        // Persist the user's message.
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

        let mut turns = self.recent_history(session_id)?;
        let system = build_system_prompt(&workspace.name, allow_writes);
        let mut events: Vec<ToolEvent> = Vec::new();

        let mut final_text = String::new();
        for _ in 0..MAX_ROUNDS {
            match llm
                .chat_with_tools(&config.default_model, &system, &turns, &specs)
                .await?
            {
                AgentStep::Message(text) => {
                    final_text = text;
                    break;
                }
                AgentStep::ToolCalls(calls) => {
                    // Record the assistant's tool-call turn.
                    turns.push(ChatTurn {
                        role: "assistant".into(),
                        content: String::new(),
                        tool_calls: calls.clone(),
                        tool_call_id: None,
                    });
                    for call in calls {
                        let (result, blocked, summary) =
                            self.run_tool(workspace_id, &call, allow_writes).await;
                        events.push(ToolEvent {
                            name: call.name.clone(),
                            summary,
                            result: result.clone(),
                            blocked,
                        });
                        turns.push(ChatTurn {
                            role: "tool".into(),
                            content: result,
                            tool_calls: Vec::new(),
                            tool_call_id: Some(call.id),
                        });
                    }
                }
            }
        }
        if final_text.is_empty() {
            final_text = "I couldn't complete this within the tool-call limit.".to_string();
        }

        let assistant_message = Message {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            role: Role::Assistant,
            content: final_text,
            citations: Vec::new(),
            provider: Some(provider.as_str().to_string()),
            model: Some(config.default_model),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        self.chats.append_message(&assistant_message)?;
        Ok(AgentOutcome {
            message: assistant_message,
            tool_events: events,
        })
    }

    /// Returns (result-for-model, blocked, human-summary).
    async fn run_tool(
        &self,
        workspace_id: &str,
        call: &ToolCall,
        allow_writes: bool,
    ) -> (String, bool, String) {
        let Some(tool) = self.tool_by_name(&call.name) else {
            return (
                format!("Error: unknown tool \"{}\".", call.name),
                false,
                format!("unknown tool {}", call.name),
            );
        };
        let summary = tool.describe_call(&call.arguments);
        if tool.requires_consent() && !allow_writes {
            return (
                "Permission denied: the user has not enabled write actions for this message. \
                 Describe what you would do and ask them to enable actions."
                    .to_string(),
                true,
                summary,
            );
        }
        match tool.execute(workspace_id, &call.arguments).await {
            Ok(result) => (result, false, summary),
            Err(error) => (format!("Error: {error}"), false, summary),
        }
    }

    fn recent_history(&self, session_id: &str) -> DomainResult<Vec<ChatTurn>> {
        let messages = self.chats.list_messages(session_id)?;
        Ok(messages
            .iter()
            .rev()
            .skip(1) // drop the user message we just stored; it's re-added below
            .take(HISTORY_TURNS)
            .rev()
            .map(|m| ChatTurn {
                role: match m.role {
                    Role::User => "user".into(),
                    Role::Assistant => "assistant".into(),
                },
                content: m.content.clone(),
                tool_calls: Vec::new(),
                tool_call_id: None,
            })
            .chain(std::iter::once(ChatTurn::user(
                messages
                    .last()
                    .map(|m| m.content.clone())
                    .unwrap_or_default(),
            )))
            .collect())
    }
}

fn build_system_prompt(workspace_name: &str, allow_writes: bool) -> String {
    let mut prompt = format!(
        "You are Codebase Notebook's agent for the workspace \"{workspace_name}\".\n\
         You can call tools to search the workspace's indexed sources and to take actions \
         (create issues, write wiki pages).\n\
         Guidelines:\n\
         1. Ground answers in search_sources before making claims about the code or docs.\n\
         2. Use the smallest number of tool calls needed.\n\
         3. When you have enough information, reply directly to the user in their language.\n"
    );
    if allow_writes {
        prompt.push_str(
            "4. Write actions ARE permitted this turn. Still confirm the essentials (repo, \
             title) are right before calling a write tool.\n",
        );
    } else {
        prompt.push_str(
            "4. Write actions are NOT permitted this turn. If the user asks for one, explain \
             exactly what you would do and tell them to enable \"Allow actions\" and resend.\n",
        );
    }
    prompt
}

/// Tool specs are looked up by name; expose a helper for tests/UI listings.
pub fn tool_names(tools: &[Arc<dyn Tool>]) -> HashMap<String, bool> {
    tools
        .iter()
        .map(|t| (t.spec().name, t.requires_consent()))
        .collect()
}
