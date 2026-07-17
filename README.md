# Codebase Notebook

[![Release](https://img.shields.io/github/v/release/shint1015/codebase-notebook?sort=semver)](https://github.com/shint1015/codebase-notebook/releases/latest)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![CI](https://github.com/shint1015/codebase-notebook/actions/workflows/ci.yml/badge.svg)](https://github.com/shint1015/codebase-notebook/actions/workflows/ci.yml)

**Ask questions about your own code, docs, and issues — get answers with
citations, without your confidential code leaving your machine.**

<!-- TODO: record a ~30s demo (add workspace → index → ask → click a citation)
     and drop it here as media/demo.gif — this is the highest-converting asset
     on the whole page.
<p align="center"><img src="media/demo.gif" alt="Codebase Notebook demo" width="800"></p>
-->

<p align="center"><em>Demo GIF coming soon.</em></p>

Codebase Notebook is a local-first desktop app (macOS / Windows / Linux) that
indexes your repositories, design docs, GitHub issues, and personal notes, and
lets you chat with them. Answers are grounded in *your* sources and always cite
where they came from. By default everything runs on a local model — nothing is
sent to the cloud unless you explicitly opt in.

---

## Why you might want it

- **Understand a large codebase** — "Where is the session token validated?"
  and get a cited answer pointing at the exact files and lines.
- **Search across everything at once** — code, Markdown/PDF/Word/Excel docs,
  GitHub issues, wikis, and your own notes, in one place.
- **Keep confidential code private** — it works fully offline with a local
  model. Cloud AI is optional, opt-in, and shows you exactly what would be sent
  before it sends anything.
- **Take action** — in Agent mode it can file a GitHub issue, write a wiki
  page, post to Slack, or create a Notion/Asana/Backlog/Confluence entry — but
  only when you approve.

---

## Requirements

- **A local AI model via [Ollama](https://ollama.com)** (recommended, free,
  runs offline). Install it, then pull the models:
  ```bash
  ollama pull qwen2.5-coder:14b   # chat / answers
  ollama pull nomic-embed-text    # embeddings (better search)
  ```
  A smaller machine can use `qwen2.5-coder:7b` or `:1.5b`. The app also has an
  in-app button to pull these for you if they're missing.
- Alternatively (or in addition), a **cloud provider API key** — OpenAI,
  Anthropic, Google Gemini, Mistral, or xAI — if you want to use those models.

> No Ollama and no key? The app still works with keyword-only search, but
> can't generate chat answers until a model is available.

## Install

**[⬇ Download the latest release](https://github.com/shint1015/codebase-notebook/releases/latest)**
for macOS (Apple Silicon / Intel), Windows, or Linux.

The builds are **unsigned** (signing certificates cost money; this project is
free), so your OS will warn you on first launch:

- **macOS** — right-click the app → *Open* → *Open*, or run
  `xattr -cr "/Applications/Codebase Notebook.app"`
- **Windows** — SmartScreen: *More info* → *Run anyway*

<details>
<summary>Or build it yourself</summary>

Requires [Rust](https://rustup.rs) and Node.js 18+.

```bash
git clone https://github.com/shint1015/codebase-notebook
cd codebase-notebook
npm install
npm run tauri build   # installer in src-tauri/target/release/bundle/
npm run tauri dev     # …or just run it
```

</details>

---

## Getting started

1. **Create a workspace** — click **+ Add workspace** in the sidebar and give
   it a name. A workspace is an isolated project; sources and chats never mix
   between workspaces.
2. **Add sources** — on the workspace home, add any of:
   - **Folder** or **single file** on your disk
   - a **git repository** by URL (it's cloned and managed for you)
   - a **GitHub wiki** (use the `…/repo.wiki.git` URL)
   - **GitHub issues** (`owner/repo`) — fetched and stored as Markdown
3. **Index** — click **Index all repositories**. This scans your files, splits
   them into pieces, and makes them searchable. (Secrets like API keys are
   detected and stripped out before anything is indexed.)
4. **Ask** — pick or start a chat in the sidebar and ask a question. Answers
   cite their sources; click a citation to open that file at the right line.

Local sources are watched for changes and re-indexed automatically. Managed
sources (git clones, issues) have a **Sync** button to pull the latest.

---

## Using it

### Chat

- Answers stream in and render as Markdown (code blocks, tables, etc.).
- **Citations** under each answer link back to the exact source and line.
- Hover a message to **copy** it or **fork** the conversation from that point
  into a new chat.
- From the chat header you can **Copy** the whole transcript, **Save as doc**
  (turns the chat into a searchable in-app document), or **Export** to a `.md`
  file. Rename and delete chats from the sidebar.

### Documents

Create Markdown notes right inside the app (**Documents → + New document**),
with a live **split / edit / preview** editor (Cmd/Ctrl+S to save). Your notes
become part of the workspace — indexed, searchable, and citable like any other
source.

### Cloud models (optional, Bring Your Own Key)

Open **⚙ AI Providers** to add a key for OpenAI, Anthropic, Gemini, Mistral, or
xAI. When you use a cloud model, a dialog shows **exactly which files and lines
would be sent** and asks you to approve — you can always fall back to the local
model. Keys are stored in your OS keychain, never in a plain file, and you can
set a **monthly budget** per provider. A usage log records every call with its
estimated cost and the sources included.

### Agent mode

Toggle **🛠 Agent mode** in the composer to let the model *act*, not just
answer — search your sources, then optionally take an action. Actions
(creating a GitHub issue, writing a wiki page, posting to Slack, creating a
Notion / Asana / Backlog / Confluence item) require you to tick **Allow
actions** for that message, so nothing external happens without your consent.
Connect those services under **⚙ AI Providers → Connectors**.

---

## Beyond the app: CLI, editor, and MCP

While the app is running it exposes a small, token-protected API on
`127.0.0.1` (local only, local-model only). Three optional bridges use it:

- **CLI** ([`cli/`](cli/README.md)) — `cbnb search`, `cbnb ask`, `cbnb note`,
  etc. from your terminal.
- **VS Code extension** ([`vscode-extension/`](vscode-extension/README.md)) —
  ask about the current selection or workspace without leaving your editor.
- **MCP server** ([`mcp-server/`](mcp-server/README.md)) — expose your
  workspaces to Claude Desktop and other MCP clients as tools.

---

## Your privacy

- **Offline by default.** No network calls unless you enable a cloud provider.
- **Explicit consent** before any code is sent to a cloud model, showing the
  exact content.
- **Secrets are stripped** from your sources before indexing; provider keys and
  connector tokens live in the OS keychain.
- **Everything is stored locally** — your index, chats, and documents stay in
  the app's data directory on your machine.

---

## For developers

Built with Tauri v2 (Rust) + React/TypeScript, SQLite (FTS5 + vector search),
and Ollama, following Clean Architecture on both the Rust and frontend sides.

```bash
npm install
npm run tauri dev
cd src-tauri && cargo test    # backend tests
```

Contribution workflow, branching, and release/versioning are documented in
[CONTRIBUTING.md](CONTRIBUTING.md). Release notes live in
[CHANGELOG.md](CHANGELOG.md).
