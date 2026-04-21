//! tailwind-no-apply-for-variants backend — flag `@apply` directives that
//! live outside `@layer base` / `@layer typography`. The rule scopes itself
//! to `.css` files; any other extension short-circuits so mentions of
//! `@apply` in JS/TS source (strings, comments) don't produce noise.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let ext = ctx.path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "css" {
            return vec![];
        }

        let mut diags = Vec::new();
        // Track the innermost enclosing `@layer base`/`typography` block via
        // its opening brace depth. Any `@apply` found while `layer_base_depth`
        // is `Some(_)` is considered inside that layer and skipped.
        let mut layer_base_depth: Option<usize> = None;
        let mut brace_depth: usize = 0;

        for (i, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            let opens_base_layer = t.starts_with("@layer base") || t.starts_with("@layer typography");

            // Walk the line character-by-character so `@apply` detection and
            // brace-depth bookkeeping stay in sync — a line like
            // `.btn { @apply px-4; }` opens a block, runs `@apply`, and
            // closes the block on the same line.
            let mut apply_inside_non_base = false;
            let mut iter = t.char_indices().peekable();
            while let Some((idx, ch)) = iter.next() {
                match ch {
                    '{' => {
                        brace_depth += 1;
                        if opens_base_layer && layer_base_depth.is_none() {
                            layer_base_depth = Some(brace_depth);
                        }
                    }
                    '}' => {
                        if let Some(d) = layer_base_depth
                            && brace_depth == d
                        {
                            layer_base_depth = None;
                        }
                        brace_depth = brace_depth.saturating_sub(1);
                    }
                    '@' if t[idx..].starts_with("@apply") => {
                        // Only flag if not inside an `@layer base` block.
                        let inside_base = layer_base_depth.is_some_and(|d| brace_depth >= d);
                        if !inside_base && brace_depth > 0 {
                            apply_inside_non_base = true;
                        }
                        // Advance past "@apply" so we don't re-match.
                        for _ in 0..5 {
                            iter.next();
                        }
                    }
                    _ => {}
                }
            }

            if apply_inside_non_base {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "Avoid `@apply` outside `@layer base` — compose classes in JSX or use CSS variables.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run_css(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("styles.css"), src))
    }

    fn run_ts(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), src))
    }

    #[test]
    fn flags_apply_in_component() {
        assert_eq!(run_css(".btn { @apply px-4 py-2 rounded; }").len(), 1);
    }

    #[test]
    fn allows_apply_in_base_layer() {
        assert!(run_css("@layer base {\n  body { @apply font-sans; }\n}").is_empty());
    }

    #[test]
    fn ignores_non_css_files() {
        assert!(run_ts("@apply px-4").is_empty());
    }
}
