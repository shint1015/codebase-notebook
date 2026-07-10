# Codebase Notebook MCP server

An [MCP](https://modelcontextprotocol.io) server that exposes the
[Codebase Notebook](../README.md) desktop app's local API as tools, so MCP
clients (Claude Desktop, etc.) can search your indexed sources, ask grounded
questions, re-index, and write documents.

The desktop app must be running — it serves the token-protected local API on
`127.0.0.1:43110`. Answers use the **local** model only, so the bridge can
never trigger an external send.

## Tools

- `list_workspaces` — list workspaces (id + name)
- `search_sources` — search a workspace's indexed sources
- `ask` — grounded, cited answer from the local model
- `index_workspace` — re-index a workspace
- `list_notes` — list in-app documents
- `create_note` — create an indexed markdown document

## Install

```bash
cd mcp-server
npm install
```

## Configure an MCP client

Add to your client's MCP config (e.g. Claude Desktop
`claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "codebase-notebook": {
      "command": "node",
      "args": ["/absolute/path/to/codebase-notebook/mcp-server/server.mjs"]
    }
  }
}
```

Optional env: `CBNB_API_BASE` (default `http://127.0.0.1:43110`), `CBNB_TOKEN`
/ `CBNB_TOKEN_PATH` to override the token or its location.
