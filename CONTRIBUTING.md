# Contributing

## Stack

Tauri v2 (Rust core) + React + TypeScript + Vite, SQLite (FTS5 keyword search +
embedded vector search), Ollama for local inference/embeddings with pluggable
provider adapters for cloud models. Clean Architecture (domain / application /
infrastructure / presentation) on both the Rust and frontend sides.

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

## Update signing key

Releases are signed so the in-app updater can verify them. The keypair was
generated with `tauri signer generate`:

- **Public key** — committed in `src-tauri/tauri.conf.json` (`plugins.updater.pubkey`).
- **Private key** — stored as the repo secret `TAURI_SIGNING_PRIVATE_KEY`
  (with `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`), used by the release workflow.

⚠️ **Back the private key up.** If it is lost, already-installed apps will
reject every future update — the only fix is asking users to reinstall by hand.
Keep an offline copy of the key file outside the repo.

Local signed build:

```bash
export TAURI_SIGNING_PRIVATE_KEY="$(cat /path/to/updater.key)"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
npm run tauri build
```

## Demo GIF

The README's demo GIF must be a **real screen recording** of the app — never a
mock-up, since readers take it as what the product actually does.

1. Run the app with a workspace that has sources indexed.
2. Record ~30s with Cmd+Shift+5 (macOS): add a source → index → ask a question
   → click a citation.
3. Convert and optimise:

   ```bash
   ./scripts/make-demo-gif.sh ~/Desktop/recording.mov [start] [duration]
   ```

4. Uncomment the `media/demo.gif` block at the top of README.md.
