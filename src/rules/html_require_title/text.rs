//! html-require-title — flags files containing an `<html` opening tag that
//! lack a `<title` element. Non-HTML documents (no `<html`) are skipped so
//! the rule stays silent on templates/fragments that aren't full documents.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.contains("<html") {
            return Vec::new();
        }
        if src.contains("<title") {
            return Vec::new();
        }
        vec![Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: super::META.id.into(),
            message: "HTML document is missing a `<title>` element.".into(),
            severity: Severity::Warning,
            span: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("index.html"), src))
    }

    #[test]
    fn flags_html_without_title() {
        let src = "<!doctype html><html><head></head><body></body></html>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_html_with_title() {
        let src = "<!doctype html><html><head><title>Hi</title></head><body></body></html>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_non_html_fragment() {
        let src = "<template><div>no html root here</div></template>";
        assert!(run(src).is_empty());
    }
}
