use regex::Regex;

use crate::domain::services::{SecretFinding, SecretScanner};

/// Regex-based credential detector applied to every file before indexing.
/// Matches are replaced with `[REDACTED:<rule>]` so secrets never reach the
/// index, the LLM prompt, or any external provider.
pub struct RegexSecretScanner {
    rules: Vec<(String, Regex)>,
}

impl RegexSecretScanner {
    pub fn new() -> Self {
        let patterns: &[(&str, &str)] = &[
            ("aws-access-key", r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b"),
            ("github-token", r"\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{36,}\b"),
            ("github-pat", r"\bgithub_pat_[A-Za-z0-9_]{22,}\b"),
            ("openai-key", r"\bsk-[A-Za-z0-9_-]{20,}\b"),
            ("anthropic-key", r"\bsk-ant-[A-Za-z0-9_-]{20,}\b"),
            ("slack-token", r"\bxox[baprs]-[A-Za-z0-9-]{10,}\b"),
            ("google-api-key", r"\bAIza[0-9A-Za-z_-]{35}\b"),
            (
                "private-key-block",
                r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----",
            ),
            (
                "generic-assignment",
                r#"(?i)\b(?:api[_-]?key|api[_-]?secret|auth[_-]?token|access[_-]?token|client[_-]?secret|password|passwd)\b\s*[:=]\s*["'][^"'\s]{8,}["']"#,
            ),
            ("jwt", r"\beyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\b"),
        ];
        let rules = patterns
            .iter()
            .map(|(name, pattern)| {
                (
                    name.to_string(),
                    Regex::new(pattern).expect("secret pattern must compile"),
                )
            })
            .collect();
        Self { rules }
    }
}

impl Default for RegexSecretScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretScanner for RegexSecretScanner {
    fn scan(&self, content: &str) -> Vec<SecretFinding> {
        let mut findings = Vec::new();
        for (rule, regex) in &self.rules {
            for m in regex.find_iter(content) {
                let line = content[..m.start()].matches('\n').count() + 1;
                findings.push(SecretFinding {
                    rule: rule.clone(),
                    line,
                });
            }
        }
        findings
    }

    fn redact(&self, content: &str) -> String {
        let mut result = content.to_string();
        for (rule, regex) in &self.rules {
            result = regex
                .replace_all(&result, format!("[REDACTED:{rule}]"))
                .into_owned();
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_and_redacts_aws_key() {
        let scanner = RegexSecretScanner::new();
        let content = "aws_key = AKIAIOSFODNN7EXAMPLE\nok line";
        let findings = scanner.scan(content);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, "aws-access-key");
        assert_eq!(findings[0].line, 1);
        let redacted = scanner.redact(content);
        assert!(!redacted.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(redacted.contains("[REDACTED:aws-access-key]"));
    }

    #[test]
    fn detects_private_key_block() {
        let scanner = RegexSecretScanner::new();
        let content = "-----BEGIN RSA PRIVATE KEY-----\nMIIabc\n-----END RSA PRIVATE KEY-----";
        assert!(!scanner.scan(content).is_empty());
        assert!(!scanner.redact(content).contains("MIIabc"));
    }

    #[test]
    fn detects_generic_password_assignment() {
        let scanner = RegexSecretScanner::new();
        let content = r#"password = "hunter2hunter2""#;
        assert!(!scanner.scan(content).is_empty());
    }

    #[test]
    fn clean_code_passes_untouched() {
        let scanner = RegexSecretScanner::new();
        let content = "fn main() { println!(\"hello\"); }";
        assert!(scanner.scan(content).is_empty());
        assert_eq!(scanner.redact(content), content);
    }
}
