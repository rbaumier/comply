//! Find string literals containing `w-screen` or `h-screen` as a
//! whole class token.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const TARGETS: &[&str] = &["w-screen", "h-screen"];

fn find_offenses(source: &str) -> Vec<(usize, &'static str)> {
    let mut out = Vec::new();
    for target in TARGETS {
        let mut from = 0usize;
        while let Some(rel) = source[from..].find(target) {
            let abs = from + rel;
            let bytes = source.as_bytes();
            let pre = bytes.get(abs.saturating_sub(1)).copied();
            let post = bytes.get(abs + target.len()).copied();
            let pre_ok = pre.is_none_or(|c| !c.is_ascii_alphanumeric() && c != b'_' && c != b'-');
            let post_ok = post.is_none_or(|c| !c.is_ascii_alphanumeric() && c != b'_' && c != b'-');
            if pre_ok && post_ok {
                out.push((abs, *target));
            }
            from = abs + target.len();
        }
    }
    out
}

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
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        find_offenses(ctx.source)
            .into_iter()
            .map(|(offset, klass)| {
                let (line, column) = byte_to_line_col(ctx.source, offset);
                Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`{klass}` jumps when the mobile URL bar collapses — use `{}` instead.",
                        if klass.starts_with('w') {
                            "w-full"
                        } else {
                            "min-h-dvh"
                        }
                    ),
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
    fn flags_w_screen() {
        assert_eq!(run(r#"const x = <div className="w-screen" />;"#).len(), 1);
    }

    #[test]
    fn flags_h_screen() {
        assert_eq!(
            run(r#"const x = <div className="flex h-screen" />;"#).len(),
            1
        );
    }

    #[test]
    fn flags_both() {
        assert_eq!(
            run(r#"const x = <div className="w-screen h-screen" />;"#).len(),
            2
        );
    }

    #[test]
    fn allows_w_full() {
        assert!(run(r#"const x = <div className="w-full min-h-dvh" />;"#).is_empty());
    }

    #[test]
    fn allows_unrelated_classes_with_screen_suffix() {
        // `w-screenshot` is not `w-screen`.
        assert!(run(r#"const x = <div className="w-screenshot" />;"#).is_empty());
    }
}
