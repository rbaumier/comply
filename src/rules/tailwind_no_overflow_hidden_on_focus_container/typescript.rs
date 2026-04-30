//! Find `overflow-hidden` as a whole class token in source.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const TARGET: &str = "overflow-hidden";

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

fn find_offenses(source: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = source[from..].find(TARGET) {
        let abs = from + rel;
        let bytes = source.as_bytes();
        let pre = bytes.get(abs.saturating_sub(1)).copied();
        let post = bytes.get(abs + TARGET.len()).copied();
        let pre_ok = pre.is_none_or(|c| !c.is_ascii_alphanumeric() && c != b'_' && c != b'-');
        let post_ok = post.is_none_or(|c| !c.is_ascii_alphanumeric() && c != b'_' && c != b'-');
        if pre_ok && post_ok {
            out.push(abs);
        }
        from = abs + TARGET.len();
    }
    out
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        find_offenses(ctx.source)
            .into_iter()
            .map(|offset| {
                let (line, column) = byte_to_line_col(ctx.source, offset);
                Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`overflow-hidden` clips focus rings on focusable children — \
                              use `overflow-clip` or move clipping to a non-focus host."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.tsx"), source))
    }

    #[test]
    fn flags_overflow_hidden() {
        assert_eq!(
            run(r#"const x = <div className="overflow-hidden" />;"#).len(),
            1
        );
    }

    #[test]
    fn flags_overflow_hidden_among_classes() {
        assert_eq!(
            run(r#"const x = <div className="rounded overflow-hidden p-4" />;"#).len(),
            1
        );
    }

    #[test]
    fn allows_overflow_clip() {
        assert!(run(r#"const x = <div className="overflow-clip" />;"#).is_empty());
    }

    #[test]
    fn allows_overflow_auto() {
        assert!(run(r#"const x = <div className="overflow-auto" />;"#).is_empty());
    }
}
