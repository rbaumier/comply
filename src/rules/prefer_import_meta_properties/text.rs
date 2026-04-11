//! prefer-import-meta-properties — flag `fileURLToPath(import.meta.url)` patterns.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// Detect `fileURLToPath(import.meta.url)` — should be `import.meta.filename`.
fn find_file_url_to_path(line: &str) -> Vec<usize> {
    let needle = "fileURLToPath(import.meta.url)";
    let mut hits = Vec::new();
    let mut start = 0;
    while let Some(pos) = line[start..].find(needle) {
        let abs = start + pos;
        // Ensure not part of a larger identifier
        if abs == 0 || !is_ident_char(line.as_bytes()[abs - 1]) {
            hits.push(abs);
        }
        start = abs + needle.len();
    }
    hits
}

/// Detect `dirname(fileURLToPath(import.meta.url))` — should be `import.meta.dirname`.
fn find_dirname_pattern(line: &str) -> Vec<usize> {
    let needle = "dirname(fileURLToPath(import.meta.url))";
    let mut hits = Vec::new();
    let mut start = 0;
    while let Some(pos) = line[start..].find(needle) {
        let abs = start + pos;
        hits.push(abs);
        start = abs + needle.len();
    }
    hits
}

/// Detect `path.dirname(fileURLToPath(import.meta.url))` variant.
fn find_path_dirname_pattern(line: &str) -> Vec<usize> {
    let needle = "path.dirname(fileURLToPath(import.meta.url))";
    let mut hits = Vec::new();
    let mut start = 0;
    while let Some(pos) = line[start..].find(needle) {
        let abs = start + pos;
        hits.push(abs);
        start = abs + needle.len();
    }
    hits
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            // Check for dirname patterns first (they're more specific and
            // subsume the filename pattern).
            let mut dirname_ranges: Vec<(usize, usize)> = Vec::new();

            for col in find_path_dirname_pattern(line) {
                let end = col + "path.dirname(fileURLToPath(import.meta.url))".len();
                dirname_ranges.push((col, end));
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "prefer-import-meta-properties".into(),
                    message: "Use `import.meta.dirname` instead of `path.dirname(fileURLToPath(import.meta.url))`.".into(),
                    severity: Severity::Warning,
                });
            }

            for col in find_dirname_pattern(line) {
                let end = col + "dirname(fileURLToPath(import.meta.url))".len();
                // Skip if already covered by a path.dirname match
                if dirname_ranges.iter().any(|&(s, e)| col >= s && end <= e) {
                    continue;
                }
                dirname_ranges.push((col, end));
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "prefer-import-meta-properties".into(),
                    message: "Use `import.meta.dirname` instead of `dirname(fileURLToPath(import.meta.url))`.".into(),
                    severity: Severity::Warning,
                });
            }

            for col in find_file_url_to_path(line) {
                let end = col + "fileURLToPath(import.meta.url)".len();
                // Skip if already covered by a dirname match
                if dirname_ranges.iter().any(|&(s, e)| col >= s && end <= e) {
                    continue;
                }
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "prefer-import-meta-properties".into(),
                    message:
                        "Use `import.meta.filename` instead of `fileURLToPath(import.meta.url)`."
                            .into(),
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
    fn flags_file_url_to_path() {
        let d = run("const file = fileURLToPath(import.meta.url);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("import.meta.filename"));
    }

    #[test]
    fn flags_dirname_pattern() {
        let d = run("const dir = dirname(fileURLToPath(import.meta.url));");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("import.meta.dirname"));
    }

    #[test]
    fn flags_path_dirname_pattern() {
        let d = run("const dir = path.dirname(fileURLToPath(import.meta.url));");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("import.meta.dirname"));
    }

    #[test]
    fn allows_import_meta_filename() {
        assert!(run("const file = import.meta.filename;").is_empty());
    }

    #[test]
    fn allows_import_meta_dirname() {
        assert!(run("const dir = import.meta.dirname;").is_empty());
    }

    #[test]
    fn no_duplicate_for_dirname_containing_file_url() {
        // dirname(fileURLToPath(...)) should emit ONE diagnostic (dirname), not two
        let d = run("const dir = dirname(fileURLToPath(import.meta.url));");
        assert_eq!(d.len(), 1);
    }
}
