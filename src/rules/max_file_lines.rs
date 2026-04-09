//! max-file-lines — flags files exceeding 200 lines.
//!
//! Why: large files accumulate mixed responsibilities. Splitting by
//! responsibility keeps modules focused and reviewable.

use crate::diagnostic::{Diagnostic, Severity};
use crate::files::Language;
use crate::rules::Rule;
use std::path::Path;

const MAX_LINES: usize = 200;

pub struct MaxFileLines;

impl Rule for MaxFileLines {
    fn id(&self) -> &'static str {
        "max-file-lines"
    }

    fn languages(&self) -> &[Language] {
        &[
            Language::TypeScript,
            Language::Tsx,
            Language::JavaScript,
            Language::Rust,
        ]
    }

    fn check(&self, path: &Path, source: &str, _language: Language) -> Vec<Diagnostic> {
        let count = source.lines().count();
        if count > MAX_LINES {
            vec![Diagnostic {
                path: path.to_path_buf(),
                line: MAX_LINES + 1,
                column: 1,
                rule_id: self.id().into(),
                message: format!(
                    "File has {count} lines — split by responsibility (max {MAX_LINES}). \
                     Extract helpers below line {MAX_LINES} into a separate module."
                ),
                severity: Severity::Error,
            }]
        } else {
            vec![]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn flags_file_over_limit() {
        let source = "x\n".repeat(MAX_LINES + 5);
        let diags = MaxFileLines.check(Path::new("foo.ts"), &source, Language::TypeScript);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "max-file-lines");
    }

    #[test]
    fn allows_file_at_limit() {
        let source = "x\n".repeat(MAX_LINES);
        let diags = MaxFileLines.check(Path::new("foo.ts"), &source, Language::TypeScript);
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_file_under_limit() {
        let source = "x\n".repeat(50);
        let diags = MaxFileLines.check(Path::new("foo.ts"), &source, Language::TypeScript);
        assert!(diags.is_empty());
    }
}
