// Shared client for the Codebase Notebook local API. Used by both the CLI
// and the MCP server. Talks to 127.0.0.1:43110 with the token the desktop
// app writes to its app-data dir.
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

export const API_BASE = process.env.CBNB_API_BASE ?? "http://127.0.0.1:43110";
const APP_ID = "com.spcsft.codebase-notebook";

function tokenPath() {
  if (process.env.CBNB_TOKEN_PATH) return process.env.CBNB_TOKEN_PATH;
  switch (process.platform) {
    case "darwin":
      return join(homedir(), "Library", "Application Support", APP_ID, "api-token");
    case "win32":
      return join(process.env.APPDATA ?? "", APP_ID, "api-token");
    default:
      return join(
        process.env.XDG_DATA_HOME ?? join(homedir(), ".local", "share"),
        APP_ID,
        "api-token",
      );
  }
}

export function readToken() {
  if (process.env.CBNB_TOKEN) return process.env.CBNB_TOKEN.trim();
  try {
    const token = readFileSync(tokenPath(), "utf8").trim();
    return token.length > 0 ? token : null;
  } catch {
    return null;
  }
}

export async function apiRequest(method, route, body) {
  const token = readToken();
  if (!token) {
    throw new Error(
      "No API token found. Launch the Codebase Notebook desktop app once so it can create the token, or set CBNB_TOKEN.",
    );
  }
  let response;
  try {
    response = await fetch(`${API_BASE}${route}`, {
      method,
      headers: {
        Authorization: `Bearer ${token}`,
        "Content-Type": "application/json",
      },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  } catch (e) {
    throw new Error(
      `Cannot reach the Codebase Notebook app at ${API_BASE}. Is it running? (${e.message})`,
    );
  }
  const text = await response.text();
  const data = text ? JSON.parse(text) : {};
  if (!response.ok) {
    throw new Error(data.error ?? `API returned ${response.status}`);
  }
  return data;
}

export const client = {
  health: () => apiRequest("GET", "/health"),
  listWorkspaces: () => apiRequest("GET", "/api/workspaces"),
  listSessions: (workspaceId) =>
    apiRequest("GET", `/api/sessions?workspace_id=${encodeURIComponent(workspaceId)}`),
  listNotes: (workspaceId) =>
    apiRequest("GET", `/api/notes?workspace_id=${encodeURIComponent(workspaceId)}`),
  search: (workspaceId, query, limit) =>
    apiRequest("POST", "/api/search", { workspace_id: workspaceId, query, limit }),
  index: (workspaceId) => apiRequest("POST", "/api/index", { workspace_id: workspaceId }),
  createNote: (workspaceId, title, content) =>
    apiRequest("POST", "/api/notes", { workspace_id: workspaceId, title, content }),
  ask: (workspaceId, question, opts = {}) =>
    apiRequest("POST", "/api/ask", {
      workspace_id: workspaceId,
      question,
      session_id: opts.sessionId,
      agent: opts.agent ?? false,
      allow_writes: opts.allowWrites ?? false,
    }),
};
