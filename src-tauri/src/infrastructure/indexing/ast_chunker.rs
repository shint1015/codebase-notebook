//! Syntax-aware chunking with tree-sitter: source files split along
//! top-level items (functions, classes, impl blocks) instead of arbitrary
//! line windows, so retrieved chunks are whole units of meaning.

use tree_sitter::Parser;

use crate::domain::chunking::{chunk_text, TextChunk};
use crate::domain::services::Chunker;

const MAX_CHUNK_LINES: usize = 60;
const OVERLAP_LINES: usize = 8;
/// A single item larger than this is split internally with line windows.
const HUGE_NODE_LINES: usize = 120;

pub struct SmartChunker;

fn grammar_for(language: &str) -> Option<tree_sitter::Language> {
    match language {
        "rust" => Some(tree_sitter_rust::LANGUAGE.into()),
        "typescript" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        "javascript" => Some(tree_sitter_javascript::LANGUAGE.into()),
        "python" => Some(tree_sitter_python::LANGUAGE.into()),
        "go" => Some(tree_sitter_go::LANGUAGE.into()),
        _ => None,
    }
}

fn slice_lines(lines: &[&str], start: usize, end: usize) -> TextChunk {
    TextChunk {
        content: lines[start..=end].join("\n"),
        start_line: start + 1,
        end_line: end + 1,
    }
}

fn chunk_ast(content: &str, grammar: tree_sitter::Language) -> Option<Vec<TextChunk>> {
    let mut parser = Parser::new();
    parser.set_language(&grammar).ok()?;
    let tree = parser.parse(content, None)?;
    let root = tree.root_node();
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return Some(Vec::new());
    }

    let mut chunks: Vec<TextChunk> = Vec::new();
    let mut pending: Option<(usize, usize)> = None;
    let mut flush = |pending: &mut Option<(usize, usize)>, chunks: &mut Vec<TextChunk>| {
        if let Some((start, end)) = pending.take() {
            let chunk = slice_lines(&lines, start, end.min(lines.len() - 1));
            if !chunk.content.trim().is_empty() {
                chunks.push(chunk);
            }
        }
    };

    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        let start = child.start_position().row;
        let end = child.end_position().row.min(lines.len().saturating_sub(1));

        // Oversized items (a 500-line function) get internal line windows.
        if end - start + 1 > HUGE_NODE_LINES {
            flush(&mut pending, &mut chunks);
            let body = lines[start..=end].join("\n");
            for sub in chunk_text(&body, MAX_CHUNK_LINES, OVERLAP_LINES) {
                chunks.push(TextChunk {
                    content: sub.content,
                    start_line: start + sub.start_line,
                    end_line: start + sub.end_line,
                });
            }
            continue;
        }

        match pending {
            None => pending = Some((start, end)),
            Some((current_start, current_end)) => {
                // Merge small neighbouring items until the window fills up.
                if end.saturating_sub(current_start) + 1 <= MAX_CHUNK_LINES {
                    pending = Some((current_start, current_end.max(end)));
                } else {
                    flush(&mut pending, &mut chunks);
                    pending = Some((start, end));
                }
            }
        }
    }
    flush(&mut pending, &mut chunks);

    if chunks.is_empty() {
        None // e.g. parser produced nothing useful; caller falls back
    } else {
        Some(chunks)
    }
}

impl Chunker for SmartChunker {
    fn chunk(&self, content: &str, language: &str) -> Vec<TextChunk> {
        if let Some(grammar) = grammar_for(language) {
            if let Some(chunks) = chunk_ast(content, grammar) {
                return chunks;
            }
        }
        chunk_text(content, MAX_CHUNK_LINES, OVERLAP_LINES)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_functions_stay_whole() {
        let code = r#"use std::fmt;

fn alpha() -> i32 {
    let x = 1;
    x + 1
}

fn beta() {
    println!("beta");
}
"#;
        let chunks = SmartChunker.chunk(code, "rust");
        // Small file: items merge into one chunk covering everything.
        assert!(!chunks.is_empty());
        let joined: String = chunks.iter().map(|c| c.content.clone()).collect();
        assert!(joined.contains("fn alpha"));
        assert!(joined.contains("fn beta"));
        // No chunk starts mid-function body.
        for chunk in &chunks {
            assert!(!chunk.content.trim_start().starts_with("x + 1"));
        }
    }

    #[test]
    fn large_items_split_at_boundaries() {
        // 30 functions of 5 lines: chunks must break between functions.
        let mut code = String::new();
        for i in 0..30 {
            code.push_str(&format!(
                "fn func_{i}() -> i32 {{\n    let value = {i};\n    value * 2\n}}\n\n"
            ));
        }
        let chunks = SmartChunker.chunk(&code, "rust");
        assert!(chunks.len() >= 2);
        for chunk in &chunks {
            assert!(
                chunk.content.trim_start().starts_with("fn func_"),
                "chunk must start at a function boundary, got: {}",
                &chunk.content[..40.min(chunk.content.len())]
            );
        }
    }

    #[test]
    fn python_and_typescript_parse() {
        let py = "def hello():\n    return 1\n\nclass Thing:\n    pass\n";
        assert!(!SmartChunker.chunk(py, "python").is_empty());
        let ts = "export function greet(name: string): string {\n  return `hi ${name}`;\n}\n";
        assert!(!SmartChunker.chunk(ts, "typescript").is_empty());
    }

    #[test]
    fn unknown_language_falls_back_to_lines() {
        let text = (1..=100)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let chunks = SmartChunker.chunk(&text, "markdown");
        assert!(chunks.len() >= 2);
    }
}
