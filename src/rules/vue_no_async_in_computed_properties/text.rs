//! vue-no-async-in-computed-properties text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Heuristic regex-free scan: look for `computed(async` literal in the
/// source. The Vue SFC text contains the `<script>` block as-is, so
/// this catches `computed(async () => ...)` and `computed(async function`
/// without needing an AST.
fn detect_offsets(source: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    let needle = b"computed(async";
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if &bytes[i..i + needle.len()] == needle {
            // Word boundary on the left: previous char must not be alphanumeric.
            let prev_ok = i == 0
                || !(bytes[i - 1].is_ascii_alphanumeric()
                    || bytes[i - 1] == b'_'
                    || bytes[i - 1] == b'$');
            if prev_ok {
                out.push(i);
            }
        }
        i += 1;
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
        detect_offsets(ctx.source)
            .into_iter()
            .map(|offset| {
                let (line, column) = byte_to_line_col(ctx.source, offset);
                Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`computed(async …)` returns a Promise — the template \
                              renders `[object Promise]`. Use `watch` + a ref instead."
                        .into(),
                    severity: Severity::Error,
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

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("App.vue"), src))
    }

    #[test]
    fn flags_async_arrow_computed() {
        let src = "<script setup>\nconst x = computed(async () => await fetch('/x'));\n</script>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_async_function_computed() {
        let src = "const x = computed(async function () { return 1; });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_sync_computed() {
        let src = "const total = computed(() => items.value.length);";
        assert!(run(src).is_empty());
    }
}
