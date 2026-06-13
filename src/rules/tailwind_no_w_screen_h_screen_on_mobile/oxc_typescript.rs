use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const TARGETS: &[&str] = &["w-screen", "h-screen"];

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
        Some(TARGETS)
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut out = Vec::new();
        for target in TARGETS {
            if !ctx.source_contains(target) {
                continue;
            }
            let mut from = 0usize;
            while let Some(rel) = ctx.source[from..].find(target) {
                let abs = from + rel;
                let bytes = ctx.source.as_bytes();
                let pre = bytes.get(abs.saturating_sub(1)).copied();
                let post = bytes.get(abs + target.len()).copied();
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
                        message: format!(
                            "`{target}` jumps when the mobile URL bar collapses — use `{}` instead.",
                            if target.starts_with('w') { "w-full" } else { "min-h-dvh" }
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
                from = abs + target.len();
            }
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
        assert!(run(r#"const x = <div className="w-screenshot" />;"#).is_empty());
    }
}
