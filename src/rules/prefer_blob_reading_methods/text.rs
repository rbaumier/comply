use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const METHODS: &[(&str, &str)] = &[
    (".readAsText(", "text"),
    (".readAsArrayBuffer(", "arrayBuffer"),
];

/// Detect `FileReader#readAsText(…)` and `FileReader#readAsArrayBuffer(…)`.
fn find_file_reader_method(line: &str) -> Option<(&'static str, &'static str)> {
    for &(pattern, replacement) in METHODS {
        if line.contains(pattern) {
            let method_name = pattern.trim_start_matches('.').trim_end_matches('(');
            return Some((method_name, replacement));
        }
    }
    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some((method, replacement)) = find_file_reader_method(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "prefer-blob-reading-methods".into(),
                    message: format!(
                        "Prefer `Blob#{}()` over `FileReader#{}(blob)`.",
                        replacement, method
                    ),
                    severity: Severity::Warning,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_read_as_text() {
        let code = "reader.readAsText(blob);";
        let d = run(code);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Blob#text()"));
    }

    #[test]
    fn flags_read_as_array_buffer() {
        let code = "reader.readAsArrayBuffer(blob);";
        let d = run(code);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Blob#arrayBuffer()"));
    }

    #[test]
    fn allows_blob_text() {
        let code = "const text = await blob.text();";
        assert!(run(code).is_empty());
    }

    #[test]
    fn allows_blob_array_buffer() {
        let code = "const buf = await blob.arrayBuffer();";
        assert!(run(code).is_empty());
    }

    #[test]
    fn allows_unrelated_code() {
        let code = "const data = JSON.parse(response);";
        assert!(run(code).is_empty());
    }
}
