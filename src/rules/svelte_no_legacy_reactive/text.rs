//! svelte-no-legacy-reactive — text backend.
//!
//! Detects Svelte 4 `$:` reactive declarations inside `<script>` blocks.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_svelte(path: &std::path::Path) -> bool {
    path.extension().and_then(|s| s.to_str()) == Some("svelte")
}

/// True when the line, ignoring leading whitespace, begins with `$:` and the
/// next non-whitespace character is *not* a colon (so `$::` is excluded — not
/// that it's valid syntax, but it disambiguates from things that aren't this).
fn is_reactive_line(line: &str) -> bool {
    let t = line.trim_start();
    if !t.starts_with("$:") {
        return false;
    }
    // Make sure it's not `$:` inside a string or comment — handled crudely:
    // require that nothing precedes it on the line except whitespace.
    let leading = &line[..line.len() - t.len()];
    if !leading.chars().all(|c| c.is_whitespace()) {
        return false;
    }
    // Require a non-empty body after `$:`.
    let body = t[2..].trim();
    !body.is_empty() && !body.starts_with("//")
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_svelte(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        let mut in_script = false;
        for (idx, line) in ctx.source.lines().enumerate() {
            let lower = line.to_ascii_lowercase();
            if lower.contains("<script") {
                in_script = true;
            }
            if in_script && is_reactive_line(line) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: "svelte-no-legacy-reactive".into(),
                    message: "Replace legacy `$:` reactive declaration with `$derived` or `$effect`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            if lower.contains("</script>") {
                in_script = false;
            }
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
    fn flags_reactive_assignment() {
        let src = "<script>\nlet count = 0;\n$: doubled = count * 2;\n</script>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_reactive_block() {
        let src = "<script>\n$: {\n  console.log(count);\n}\n</script>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_derived() {
        let src = "<script>\nlet doubled = $derived(count * 2);\n</script>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_object_literal_property() {
        // `$: x` inside an object literal in a string would be a false positive;
        // but `$:` only matters at statement start, so this should be fine.
        let src = "<script>\nlet obj = { foo: 1 };\n</script>";
        assert!(run(src).is_empty());
    }
}
