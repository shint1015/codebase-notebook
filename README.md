# Codebase Notebook

A local-first, source-grounded knowledge notebook for engineers — ask questions about your
codebase and design docs, get answers **with citations**, while keeping confidential code on
your machine.

## Concept

- **Local-first**: no external network calls by default. Works with local LLMs (Ollama).
- **BYOK (Bring Your Own Key)**: optionally use OpenAI / Anthropic / other cloud providers
  with your own API key — only after an explicit per-request confirmation showing exactly
  what content would be sent.
- **Source-grounded**: answers are based on indexed sources and always cite them.
  If the sources don't contain the answer, it says so.
- **Secret-aware**: API keys, tokens, and private keys are detected and excluded at
  indexing time. API keys for providers are stored in the OS keychain, never in the DB.

## Stack

- Tauri v2 (Rust core) + React + TypeScript + Vite
- SQLite (FTS5 keyword search + embedded vector search)
- Ollama for local inference / embeddings, pluggable provider adapters for cloud models
- Clean Architecture (domain / application / infrastructure / presentation) on both sides

## Development

```bash
npm install
npm run tauri dev

# Rust tests
cd src-tauri && cargo test
```
