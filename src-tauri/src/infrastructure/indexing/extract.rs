//! Text extraction for binary document formats (Word / Excel / PDF).
//! Extracted text flows through the same chunking, secret-redaction and
//! search pipeline as plain source files.

use std::io::Read;
use std::path::Path;

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::domain::error::{DomainError, DomainResult};

/// Cap extracted text so a huge spreadsheet cannot blow up the index.
const MAX_EXTRACTED_CHARS: usize = 2_000_000;

fn cap(mut text: String) -> String {
    if text.chars().count() > MAX_EXTRACTED_CHARS {
        text = text.chars().take(MAX_EXTRACTED_CHARS).collect();
        text.push_str("\n[truncated]");
    }
    text
}

/// .docx: a zip whose word/document.xml holds paragraphs (`w:p`) of text
/// runs (`w:t`). Tables and headers come along as plain text lines.
pub fn extract_docx(path: &Path) -> DomainResult<String> {
    let file = std::fs::File::open(path)
        .map_err(|e| DomainError::Indexing(format!("open docx: {e}")))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| DomainError::Indexing(format!("read docx archive: {e}")))?;
    let mut xml = String::new();
    archive
        .by_name("word/document.xml")
        .map_err(|e| DomainError::Indexing(format!("docx has no document.xml: {e}")))?
        .read_to_string(&mut xml)
        .map_err(|e| DomainError::Indexing(format!("read document.xml: {e}")))?;

    let mut reader = Reader::from_str(&xml);
    let mut output = String::new();
    let mut in_text = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) if e.local_name().as_ref() == b"t" => in_text = true,
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"t" => in_text = false,
                b"p" => output.push('\n'),
                _ => {}
            },
            Ok(Event::Empty(e)) => match e.local_name().as_ref() {
                b"tab" => output.push('\t'),
                b"br" => output.push('\n'),
                _ => {}
            },
            Ok(Event::Text(t)) if in_text => {
                output.push_str(&t.unescape().unwrap_or_default());
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(DomainError::Indexing(format!("parse document.xml: {e}"))),
            _ => {}
        }
    }
    Ok(cap(output))
}

/// .xlsx / .xls / .ods via calamine: every sheet becomes a "## Sheet: name"
/// section with tab-separated rows, which chunks and searches well.
pub fn extract_spreadsheet(path: &Path) -> DomainResult<String> {
    use calamine::{open_workbook_auto, Reader as _};

    let mut workbook = open_workbook_auto(path)
        .map_err(|e| DomainError::Indexing(format!("open spreadsheet: {e}")))?;
    let mut output = String::new();
    for sheet_name in workbook.sheet_names().to_owned() {
        let Ok(range) = workbook.worksheet_range(&sheet_name) else {
            continue;
        };
        output.push_str(&format!("## Sheet: {sheet_name}\n"));
        for row in range.rows() {
            let line = row
                .iter()
                .map(|cell| cell.to_string())
                .collect::<Vec<_>>()
                .join("\t");
            if !line.trim().is_empty() {
                output.push_str(&line);
                output.push('\n');
            }
        }
        output.push('\n');
    }
    Ok(cap(output))
}

/// .pdf via pdf-extract. Some PDFs (scans without OCR, exotic encodings)
/// yield little or no text — those are skipped by the caller when empty.
pub fn extract_pdf(path: &Path) -> DomainResult<String> {
    // pdf-extract can panic on malformed files; contain it.
    let path = path.to_path_buf();
    let result = std::panic::catch_unwind(move || pdf_extract::extract_text(&path));
    match result {
        Ok(Ok(text)) => Ok(cap(text)),
        Ok(Err(e)) => Err(DomainError::Indexing(format!("extract pdf: {e}"))),
        Err(_) => Err(DomainError::Indexing("pdf parser crashed on this file".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_docx(path: &Path, body_xml: &str) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip_writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        zip_writer.start_file("word/document.xml", options).unwrap();
        zip_writer
            .write_all(
                format!(
                    r#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body_xml}</w:body></w:document>"#
                )
                .as_bytes(),
            )
            .unwrap();
        zip_writer.finish().unwrap();
    }

    #[test]
    fn extracts_paragraphs_from_docx() {
        let dir = std::env::temp_dir().join(format!("cbnb-docx-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("design.docx");
        write_docx(
            &path,
            "<w:p><w:r><w:t>Design Doc</w:t></w:r></w:p>\
             <w:p><w:r><w:t>The auth service issues</w:t></w:r><w:r><w:t xml:space=\"preserve\"> tokens.</w:t></w:r></w:p>",
        );
        let text = extract_docx(&path).unwrap();
        assert_eq!(text, "Design Doc\nThe auth service issues tokens.\n");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_non_docx_zip() {
        let dir = std::env::temp_dir().join(format!("cbnb-docx-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("not-a.docx");
        std::fs::write(&path, "plain text, not a zip").unwrap();
        assert!(extract_docx(&path).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
