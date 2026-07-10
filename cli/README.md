# Codebase Notebook CLI (`cbnb`)

Operate the [Codebase Notebook](../README.md) desktop app from the terminal.
It talks to the app's local API (`127.0.0.1:43110`) using the token the app
writes to its data dir — so the desktop app must be running.

Answers use the **local** model only (the API can never trigger an external
send), keeping the CLI Local-first.

## Install

```bash
cd cli
npm link          # exposes `cbnb` globally (or: node cbnb.mjs …)
```

## Usage

```bash
cbnb workspaces                          # list workspaces (id  name)
cbnb notes <workspace_id>                # list in-app documents
cbnb index <workspace_id>                # re-index a workspace
cbnb search <workspace_id> <query...>    # search indexed sources
cbnb ask <workspace_id> <question...>    # grounded answer with citations
cbnb agent <workspace_id> <task...>      # agent mode (read-only tools)
cbnb agent-write <workspace_id> <task>   # agent mode allowing write actions
cbnb note <workspace_id> <title> <file>  # create a document (- = stdin)
cbnb health

# machine-readable output
cbnb workspaces --json
# continue a chat
cbnb ask <ws> "and its tests?" --session <session_id>
```

## Environment

- `CBNB_API_BASE` — API base URL (default `http://127.0.0.1:43110`)
- `CBNB_TOKEN` / `CBNB_TOKEN_PATH` — override the API token / its path
