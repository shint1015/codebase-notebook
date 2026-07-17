import { useEffect, useMemo, useRef, useState } from "react";

interface Props {
  value: string;
  placeholder: string;
  disabled?: boolean;
  /** Indexed paths in the workspace, offered after typing "@". */
  paths: string[];
  onChange: (value: string) => void;
  onSubmit: () => void;
}

/** The "@partial" the caret currently sits in, if any. */
function mentionAtCaret(value: string, caret: number) {
  const upto = value.slice(0, caret);
  const at = upto.lastIndexOf("@");
  if (at < 0) return null;
  // Must start a word and contain no whitespace since the '@'.
  if (at > 0 && !/\s/.test(upto[at - 1])) return null;
  const partial = upto.slice(at + 1);
  if (/\s/.test(partial)) return null;
  return { start: at, partial };
}

/**
 * Composer textarea with `@file` autocomplete: typing "@" suggests indexed
 * paths so the user can pin exact context instead of relying on search.
 */
export function MentionInput({
  value,
  placeholder,
  disabled,
  paths,
  onChange,
  onSubmit,
}: Props) {
  const ref = useRef<HTMLTextAreaElement>(null);
  const [mention, setMention] = useState<{ start: number; partial: string } | null>(null);
  const [active, setActive] = useState(0);

  const suggestions = useMemo(() => {
    if (!mention) return [];
    const q = mention.partial.toLowerCase();
    return paths
      .filter((p) => p.toLowerCase().includes(q))
      // Prefer matches on the file name over deep-path matches.
      .sort((a, b) => {
        const an = a.split("/").pop()!.toLowerCase().startsWith(q) ? 0 : 1;
        const bn = b.split("/").pop()!.toLowerCase().startsWith(q) ? 0 : 1;
        return an - bn || a.length - b.length;
      })
      .slice(0, 8);
  }, [mention, paths]);

  useEffect(() => setActive(0), [mention?.partial]);

  const sync = (next: string, caret: number) => {
    onChange(next);
    setMention(mentionAtCaret(next, caret));
  };

  const accept = (path: string) => {
    if (!mention) return;
    const caret = ref.current?.selectionStart ?? value.length;
    const next = `${value.slice(0, mention.start)}@${path} ${value.slice(caret)}`;
    onChange(next);
    setMention(null);
    // Put the caret right after the inserted mention.
    requestAnimationFrame(() => {
      const pos = mention.start + path.length + 2;
      ref.current?.focus();
      ref.current?.setSelectionRange(pos, pos);
    });
  };

  const open = mention !== null && suggestions.length > 0;

  return (
    <div className="mention-wrap">
      {open && (
        <ul className="mention-list">
          {suggestions.map((path, i) => (
            <li
              key={path}
              className={i === active ? "active" : ""}
              onMouseEnter={() => setActive(i)}
              onMouseDown={(e) => {
                e.preventDefault();
                accept(path);
              }}
            >
              {path}
            </li>
          ))}
        </ul>
      )}
      <textarea
        ref={ref}
        value={value}
        placeholder={placeholder}
        onChange={(e) => sync(e.target.value, e.target.selectionStart)}
        onClick={(e) => setMention(mentionAtCaret(value, e.currentTarget.selectionStart))}
        onBlur={() => setMention(null)}
        onKeyDown={(e) => {
          if (open) {
            if (e.key === "ArrowDown") {
              e.preventDefault();
              setActive((i) => Math.min(i + 1, suggestions.length - 1));
              return;
            }
            if (e.key === "ArrowUp") {
              e.preventDefault();
              setActive((i) => Math.max(i - 1, 0));
              return;
            }
            if (e.key === "Tab" || (e.key === "Enter" && !e.ctrlKey && !e.metaKey)) {
              e.preventDefault();
              accept(suggestions[active]);
              return;
            }
            if (e.key === "Escape") {
              setMention(null);
              return;
            }
          }
          if (
            e.key === "Enter" &&
            (e.ctrlKey || e.metaKey) &&
            !e.nativeEvent.isComposing
          ) {
            e.preventDefault();
            if (!disabled && value.trim()) onSubmit();
          }
        }}
        rows={2}
      />
    </div>
  );
}
