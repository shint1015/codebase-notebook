import { useMemo } from "react";
import CodeMirror from "@uiw/react-codemirror";
import { oneDark } from "@codemirror/theme-one-dark";
import { EditorView } from "@codemirror/view";
import { markdown } from "@codemirror/lang-markdown";
import { javascript } from "@codemirror/lang-javascript";
import { rust } from "@codemirror/lang-rust";
import { python } from "@codemirror/lang-python";
import { json } from "@codemirror/lang-json";
import { go } from "@codemirror/lang-go";
import { html } from "@codemirror/lang-html";
import { css } from "@codemirror/lang-css";
import { sql } from "@codemirror/lang-sql";
import { yaml } from "@codemirror/lang-yaml";

/** Pick a CodeMirror language extension from a file name. */
function languageFor(fileName: string) {
  const ext = fileName.split(".").pop()?.toLowerCase() ?? "";
  switch (ext) {
    case "md":
    case "markdown":
      return [markdown()];
    case "ts":
    case "tsx":
      return [javascript({ typescript: true, jsx: ext === "tsx" })];
    case "js":
    case "jsx":
    case "mjs":
    case "cjs":
      return [javascript({ jsx: ext === "jsx" })];
    case "rs":
      return [rust()];
    case "py":
      return [python()];
    case "json":
      return [json()];
    case "go":
      return [go()];
    case "html":
    case "htm":
      return [html()];
    case "css":
    case "scss":
      return [css()];
    case "sql":
      return [sql()];
    case "yaml":
    case "yml":
      return [yaml()];
    default:
      return [];
  }
}

interface Props {
  value: string;
  fileName: string;
  onChange?: (value: string) => void;
  readOnly?: boolean;
  /** 1-based line to scroll to and highlight on open. */
  jumpToLine?: number;
  onSave?: () => void;
}

export function CodeEditor({
  value,
  fileName,
  onChange,
  readOnly = false,
  jumpToLine,
  onSave,
}: Props) {
  const extensions = useMemo(
    () => [...languageFor(fileName), EditorView.lineWrapping],
    [fileName],
  );

  return (
    <CodeMirror
      className="code-editor"
      value={value}
      height="100%"
      theme={oneDark}
      extensions={extensions}
      editable={!readOnly}
      basicSetup={{ lineNumbers: true, foldGutter: true, highlightActiveLine: !readOnly }}
      onChange={onChange}
      onCreateEditor={(view) => {
        if (!jumpToLine) return;
        // Scroll the cited line into the middle of the viewport.
        const line = view.state.doc.line(
          Math.min(Math.max(jumpToLine, 1), view.state.doc.lines),
        );
        view.dispatch({
          selection: { anchor: line.from },
          effects: EditorView.scrollIntoView(line.from, { y: "center" }),
        });
      }}
      onKeyDown={(e) => {
        if (e.key === "s" && (e.metaKey || e.ctrlKey)) {
          e.preventDefault();
          onSave?.();
        }
      }}
    />
  );
}
