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

## Branching & Versioning

Branches follow a develop-based flow:

- `main` — released versions only; every release commit is tagged `vX.Y.Z`.
- `develop` — integration branch. Carries a `-dev.N` prerelease version.
- `feature/*`, `fix/*` — cut from `develop`, merged back into `develop`.

The project follows [Semantic Versioning](https://semver.org/); `package.json`
is the single source of truth and `scripts/bump-version.mjs` propagates the
version to `tauri.conf.json`, `Cargo.toml`, `Cargo.lock`, and `CHANGELOG.md`.

Releasing (on `main`, when merging develop):

```bash
git checkout main && git merge --no-ff develop
npm run version:minor        # or version:patch / version:major
                             # finalizes 0.3.0-dev.N -> 0.3.0 and promotes
                             # CHANGELOG [Unreleased] into the new version
git add -A && git commit -m "Release vX.Y.Z"
git tag vX.Y.Z
git checkout develop && git merge main   # bring the release back
npm run version:dev          # optional: start the next prerelease (0.4.0-dev.0)
git add -A && git commit -m "Start X.Y.Z-dev.0"
```

Day-to-day on `develop`: add CHANGELOG notes under `[Unreleased]` as you work;
run `npm run version:dev` whenever you want a new distinguishable dev build.
The SQLite schema is versioned independently via `PRAGMA user_version`
migrations in `src-tauri/src/infrastructure/persistence/mod.rs` (append-only).
