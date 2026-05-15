//! eslint-comments-disable-enable-pair text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn byte_to_line_col(source: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    for (i, c) in source.char_indices() {
        if i >= byte_offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["eslint-disable"])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        let mut diags = Vec::new();
        // Walk block comments. Block-form `eslint-disable` is the only one
        // that opens a region; `-next-line` and `-line` are scoped to one
        // line and don't need pairing.
        let mut from = 0usize;
        while let Some(rel) = src[from..].find("/* eslint-disable") {
            let abs = from + rel;
            // Skip directives that are scoped to one line.
            let tail = &src[abs + "/* eslint-disable".len()..];
            if tail.starts_with("-next-line") || tail.starts_with("-line") {
                from = abs + 1;
                continue;
            }
            // Find the matching `*/` close.
            let Some(end_rel) = src[abs..].find("*/") else { break };
            let after = abs + end_rel + 2;
            // Is there a corresponding `/* eslint-enable */` later in the
            // file? We're permissive: any block comment with
            // `eslint-enable` is enough — we don't enforce that the rule
            // list matches.
            if !src[after..].contains("eslint-enable") {
                let (line, column) = byte_to_line_col(src, abs);
                diags.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`/* eslint-disable */` block has no matching `/* eslint-enable */` \
                              — the rule stays disabled for the rest of the file."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            from = after;
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("foo.ts"), src))
    }

    #[test]
    fn flags_disable_without_enable() {
        let src = "/* eslint-disable no-console */\nconsole.log('x');\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_disable_with_enable() {
        let src = "/* eslint-disable no-console */\nconsole.log('x');\n/* eslint-enable */\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_disable_next_line() {
        let src = "// eslint-disable-next-line no-console\nconsole.log('x');\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_disable_line() {
        let src = "console.log('x'); // eslint-disable-line no-console\n";
        assert!(run(src).is_empty());
    }
}
