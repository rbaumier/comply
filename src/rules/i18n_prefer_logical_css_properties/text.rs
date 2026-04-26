//! i18n-prefer-logical-css-properties text backend.
//!
//! CSS-in-JS (styled-components, emotion, vanilla-extract) and
//! template-literal style blocks frequently hard-code physical
//! directions (`margin-left`, `padding-right`, `text-align: left`).
//! These mirror incorrectly under RTL locales. The rule scans for the
//! common offenders and nudges authors toward logical equivalents
//! (`margin-inline-start`, `text-align: start`, …). Bare `left:` /
//! `right:` are intentionally not flagged because they are too easily
//! confused with JS identifiers / object keys — flagging them would
//! generate noise.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Pairs of `(needle, message)` that indicate a physical property.
const PATTERNS: &[(&str, &str)] = &[
    (
        "margin-left:",
        "Use `margin-inline-start` instead of `margin-left` for RTL-safe layouts.",
    ),
    (
        "margin-right:",
        "Use `margin-inline-end` instead of `margin-right` for RTL-safe layouts.",
    ),
    (
        "padding-left:",
        "Use `padding-inline-start` instead of `padding-left` for RTL-safe layouts.",
    ),
    (
        "padding-right:",
        "Use `padding-inline-end` instead of `padding-right` for RTL-safe layouts.",
    ),
    (
        "border-left:",
        "Use `border-inline-start` instead of `border-left` for RTL-safe layouts.",
    ),
    (
        "border-right:",
        "Use `border-inline-end` instead of `border-right` for RTL-safe layouts.",
    ),
    (
        "text-align: left",
        "Use `text-align: start` instead of `text-align: left` for RTL-safe layouts.",
    ),
    (
        "text-align:left",
        "Use `text-align: start` instead of `text-align: left` for RTL-safe layouts.",
    ),
    (
        "text-align: right",
        "Use `text-align: end` instead of `text-align: right` for RTL-safe layouts.",
    ),
    (
        "text-align:right",
        "Use `text-align: end` instead of `text-align: right` for RTL-safe layouts.",
    ),
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }
            for (needle, message) in PATTERNS {
                if let Some(col) = line.find(needle) {
                    diags.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: i + 1,
                        column: col + 1,
                        rule_id: super::META.id.into(),
                        message: (*message).into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                    // Only emit one diagnostic per line to avoid
                    // double-reporting when `text-align:left` matches
                    // two overlapping patterns.
                    break;
                }
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.tsx"), src))
    }

    #[test]
    fn flags_margin_left() {
        assert_eq!(run("const s = css`margin-left: 10px;`").len(), 1);
    }

    #[test]
    fn flags_text_align_left() {
        assert_eq!(run("  text-align: left;").len(), 1);
    }

    #[test]
    fn flags_border_right() {
        assert_eq!(run("border-right: 1px solid;").len(), 1);
    }

    #[test]
    fn allows_logical_margin() {
        assert!(run("margin-inline-start: 10px;").is_empty());
    }

    #[test]
    fn allows_logical_text_align() {
        assert!(run("text-align: start;").is_empty());
    }

    #[test]
    fn allows_commented_line() {
        assert!(run("// margin-left: 10px;").is_empty());
    }
}
