//! svelte-no-on-colon-directive — text backend.
//!
//! Flags `on:event` attributes (Svelte 4 syntax) outside `<script>` blocks.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_svelte(path: &std::path::Path) -> bool {
    path.extension().and_then(|s| s.to_str()) == Some("svelte")
}

/// True when the byte at `idx` is the start of an `on:` attribute. Requires
/// that the previous byte be whitespace (attributes are space-separated) and
/// that the character following `on:` is an ASCII letter (event name start).
fn is_on_colon_at(bytes: &[u8], idx: usize) -> bool {
    if idx + 3 > bytes.len() {
        return false;
    }
    if &bytes[idx..idx + 3] != b"on:" {
        return false;
    }
    if idx == 0 {
        return false;
    }
    let prev = bytes[idx - 1];
    if !(prev == b' ' || prev == b'\t' || prev == b'\n' || prev == b'\r') {
        return false;
    }
    let next = bytes.get(idx + 3).copied().unwrap_or(0);
    next.is_ascii_alphabetic()
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_svelte(ctx.path) {
            return Vec::new();
        }
        let source = ctx.source;
        let bytes = source.as_bytes();
        let mut diagnostics = Vec::new();
        let mut in_script = false;
        let mut line_starts: Vec<usize> = vec![0];
        for (i, b) in bytes.iter().enumerate() {
            if *b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        // Walk character-by-character with a tiny state machine for <script>.
        let mut i = 0;
        while i < bytes.len() {
            if !source.is_char_boundary(i) {
                i += 1;
                continue;
            }
            if !in_script && i + 7 <= bytes.len() && source[i..].to_ascii_lowercase().starts_with("<script") {
                in_script = true;
            } else if in_script && i + 9 <= bytes.len() && source[i..].to_ascii_lowercase().starts_with("</script>") {
                in_script = false;
                i += 9;
                continue;
            }
            if !in_script && is_on_colon_at(bytes, i) {
                let line = line_starts.partition_point(|&s| s <= i);
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line,
                    column: 1,
                    rule_id: "svelte-no-on-colon-directive".into(),
                    message: "Replace `on:event` directive with the `onevent` attribute (Svelte 5).".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            i += 1;
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
    fn flags_on_click() {
        let src = "<button on:click={handler}>x</button>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_on_submit() {
        let src = "<form on:submit={save}>\n</form>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_onclick_attribute() {
        let src = "<button onclick={handler}>x</button>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_text_containing_on_colon() {
        // Inside `<script>` we don't flag — that's TS code where `on:` doesn't
        // apply.
        let src = "<script>\nconst regex = /on:/;\n</script>";
        assert!(run(src).is_empty());
    }
}
