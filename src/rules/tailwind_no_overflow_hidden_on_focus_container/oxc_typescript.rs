use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

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

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[TARGET])
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !ctx.source_contains(TARGET) {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut from = 0usize;
        while let Some(rel) = ctx.source[from..].find(TARGET) {
            let abs = from + rel;
            let bytes = ctx.source.as_bytes();
            let pre = bytes.get(abs.saturating_sub(1)).copied();
            let post = bytes.get(abs + TARGET.len()).copied();
            let pre_ok =
                pre.is_none_or(|c| !c.is_ascii_alphanumeric() && c != b'_' && c != b'-');
            let post_ok =
                post.is_none_or(|c| !c.is_ascii_alphanumeric() && c != b'_' && c != b'-');
            if pre_ok && post_ok {
                let (line, column) = byte_to_line_col(ctx.source, abs);
                out.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`overflow-hidden` clips focus rings on focusable children — \
                              use `overflow-clip` or move clipping to a non-focus host."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            from = abs + TARGET.len();
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(source, &Check)
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
