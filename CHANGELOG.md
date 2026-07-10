# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Chat actions in the chat header: **Fork** (duplicate the conversation into a
  new chat to branch it), **Copy** (transcript to the clipboard), and **Save
  as doc** (store the chat as an in-app markdown document, then re-index).

## [0.8.0] - 2026-07-10

### Added

- In-app markdown documents: create and edit notes inside the app. Each
  workspace has a "notes" source; documents are indexed and citable like any
  other source (re-indexed on save).
- Document editor with live preview — split view (editor + rendered markdown
  side by side) or Edit / Preview tabs. Cmd/Ctrl+S saves.

## [0.7.0] - 2026-07-10

### Added

- Agent mode in chat: the model can call tools to search sources and take
  actions. Enable "Agent mode" in the composer; write actions require the
  per-message "Allow actions" opt-in (safe by default). Tool calls are shown
  as a trace above the answer.
- Agent tools: search_sources (read), create_github_issue and write_wiki_page
  (write). Works with local Ollama (qwen2.5-coder) and OpenAI-compatible
  providers; tool calls emitted in message content are recovered.
- External service connectors as agent tools: Slack (post message), Notion
  (create page), Asana (create task), Backlog (create issue) and Confluence
  (create page). Tokens are stored in the OS keychain and each action is
  gated behind the per-message write approval. Connect them in Settings.

## [0.6.0] - 2026-07-09

### Added

- Local HTTP API on `127.0.0.1:43110` for editor integrations — token
  protected (token file in the app data dir) and restricted to the local
  provider, so it can never trigger an external send.
- VS Code extension (`vscode-extension/`): ask about the current selection or
  the whole workspace; cited answers open as a markdown preview.
- GitHub Actions CI: frontend type-check/build, Rust tests and extension
  compile on pushes and pull requests.

## [0.5.0] - 2026-07-09

### Added

- Usage & audit log: every model call is recorded locally with provider,
  model, estimated cost and the exact source files included in the prompt;
  a settings card shows monthly totals and recent calls.
- Monthly budgets per external provider — requests are blocked once the
  month's estimated spend reaches the ceiling.
- Ollama onboarding banner: detects a missing server or missing chat /
  embedding models and pulls them from inside the app with live progress.

## [0.4.0] - 2026-07-09

### Added

- Syntax-aware chunking with tree-sitter for Rust, TypeScript, JavaScript,
  Python and Go: chunks follow function/class boundaries instead of raw line
  windows (other formats keep the line-based fallback).
- Conversation-aware retrieval: follow-up questions are rewritten into
  standalone search queries by the local model (never an external provider).
- Optional local-LLM reranking of search results and a configurable Ollama
  embedding model (e.g. bge-m3 for Japanese) in a new Search settings card.

## [0.3.0] - 2026-07-09

### Added

- Streaming chat: answers render token-by-token from all providers that
  support it (Ollama, OpenAI-compatible, Anthropic).
- Assistant answers render as markdown (GFM) with styled code blocks.
- Session management: rename and delete chats from the sidebar; export a
  chat as a markdown transcript.
- Citations can be opened in the editor (VS Code `code -g`, falling back to
  the OS file manager) at the cited line.
- Sync button on managed sources: `git pull` for clones, re-fetch for GitHub
  issues, followed by re-indexing.
- Local sources are watched for file changes and re-indexed automatically
  after a quiet period.
- Publish panel: create GitHub issues (via the authenticated `gh` CLI) and
  write wiki pages (committed and pushed to the cloned wiki, then re-indexed).
- Single files can be added as sources, not only folders.
- GitHub issues can be fetched (via the authenticated `gh` CLI, or the public
  REST API) and indexed as markdown documents; GitHub wikis work through the
  existing git clone (`….wiki.git`).
- Asking in a workspace with no indexed sources now returns a clear
  "run indexing first" error instead of a model refusal.
- The system prompt includes a workspace overview (repository list), so
  meta questions like "which repositories are in this workspace?" are
  answerable.

### Changed

- Sidebar now shows the selected workspace's chat list (with "+ New chat")
  and can be collapsed to an icon rail; the model provider selector moved to
  the top-right of the chat view.

## [0.2.0] - 2026-07-09

### Added

- Google Gemini provider adapter (Generative Language API).
- Mistral and xAI (Grok) providers via the OpenAI-compatible adapter.
- Multiple repositories per workspace; add local folders or clone directly
  from a git URL (clones are app-managed and removed with the entry).
- Workspace home view with repository management and a chat session list;
  chats open in their own view with back navigation.
- Document extraction for Word (.docx), Excel/OpenDocument (.xlsx/.xls/.ods)
  and PDF files, plus CSV/TSV as plain text — all flowing through the same
  chunking, secret-redaction and citation pipeline.

### Changed

- Chat sends with Ctrl+Enter / ⌘+Enter instead of plain Enter.
- SQLite schema v2: repositories table; indexed document paths are prefixed
  with their repository name (existing indexes are cleared — re-index once).

## [0.1.0] - 2026-07-08

### Added

- Workspace management: add a local folder as an isolated project workspace.
- Indexing pipeline: file scanning, blank-line-aware chunking, incremental
  re-indexing via content hashes.
- Secret safety: credentials (AWS keys, GitHub/Slack/OpenAI/Anthropic tokens,
  private key blocks, generic assignments) are detected and redacted before
  anything enters the index; `.env`-style files are skipped entirely.
- Hybrid search: SQLite FTS5 (BM25) keyword search fused with local Ollama
  embeddings via Reciprocal Rank Fusion; degrades gracefully to keyword-only
  when no embedder is available.
- Source-grounded chat: answers cite indexed chunks with `[n]` markers, and
  the assistant is instructed to refuse questions the sources do not cover.
- BYOK provider support: Ollama (local default), OpenAI, Anthropic, and any
  OpenAI-compatible endpoint behind a Model Router / Provider Adapter split;
  API keys are stored in the OS keychain, never in the database.
- External-send consent: before any external provider call, a dialog lists the
  exact files and line ranges that would be sent, with "allow once",
  "use local model instead" and "cancel" options; the backend enforces the
  same gate independently of the UI.
- Versioned SQLite schema migrations (`PRAGMA user_version`).
