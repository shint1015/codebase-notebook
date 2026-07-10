#!/usr/bin/env node
// MCP server for Codebase Notebook. Exposes the desktop app's local API as
// MCP tools so any MCP client (Claude Desktop, etc.) can search the user's
// indexed sources, ask grounded questions, index, and write documents.
//
// The desktop app must be running (it serves the token-protected local API).
// Answers use the LOCAL model only — the bridge can never trigger an external
// send.
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";
import { client } from "./lib.mjs";

const server = new McpServer({
  name: "codebase-notebook",
  version: "0.1.0",
});

const text = (value) => ({
  content: [
    {
      type: "text",
      text: typeof value === "string" ? value : JSON.stringify(value, null, 2),
    },
  ],
});

const errorText = (e) => ({
  content: [{ type: "text", text: `error: ${e.message}` }],
  isError: true,
});

server.tool(
  "list_workspaces",
  "List the Codebase Notebook workspaces (id and name).",
  {},
  async () => {
    try {
      return text(await client.listWorkspaces());
    } catch (e) {
      return errorText(e);
    }
  },
);

server.tool(
  "search_sources",
  "Search a workspace's indexed sources (code, docs, issues, notes). Returns matching chunks with file paths and line numbers.",
  {
    workspace_id: z.string(),
    query: z.string(),
    limit: z.number().int().min(1).max(50).optional(),
  },
  async ({ workspace_id, query, limit }) => {
    try {
      return text(await client.search(workspace_id, query, limit ?? 10));
    } catch (e) {
      return errorText(e);
    }
  },
);

server.tool(
  "ask",
  "Ask a question grounded in a workspace's indexed sources. Returns a cited answer produced by the local model.",
  {
    workspace_id: z.string(),
    question: z.string(),
    session_id: z.string().optional(),
  },
  async ({ workspace_id, question, session_id }) => {
    try {
      const res = await client.ask(workspace_id, question, { sessionId: session_id });
      return text(res);
    } catch (e) {
      return errorText(e);
    }
  },
);

server.tool(
  "index_workspace",
  "Re-index a workspace so recent source changes become searchable.",
  { workspace_id: z.string() },
  async ({ workspace_id }) => {
    try {
      return text(await client.index(workspace_id));
    } catch (e) {
      return errorText(e);
    }
  },
);

server.tool(
  "list_notes",
  "List the in-app markdown documents (notes) of a workspace.",
  { workspace_id: z.string() },
  async ({ workspace_id }) => {
    try {
      return text(await client.listNotes(workspace_id));
    } catch (e) {
      return errorText(e);
    }
  },
);

server.tool(
  "create_note",
  "Create an in-app markdown document in a workspace. It is indexed and becomes searchable/citable.",
  {
    workspace_id: z.string(),
    title: z.string(),
    content: z.string(),
  },
  async ({ workspace_id, title, content }) => {
    try {
      return text(await client.createNote(workspace_id, title, content));
    } catch (e) {
      return errorText(e);
    }
  },
);

const transport = new StdioServerTransport();
await server.connect(transport);
