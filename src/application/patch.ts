// Deterministic snippet→file merge used by the "Apply" flow in chat.
// The result is only a starting point: the user always reviews and can edit
// the merged text in a diff view before anything is written to disk.

export type MergeOutcome =
  /** The file already contains the snippet verbatim. */
  | { kind: "already" }
  /** A contiguous region was replaced automatically. */
  | { kind: "merged"; updated: string; strategy: "anchor" | "citation" }
  /** No safe automatic merge — the user edits the diff by hand. */
  | { kind: "manual" };

/** Indices of lines in `lines` that match `target`, exact or trimmed. */
function matches(lines: string[], target: string, from: number): number[] {
  const exact: number[] = [];
  const trimmed: number[] = [];
  const want = target.trim();
  for (let i = from; i < lines.length; i++) {
    if (lines[i] === target) exact.push(i);
    else if (lines[i].trim() === want) trimmed.push(i);
  }
  return exact.length > 0 ? exact : trimmed;
}

/**
 * Try to merge a chat-proposed snippet into a file.
 *
 * Strategy order:
 * 1. `already` — the snippet is present verbatim.
 * 2. `anchor` — the snippet's first and last non-blank lines each match a
 *    unique line in the file; the region between them is replaced. This is
 *    why the system prompt asks the model to include unchanged surrounding
 *    lines in proposed blocks.
 * 3. `citation` — fall back to replacing the cited line range, if any.
 */
export function mergeSnippet(
  file: string,
  snippet: string,
  citation?: { start_line: number; end_line: number },
): MergeOutcome {
  const snippetTrimmed = snippet.replace(/\n+$/, "");
  if (snippetTrimmed.length === 0) return { kind: "manual" };
  if (file.includes(snippetTrimmed)) return { kind: "already" };

  const fileLines = file.split("\n");
  const snippetLines = snippetTrimmed.split("\n");
  let first = 0;
  while (first < snippetLines.length && snippetLines[first].trim() === "") first++;
  let last = snippetLines.length - 1;
  while (last > first && snippetLines[last].trim() === "") last--;
  const body = snippetLines.slice(first, last + 1);

  // Closing lines like `}` are rarely unique, so one unique anchor (either
  // end) is enough; among candidates for the other end we take the region
  // whose length is closest to the snippet's — the user reviews the diff
  // before anything is written, so a wrong guess is visible, not silent.
  const pickClosest = (candidates: number[], anchor: number, span: number) => {
    let best: number | null = null;
    for (const c of candidates) {
      if (
        best === null ||
        Math.abs(Math.abs(c - anchor) + 1 - span) <
          Math.abs(Math.abs(best - anchor) + 1 - span)
      ) {
        best = c;
      }
    }
    return best;
  };
  const starts = matches(fileLines, body[0], 0);
  const allEnds = matches(fileLines, body[body.length - 1], 0);
  let start: number | null = null;
  let end: number | null = null;
  if (body.length === 1) {
    if (starts.length === 1) [start, end] = [starts[0], starts[0]];
  } else if (starts.length === 1) {
    start = starts[0];
    end = pickClosest(allEnds.filter((e) => e > start!), start, body.length);
  } else if (allEnds.length === 1) {
    end = allEnds[0];
    start = pickClosest(starts.filter((s) => s < end!), end, body.length);
  }
  if (start !== null && end !== null) {
    const updated = [
      ...fileLines.slice(0, start),
      ...body,
      ...fileLines.slice(end + 1),
    ].join("\n");
    return { kind: "merged", updated, strategy: "anchor" };
  }

  if (
    citation &&
    citation.start_line >= 1 &&
    citation.end_line >= citation.start_line &&
    citation.end_line <= fileLines.length
  ) {
    const updated = [
      ...fileLines.slice(0, citation.start_line - 1),
      ...body,
      ...fileLines.slice(citation.end_line),
    ].join("\n");
    return { kind: "merged", updated, strategy: "citation" };
  }

  return { kind: "manual" };
}

/** Parse `path=some/file.rs` from a code fence meta string, if present. */
export function pathFromFenceMeta(meta: string | undefined): string | null {
  if (!meta) return null;
  const match = meta.match(/(?:^|\s)path=(\S+)/);
  return match ? match[1].replace(/^["'`]|["'`]$/g, "") : null;
}
