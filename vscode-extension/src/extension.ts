import * as vscode from "vscode";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";

const API_BASE = "http://127.0.0.1:43110";
const APP_ID = "com.spcsft.codebase-notebook";

interface Workspace {
  id: string;
  name: string;
}

interface Citation {
  marker: number;
  rel_path: string;
  start_line: number;
  end_line: number;
}

interface AskResponse {
  session_id: string;
  message: {
    content: string;
    citations: Citation[];
    model: string | null;
  };
}

/** The app writes its API token under its data dir; only this user can read it. */
function tokenPath(): string {
  switch (process.platform) {
    case "darwin":
      return path.join(os.homedir(), "Library", "Application Support", APP_ID, "api-token");
    case "win32":
      return path.join(process.env.APPDATA ?? "", APP_ID, "api-token");
    default:
      return path.join(
        process.env.XDG_DATA_HOME ?? path.join(os.homedir(), ".local", "share"),
        APP_ID,
        "api-token",
      );
  }
}

function readToken(): string | null {
  try {
    const token = fs.readFileSync(tokenPath(), "utf8").trim();
    return token.length > 0 ? token : null;
  } catch {
    return null;
  }
}

async function api<T>(route: string, init?: RequestInit): Promise<T> {
  const token = readToken();
  if (!token) {
    throw new Error(
      "Codebase Notebook app is not set up — launch the app once so it can create its API token.",
    );
  }
  const response = await fetch(`${API_BASE}${route}`, {
    ...init,
    headers: {
      Authorization: `Bearer ${token}`,
      "Content-Type": "application/json",
      ...(init?.headers ?? {}),
    },
  });
  if (!response.ok) {
    const body = (await response.json().catch(() => ({}))) as { error?: string };
    throw new Error(body.error ?? `API returned ${response.status}`);
  }
  return (await response.json()) as T;
}

async function pickWorkspace(context: vscode.ExtensionContext): Promise<Workspace | null> {
  const workspaces = await api<Workspace[]>("/api/workspaces");
  if (workspaces.length === 0) {
    vscode.window.showWarningMessage("Codebase Notebook has no workspaces yet.");
    return null;
  }
  const lastId = context.globalState.get<string>("lastWorkspaceId");
  if (workspaces.length === 1) return workspaces[0];
  const items = workspaces
    .sort((a, b) => (a.id === lastId ? -1 : b.id === lastId ? 1 : 0))
    .map((ws) => ({ label: ws.name, ws }));
  const picked = await vscode.window.showQuickPick(items, {
    placeHolder: "Codebase Notebook workspace",
  });
  if (!picked) return null;
  await context.globalState.update("lastWorkspaceId", picked.ws.id);
  return picked.ws;
}

async function ask(context: vscode.ExtensionContext, withSelection: boolean): Promise<void> {
  const editor = vscode.window.activeTextEditor;
  const selection =
    withSelection && editor && !editor.selection.isEmpty
      ? editor.document.getText(editor.selection)
      : null;

  const workspace = await pickWorkspace(context);
  if (!workspace) return;

  const question = await vscode.window.showInputBox({
    prompt: selection
      ? "Question about the selected code"
      : `Ask workspace "${workspace.name}"`,
    placeHolder: "e.g. Where is the session token validated?",
  });
  if (!question) return;

  const language = editor?.document.languageId ?? "";
  const fullQuestion = selection
    ? `${question}\n\nRegarding this code from ${path.basename(
        editor!.document.fileName,
      )}:\n\`\`\`${language}\n${selection}\n\`\`\``
    : question;

  await vscode.window.withProgress(
    {
      location: vscode.ProgressLocation.Notification,
      title: "Codebase Notebook: asking the local model…",
      cancellable: false,
    },
    async () => {
      const answer = await api<AskResponse>("/api/ask", {
        method: "POST",
        body: JSON.stringify({ workspace_id: workspace.id, question: fullQuestion }),
      });
      const citations = answer.message.citations
        .map((c) => `- [${c.marker}] ${c.rel_path} (lines ${c.start_line}-${c.end_line})`)
        .join("\n");
      const content = `# ${question}\n\n${answer.message.content}\n\n${
        citations ? `---\nSources:\n${citations}\n` : ""
      }`;
      const doc = await vscode.workspace.openTextDocument({
        content,
        language: "markdown",
      });
      await vscode.window.showTextDocument(doc, { preview: true, viewColumn: vscode.ViewColumn.Beside });
    },
  );
}

export function activate(context: vscode.ExtensionContext): void {
  context.subscriptions.push(
    vscode.commands.registerCommand("codebaseNotebook.askSelection", async () => {
      try {
        await ask(context, true);
      } catch (error) {
        vscode.window.showErrorMessage(`Codebase Notebook: ${error}`);
      }
    }),
    vscode.commands.registerCommand("codebaseNotebook.ask", async () => {
      try {
        await ask(context, false);
      } catch (error) {
        vscode.window.showErrorMessage(`Codebase Notebook: ${error}`);
      }
    }),
  );
}

export function deactivate(): void {}
