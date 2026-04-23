//! html-no-script-style-type — flags redundant `type` attributes on
//! `<script>` and `<style>` tags. HTML5 defaults to `text/javascript` for
//! scripts and `text/css` for styles, so these values add no information.
//! `type="module"` and other non-default values are left alone.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

const PATTERNS: &[&str] = &[
    "<script type=\"text/javascript\"",
    "<script type='text/javascript'",
    "<style type=\"text/css\"",
    "<style type='text/css'",
];

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            for pattern in PATTERNS {
                let mut search_from = 0;
                while let Some(rel) = line[search_from..].find(pattern) {
                    let match_start = search_from + rel;
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: i + 1,
                        column: match_start + 1,
                        rule_id: super::META.id.into(),
                        message: "Redundant `type` attribute: this is the default and can be omitted.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                    search_from = match_start + pattern.len();
                }
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("Comp.vue"), src))
    }

    #[test]
    fn flags_script_text_javascript() {
        let src = "<script type=\"text/javascript\">var x = 1;</script>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_style_text_css() {
        let src = "<style type=\"text/css\">.a { color: red; }</style>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_script_without_type() {
        let src = "<script>var x = 1;</script>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_style_without_type() {
        let src = "<style>.a { color: red; }</style>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_script_type_module() {
        let src = "<script type=\"module\">import x from './x.js';</script>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_single_quoted_script() {
        let src = "<script type='text/javascript'>var x = 1;</script>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_single_quoted_style() {
        let src = "<style type='text/css'>.a {}</style>";
        assert_eq!(run(src).len(), 1);
    }
}
