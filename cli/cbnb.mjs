#!/usr/bin/env node
// Codebase Notebook CLI — operate the desktop app from the terminal.
import { client } from "./lib.mjs";

const USAGE = `codebase-notebook CLI (cbnb)

Usage:
  cbnb workspaces                          List workspaces
  cbnb sessions <workspace_id>             List chat sessions
  cbnb notes <workspace_id>                List in-app documents
  cbnb index <workspace_id>                Re-index a workspace
  cbnb search <workspace_id> <query...>    Search indexed sources
  cbnb ask <workspace_id> <question...>    Ask (grounded, local model)
  cbnb agent <workspace_id> <task...>      Ask in agent mode (read-only tools)
  cbnb agent-write <workspace_id> <task>   Agent mode allowing write actions
  cbnb note <workspace_id> <title> <file>  Create a document from a file (- for stdin)
  cbnb health                              Check the app is reachable

Options:
  --json        Print raw JSON
  --session <id>  Continue an existing chat session (ask/agent)

Env:
  CBNB_API_BASE   default http://127.0.0.1:43110
  CBNB_TOKEN      override the API token
`;

function parseFlags(args) {
  const flags = { json: false, session: undefined };
  const rest = [];
  for (let i = 0; i < args.length; i++) {
    if (args[i] === "--json") flags.json = true;
    else if (args[i] === "--session") flags.session = args[++i];
    else rest.push(args[i]);
  }
  return { flags, rest };
}

function print(flags, value, human) {
  if (flags.json) {
    console.log(JSON.stringify(value, null, 2));
  } else {
    human(value);
  }
}

async function main() {
  const [command, ...raw] = process.argv.slice(2);
  const { flags, rest } = parseFlags(raw);

  if (!command || command === "help" || command === "--help") {
    console.log(USAGE);
    return;
  }

  switch (command) {
    case "health":
      print(flags, await client.health(), (v) => console.log(v.ok ? "ok" : "not ok"));
      break;

    case "workspaces": {
      const list = await client.listWorkspaces();
      print(flags, list, (ws) =>
        ws.forEach((w) => console.log(`${w.id}  ${w.name}`)),
      );
      break;
    }

    case "sessions": {
      const [workspaceId] = rest;
      requireArg(workspaceId, "workspace_id");
      const list = await client.listSessions(workspaceId);
      print(flags, list, (s) =>
        s.forEach((x) => console.log(`${x.id}  ${x.title}`)),
      );
      break;
    }

    case "notes": {
      const [workspaceId] = rest;
      requireArg(workspaceId, "workspace_id");
      const list = await client.listNotes(workspaceId);
      print(flags, list, (n) =>
        n.forEach((x) => console.log(`${x.name}\t${x.updated_at}`)),
      );
      break;
    }

    case "index": {
      const [workspaceId] = rest;
      requireArg(workspaceId, "workspace_id");
      const report = await client.index(workspaceId);
      print(flags, report, (r) =>
        console.log(
          `indexed ${r.files_indexed} files (${r.files_unchanged} unchanged), ${r.chunks_created} chunks`,
        ),
      );
      break;
    }

    case "search": {
      const [workspaceId, ...q] = rest;
      requireArg(workspaceId, "workspace_id");
      const hits = await client.search(workspaceId, q.join(" "), 10);
      print(flags, hits, (h) =>
        h.forEach((hit) =>
          console.log(
            `${hit.rel_path}:${hit.chunk.start_line}  (${hit.score.toFixed(3)})`,
          ),
        ),
      );
      break;
    }

    case "ask":
    case "agent":
    case "agent-write": {
      const [workspaceId, ...q] = rest;
      requireArg(workspaceId, "workspace_id");
      const opts = {
        sessionId: flags.session,
        agent: command !== "ask",
        allowWrites: command === "agent-write",
      };
      const res = await client.ask(workspaceId, q.join(" "), opts);
      print(flags, res, (r) => {
        console.log(r.message.content);
        if (r.tool_events?.length) {
          console.log("\n--- tools ---");
          r.tool_events.forEach((e) =>
            console.log(`${e.blocked ? "[blocked] " : ""}${e.summary}`),
          );
        }
        if (r.message.citations?.length) {
          console.log("\n--- sources ---");
          r.message.citations.forEach((c) =>
            console.log(`[${c.marker}] ${c.rel_path}:${c.start_line}`),
          );
        }
        console.log(`\n(session ${r.session_id})`);
      });
      break;
    }

    case "note": {
      const [workspaceId, title, file] = rest;
      requireArg(workspaceId, "workspace_id");
      requireArg(title, "title");
      requireArg(file, "file (use - for stdin)");
      const content =
        file === "-"
          ? await readStdin()
          : (await import("node:fs")).readFileSync(file, "utf8");
      const res = await client.createNote(workspaceId, title, content);
      print(flags, res, (r) => console.log(`created ${r.name}`));
      break;
    }

    default:
      console.error(`unknown command: ${command}\n`);
      console.log(USAGE);
      process.exit(1);
  }
}

function requireArg(value, name) {
  if (!value) {
    console.error(`error: missing ${name}`);
    process.exit(1);
  }
}

function readStdin() {
  return new Promise((resolve) => {
    let data = "";
    process.stdin.setEncoding("utf8");
    process.stdin.on("data", (chunk) => (data += chunk));
    process.stdin.on("end", () => resolve(data));
  });
}

main().catch((e) => {
  console.error(`error: ${e.message}`);
  process.exit(1);
});
