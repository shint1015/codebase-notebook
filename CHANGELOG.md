# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Google Gemini provider adapter (Generative Language API).
- Mistral and xAI (Grok) providers via the OpenAI-compatible adapter.

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
