//! svelte-no-slot-element — text backend.
//!
//! Flags any `<slot ...>` or `<slot/>` element in Svelte templates.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_svelte(path: &std::path::Path) -> bool {
    path.extension().and_then(|s| s.to_str()) == Some("svelte")
}

/// True if `<slot` at `idx` opens an actual `slot` element — i.e. the next
/// character after `<slot` is whitespace, `/`, or `>`. This rejects things
/// like `<slotted>` or `<slot-x>`.
fn is_slot_open(bytes: &[u8], idx: usize) -> bool {
    if idx + 5 > bytes.len() {
        return false;
    }
    if &bytes[idx..idx + 5] != b"<slot" {
        return false;
    }
    let next = bytes.get(idx + 5).copied().unwrap_or(0);
    matches!(next, b' ' | b'\t' | b'\n' | b'\r' | b'/' | b'>')
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_svelte(ctx.path) {
            return Vec::new();
        }
        let source = ctx.source;
        let bytes = source.as_bytes();
        let mut diagnostics = Vec::new();
        let mut search_from = 0;
        while let Some(rel) = source[search_from..].find("<slot") {
            let i = search_from + rel;
            if is_slot_open(bytes, i) {
                let line = source[..i].bytes().filter(|b| *b == b'\n').count() + 1;
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line,
                    column: 1,
                    rule_id: "svelte-no-slot-element".into(),
                    message: "Replace `<slot>` with a snippet rendered via `{@render ...}`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            search_from = i + 5;
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("Comp.svelte"), source))
    }

    #[test]
    fn flags_self_closing_slot() {
        assert_eq!(run("<div><slot /></div>").len(), 1);
    }

    #[test]
    fn flags_named_slot() {
        assert_eq!(run("<slot name=\"header\" />").len(), 1);
    }

    #[test]
    fn allows_render_directive() {
        assert!(run("{@render header?.()}").is_empty());
    }

    #[test]
    fn ignores_similarly_named_tags() {
        assert!(run("<slotted></slotted>").is_empty());
    }
}
