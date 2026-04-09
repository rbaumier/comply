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
        &[Language::TypeScript, Language::Rust]
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
