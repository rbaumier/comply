//! html-require-doctype — text backend.
//!
//! Only runs on files whose extension is `.html`. Reads the source and
//! verifies the first non-whitespace characters form `<!doctype`
//! (case-insensitive). Any other leading content — including raw HTML tags,
//! BOM-less empty content, or comments — is flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_html_file(ctx.path) {
            return Vec::new();
        }
        if starts_with_doctype(ctx.source) {
            return Vec::new();
        }
        vec![Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: "html-require-doctype".into(),
            message: "HTML file is missing a `<!DOCTYPE html>` declaration.".into(),
            severity: Severity::Warning,
            span: None,
        }]
    }
}

fn is_html_file(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("html"))
}

/// True if the first non-whitespace bytes of `source` are `<!doctype`
/// (case-insensitive). Returns `false` for empty or whitespace-only files.
fn starts_with_doctype(source: &str) -> bool {
    let trimmed = source.trim_start();
    if trimmed.len() < 9 {
        return false;
    }
    trimmed[..9].eq_ignore_ascii_case("<!doctype")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(name: &str, source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(name), source))
    }

    #[test]
    fn flags_missing_doctype() {
        let source = "<html>\n<head></head>\n<body></body>\n</html>";
        let diags = run("index.html", source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("DOCTYPE"));
    }

    #[test]
    fn allows_uppercase_doctype() {
        let source = "<!DOCTYPE html>\n<html></html>";
        assert!(run("index.html", source).is_empty());
    }

    #[test]
    fn allows_lowercase_doctype() {
        let source = "<!doctype html>\n<html></html>";
        assert!(run("index.html", source).is_empty());
    }

    #[test]
    fn allows_mixed_case_doctype() {
        let source = "<!DocType html>\n<html></html>";
        assert!(run("index.html", source).is_empty());
    }

    #[test]
    fn allows_leading_whitespace_before_doctype() {
        let source = "   \n\t<!DOCTYPE html>\n<html></html>";
        assert!(run("index.html", source).is_empty());
    }

    #[test]
    fn ignores_non_html_file() {
        // The rule should not fire on files that aren't `.html`.
        let source = "<html></html>";
        assert!(run("component.vue", source).is_empty());
        assert!(run("index.tsx", source).is_empty());
    }

    #[test]
    fn flags_empty_html_file() {
        assert_eq!(run("empty.html", "").len(), 1);
    }

    #[test]
    fn flags_leading_comment_before_doctype() {
        let source = "<!-- comment -->\n<!DOCTYPE html>";
        assert_eq!(run("index.html", source).len(), 1);
    }
}
