//! Flag `setQueryData([` (with whitespace tolerance).

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

fn find_offenses(source: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = source[from..].find("setQueryData(") {
        let abs = from + rel;
        // Word boundary before
        let pre = source.as_bytes().get(abs.saturating_sub(1)).copied();
        let boundary = pre.is_none_or(|c| !c.is_ascii_alphanumeric() && c != b'_' && c != b'$');
        if boundary {
            // After `setQueryData(`, skip whitespace, check first char.
            let after = abs + "setQueryData(".len();
            let bs = source.as_bytes();
            let mut i = after;
            while i < bs.len() && bs[i].is_ascii_whitespace() {
                i += 1;
            }
            if bs.get(i) == Some(&b'[') {
                out.push(abs);
            }
        }
        from = abs + 1;
    }
    out
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !ctx.source.contains("setQueryData(") {
            return Vec::new();
        }
        find_offenses(ctx.source)
            .into_iter()
            .map(|offset| {
                let (line, column) = byte_to_line_col(ctx.source, offset);
                Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`setQueryData` uses an inline array key — \
                              extract a query key factory so cache writes are findable."
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
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_inline_array_key() {
        let src = "queryClient.setQueryData(['users', id], data);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_inline_array_with_whitespace() {
        let src = "queryClient.setQueryData( ['users', id], data);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_factory_key() {
        let src = "queryClient.setQueryData(userKeys.detail(id), data);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_variable_key() {
        let src = "queryClient.setQueryData(myKey, data);";
        assert!(run(src).is_empty());
    }
}
