//! Find arbitrary Tailwind text size values: `text-[Npx]` where N < 12.

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

/// Look for `text-[<N>px]` where N is a positive integer < 12.
fn find_offenses(source: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = source[from..].find("text-[") {
        let abs = from + rel;
        let inside_start = abs + "text-[".len();
        // Read until `]`.
        let bytes = source.as_bytes();
        let mut i = inside_start;
        while i < bytes.len() && bytes[i] != b']' {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let inside = &source[inside_start..i];
        // Match `<int>px`.
        if let Some(stripped) = inside.strip_suffix("px")
            && let Ok(n) = stripped.parse::<u32>()
            && n < 12
        {
            out.push((abs, inside.to_string()));
        }
        from = i + 1;
    }
    out
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !ctx.source.contains("text-[") {
            return Vec::new();
        }
        find_offenses(ctx.source)
            .into_iter()
            .map(|(offset, value)| {
                let (line, column) = byte_to_line_col(ctx.source, offset);
                Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`text-[{value}]` is below 12px — use `text-xs` or larger for accessibility."
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
    fn flags_text_10px() {
        assert_eq!(run(r#"const x = <p className="text-[10px]" />;"#).len(), 1);
    }

    #[test]
    fn flags_text_11px() {
        assert_eq!(run(r#"const x = <p className="text-[11px]" />;"#).len(), 1);
    }

    #[test]
    fn allows_text_12px() {
        assert!(run(r#"const x = <p className="text-[12px]" />;"#).is_empty());
    }

    #[test]
    fn allows_text_xs() {
        assert!(run(r#"const x = <p className="text-xs" />;"#).is_empty());
    }

    #[test]
    fn allows_non_px_values() {
        assert!(run(r#"const x = <p className="text-[0.75rem]" />;"#).is_empty());
    }
}
