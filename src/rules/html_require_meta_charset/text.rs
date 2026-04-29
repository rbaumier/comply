//! html-require-meta-charset — scans HTML documents for a missing character
//! encoding declaration. A file is considered an HTML document when it
//! contains an `<html` opening tag. If neither `<meta charset` nor the
//! legacy `<meta http-equiv="Content-Type"` is present, a single diagnostic
//! is emitted on the line of the `<html` tag.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["<html"])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let source = ctx.source;
        let Some(html_offset) = source.find("<html") else {
            return Vec::new();
        };
        if source.contains("<meta charset") || source.contains("<meta http-equiv=\"Content-Type\"")
        {
            return Vec::new();
        }

        // 1-based line number of the `<html` tag.
        let line = source[..html_offset].bytes().filter(|b| *b == b'\n').count() + 1;

        vec![Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line,
            column: 1,
            rule_id: super::META.id.into(),
            message: "HTML document is missing a <meta charset> declaration in <head>.".into(),
            severity: Severity::Warning,
            span: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(path: &str, src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), src))
    }

    #[test]
    fn flags_html_missing_meta_charset() {
        let src = "<!DOCTYPE html>\n<html>\n  <head><title>x</title></head>\n  <body></body>\n</html>\n";
        let diags = run("index.html", src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_html_with_meta_charset() {
        let src = "<!DOCTYPE html>\n<html>\n  <head>\n    <meta charset=\"utf-8\">\n    <title>x</title>\n  </head>\n  <body></body>\n</html>\n";
        assert!(run("index.html", src).is_empty());
    }

    #[test]
    fn allows_html_with_legacy_http_equiv_content_type() {
        let src = "<!DOCTYPE html>\n<html>\n  <head>\n    <meta http-equiv=\"Content-Type\" content=\"text/html; charset=utf-8\">\n    <title>x</title>\n  </head>\n</html>\n";
        assert!(run("index.html", src).is_empty());
    }

    #[test]
    fn skips_non_html_files() {
        let src = "export const x = 1;\n";
        assert!(run("Comp.vue", src).is_empty());
    }
}
