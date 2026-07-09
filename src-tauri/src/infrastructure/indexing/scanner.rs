use std::path::Path;

use walkdir::WalkDir;

use super::extract;
use crate::domain::error::{DomainError, DomainResult};
use crate::domain::services::{SourceFile, SourceScanner};

/// Directories that never contain user knowledge worth indexing.
const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    "out",
    "vendor",
    ".venv",
    "venv",
    "__pycache__",
    ".next",
    ".cache",
    ".idea",
    ".vscode",
];

/// Files that must never be indexed because they typically hold credentials.
const SKIP_FILES: &[&str] = &[
    ".env",
    ".envrc",
    ".netrc",
    "id_rsa",
    "id_ed25519",
    "credentials",
    ".npmrc",
    ".pypirc",
];

const MAX_FILE_BYTES: u64 = 1_000_000;
/// Binary documents (Word/Excel/PDF) are allowed to be larger since only the
/// extracted text — not the file — enters the index.
const MAX_DOCUMENT_BYTES: u64 = 25_000_000;

/// Formats that need text extraction instead of a plain UTF-8 read.
#[derive(Clone, Copy)]
enum BinaryFormat {
    Word,
    Spreadsheet,
    Pdf,
}

const BINARY_EXTENSIONS: &[(&str, &str, BinaryFormat)] = &[
    ("docx", "word", BinaryFormat::Word),
    ("xlsx", "excel", BinaryFormat::Spreadsheet),
    ("xls", "excel", BinaryFormat::Spreadsheet),
    ("ods", "excel", BinaryFormat::Spreadsheet),
    ("pdf", "pdf", BinaryFormat::Pdf),
];

const EXTENSIONS: &[(&str, &str)] = &[
    ("md", "markdown"),
    ("markdown", "markdown"),
    ("txt", "text"),
    ("csv", "csv"),
    ("tsv", "csv"),
    ("rs", "rust"),
    ("ts", "typescript"),
    ("tsx", "typescript"),
    ("js", "javascript"),
    ("jsx", "javascript"),
    ("py", "python"),
    ("go", "go"),
    ("java", "java"),
    ("kt", "kotlin"),
    ("rb", "ruby"),
    ("php", "php"),
    ("c", "c"),
    ("h", "c"),
    ("cpp", "cpp"),
    ("hpp", "cpp"),
    ("cc", "cpp"),
    ("cs", "csharp"),
    ("swift", "swift"),
    ("sql", "sql"),
    ("sh", "shell"),
    ("bash", "shell"),
    ("yaml", "yaml"),
    ("yml", "yaml"),
    ("toml", "toml"),
    ("json", "json"),
    ("html", "html"),
    ("css", "css"),
    ("scss", "css"),
    ("proto", "protobuf"),
    ("graphql", "graphql"),
    ("dockerfile", "docker"),
];

pub struct FsSourceScanner;

impl FsSourceScanner {
    fn language_for(path: &Path) -> Option<&'static str> {
        let name = path.file_name()?.to_str()?.to_ascii_lowercase();
        if name == "dockerfile" || name == "makefile" {
            return Some("config");
        }
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        EXTENSIONS
            .iter()
            .find(|(e, _)| *e == ext)
            .map(|(_, lang)| *lang)
    }

    fn should_skip_file(path: &Path) -> bool {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            return true;
        };
        let lower = name.to_ascii_lowercase();
        // "~$" prefixed files are Office lock files.
        lower.starts_with("~$")
            || SKIP_FILES
                .iter()
                .any(|skip| lower == *skip || lower.starts_with(&format!("{skip}.")))
    }

    fn binary_format_for(path: &Path) -> Option<(&'static str, BinaryFormat)> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        BINARY_EXTENSIONS
            .iter()
            .find(|(e, _, _)| *e == ext)
            .map(|(_, lang, format)| (*lang, *format))
    }

    /// Read a file's indexable text: plain UTF-8 for source/text files,
    /// extracted text for Word/Excel/PDF. Returns None for files that should
    /// be skipped (unreadable, extraction failed, empty).
    fn read_content(path: &Path, size: u64) -> Option<(String, &'static str)> {
        if let Some((language, format)) = Self::binary_format_for(path) {
            if size > MAX_DOCUMENT_BYTES {
                return None;
            }
            let extracted = match format {
                BinaryFormat::Word => extract::extract_docx(path),
                BinaryFormat::Spreadsheet => extract::extract_spreadsheet(path),
                BinaryFormat::Pdf => extract::extract_pdf(path),
            };
            // Extraction failures (corrupt file, image-only PDF) skip the
            // file rather than aborting the whole indexing run.
            return extracted.ok().map(|content| (content, language));
        }
        let language = Self::language_for(path)?;
        if size > MAX_FILE_BYTES {
            return None;
        }
        let content = std::fs::read_to_string(path).ok()?; // non-UTF8 / unreadable
        Some((content, language))
    }
}

/// FNV-1a content hash — cheap and adequate for change detection.
fn content_hash(content: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in content.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

impl SourceScanner for FsSourceScanner {
    fn scan(&self, root_path: &str) -> DomainResult<Vec<SourceFile>> {
        let root = Path::new(root_path);
        // Single-file sources: the file itself is the whole tree.
        if root.is_file() {
            let size = root.metadata().map(|m| m.len()).unwrap_or(u64::MAX);
            let Some((content, language)) = Self::read_content(root, size) else {
                return Err(DomainError::Indexing(format!(
                    "unsupported or unreadable file: {root_path}"
                )));
            };
            if content.trim().is_empty() {
                return Ok(Vec::new());
            }
            let rel_path = root
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "file".to_string());
            return Ok(vec![SourceFile {
                rel_path,
                language: language.to_string(),
                content_hash: content_hash(&content),
                content,
            }]);
        }
        if !root.is_dir() {
            return Err(DomainError::Indexing(format!(
                "not a directory: {root_path}"
            )));
        }
        let mut files = Vec::new();
        let walker = WalkDir::new(root).follow_links(false).into_iter();
        for entry in walker.filter_entry(|e| {
            let name = e.file_name().to_str().unwrap_or("");
            !(e.file_type().is_dir() && (SKIP_DIRS.contains(&name) || name.starts_with('.')))
        }) {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            if Self::should_skip_file(path) {
                continue;
            }
            let size = entry.metadata().map(|m| m.len()).unwrap_or(u64::MAX);
            let Some((content, language)) = Self::read_content(path, size) else {
                continue;
            };
            if content.trim().is_empty() {
                continue;
            }
            let rel_path = path
                .strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            files.push(SourceFile {
                rel_path,
                language: language.to_string(),
                content_hash: content_hash(&content),
                content,
            });
        }
        files.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
        Ok(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, rel: &str, content: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn scans_supported_files_and_skips_dangerous_ones() {
        let dir = std::env::temp_dir().join(format!("cbnb-scan-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        write(&dir, "README.md", "# hello");
        write(&dir, "src/main.rs", "fn main() {}");
        write(&dir, ".env", "SECRET=x");
        write(&dir, "node_modules/pkg/index.js", "console.log(1)");
        write(&dir, "image.bin", "binary");

        let files = FsSourceScanner.scan(dir.to_str().unwrap()).unwrap();
        let paths: Vec<&str> = files.iter().map(|f| f.rel_path.as_str()).collect();
        assert_eq!(paths, vec!["README.md", "src/main.rs"]);
        assert_eq!(files[1].language, "rust");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn hash_changes_with_content() {
        assert_ne!(content_hash("a"), content_hash("b"));
        assert_eq!(content_hash("same"), content_hash("same"));
    }

    #[test]
    fn scans_docx_documents_as_extracted_text() {
        use std::io::Write as _;

        let dir = std::env::temp_dir().join(format!("cbnb-scan-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("docs")).unwrap();

        let file = std::fs::File::create(dir.join("docs/spec.docx")).unwrap();
        let mut zip_writer = zip::ZipWriter::new(file);
        zip_writer
            .start_file("word/document.xml", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip_writer
            .write_all(
                br#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Payment spec body</w:t></w:r></w:p></w:body></w:document>"#,
            )
            .unwrap();
        zip_writer.finish().unwrap();
        // Office lock files must be ignored.
        std::fs::write(dir.join("docs/~$spec.docx"), "lock").unwrap();

        let files = FsSourceScanner.scan(dir.to_str().unwrap()).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].rel_path, "docs/spec.docx");
        assert_eq!(files[0].language, "word");
        assert!(files[0].content.contains("Payment spec body"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
