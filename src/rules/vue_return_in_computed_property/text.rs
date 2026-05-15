//! vue-return-in-computed-property text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Walk from `start` (a `{` byte index in `src`) and return the byte index
/// of the matching `}`, ignoring braces inside strings and template
/// literals. Returns `None` if unbalanced.
fn matching_brace(src: &str, start: usize) -> Option<usize> {
    let bytes = src.as_bytes();
    let mut depth: i32 = 0;
    let mut i = start;
    let mut in_str: Option<u8> = None;
    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = in_str {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == q {
                in_str = None;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' | b'\'' | b'`' => in_str = Some(c),
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.contains("computed(") {
            return Vec::new();
        }
        let mut diags = Vec::new();
        // Look for `computed(() => {` pattern — only block bodies can be
        // return-less. Arrow expression bodies always return.
        let needle = "computed(() => {";
        let mut cursor = 0usize;
        while let Some(rel) = src[cursor..].find(needle) {
            let abs = cursor + rel;
            let brace_idx = abs + needle.len() - 1; // the `{` byte
            let Some(end) = matching_brace(src, brace_idx) else { break };
            let body = &src[brace_idx + 1..end];
            // Detect a `return` keyword at statement position.
            let has_return = body.lines().any(|l| {
                let t = l.trim_start();
                t.starts_with("return ") || t == "return;" || t.starts_with("return\t")
            });
            if !has_return {
                // Compute line/column of the `computed(` keyword.
                let line_no = src[..abs].bytes().filter(|b| *b == b'\n').count() + 1;
                let col = src[..abs].rfind('\n').map(|nl| abs - nl).unwrap_or(abs + 1);
                diags.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: line_no,
                    column: col,
                    rule_id: super::META.id.into(),
                    message: "`computed()` callback has a block body but never returns — \
                              the property will resolve to `undefined`."
                        .into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
            cursor = end + 1;
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("App.vue"), src))
    }

    #[test]
    fn flags_block_without_return() {
        let src = "const x = computed(() => { const a = 1; });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_block_with_return() {
        let src = "const x = computed(() => { return 1; });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_expression_body() {
        let src = "const x = computed(() => a.value + 1);";
        assert!(run(src).is_empty());
    }
}
