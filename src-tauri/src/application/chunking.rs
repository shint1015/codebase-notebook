//! Deterministic text chunking. Pure logic — no I/O, no external crates.
//!
//! MVP strategy: split on blank-line boundaries into windows of at most
//! `max_lines` lines, carrying `overlap` lines between adjacent chunks so
//! context is not lost at boundaries. AST-based chunking (tree-sitter) can
//! replace this later without touching callers.

#[derive(Debug, Clone, PartialEq)]
pub struct TextChunk {
    pub content: String,
    /// 1-based inclusive line range in the original file.
    pub start_line: usize,
    pub end_line: usize,
}

pub fn chunk_text(content: &str, max_lines: usize, overlap: usize) -> Vec<TextChunk> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return Vec::new();
    }
    let max_lines = max_lines.max(1);
    let overlap = overlap.min(max_lines - 1);

    let mut chunks = Vec::new();
    let mut start = 0usize;
    while start < lines.len() {
        let hard_end = (start + max_lines).min(lines.len());
        // Prefer to break at a blank line near the end of the window.
        let end = if hard_end < lines.len() {
            let window_floor = start + max_lines / 2;
            (window_floor..hard_end)
                .rev()
                .find(|&i| lines[i].trim().is_empty())
                .map(|i| i + 1)
                .unwrap_or(hard_end)
        } else {
            hard_end
        };

        let body = lines[start..end].join("\n");
        if !body.trim().is_empty() {
            chunks.push(TextChunk {
                content: body,
                start_line: start + 1,
                end_line: end,
            });
        }
        if end >= lines.len() {
            break;
        }
        start = end.saturating_sub(overlap).max(start + 1);
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_yields_no_chunks() {
        assert!(chunk_text("", 40, 5).is_empty());
    }

    #[test]
    fn short_input_is_single_chunk() {
        let chunks = chunk_text("a\nb\nc", 40, 5);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].start_line, 1);
        assert_eq!(chunks[0].end_line, 3);
    }

    #[test]
    fn long_input_is_split_with_overlap() {
        let text = (1..=100)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let chunks = chunk_text(&text, 30, 5);
        assert!(chunks.len() >= 3);
        // Every line must be covered.
        assert_eq!(chunks.first().unwrap().start_line, 1);
        assert_eq!(chunks.last().unwrap().end_line, 100);
        // Adjacent chunks overlap.
        assert!(chunks[1].start_line <= chunks[0].end_line);
    }

    #[test]
    fn prefers_blank_line_boundaries() {
        let mut lines: Vec<String> = (1..=40).map(|i| format!("line {i}")).collect();
        lines[24] = String::new(); // blank line at line 25
        let text = lines.join("\n");
        let chunks = chunk_text(&text, 30, 0);
        assert_eq!(chunks[0].end_line, 25);
    }
}
