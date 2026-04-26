//! vue-computed-no-side-effects AST backend.
//!
//! Scans lines between `computed(` and the matching brace/paren end, flagging
//! emit() / console.* / fetch() / axios. / assignments to `.value =` etc.
//!
//! Uses the Vue tree-sitter grammar as the dispatch mechanism: we run once per
//! file on the root `component` node, then scan the source for `computed(`
//! calls via text-matching (the grammar exposes `<script>` bodies as a single
//! `raw_text` blob, not as parsed JS/TS, so a true AST walk of the call body
//! would require re-parsing TS — which is not cheaper than the text scan for
//! this heuristic).

use crate::diagnostic::{Diagnostic, Severity};

const SIDE_EFFECT_MARKERS: &[&str] = &[
    "emit(",
    "console.",
    "fetch(",
    "axios.",
    ".post(",
    ".put(",
    ".delete(",
    ".patch(",
    "$emit(",
];

crate::ast_check! { on ["component"] => |node, source, ctx, diagnostics|
    // Gate: run once per file, when we first see the root `component` node.
    let _ = source;
    let src = ctx.source;
    let bytes = src.as_bytes();
    let mut i = 0;
    while let Some(pos) = src[i..].find("computed(") {
        let abs = i + pos;
        let prev = if abs == 0 { ' ' } else { bytes[abs - 1] as char };
        if prev.is_alphanumeric() || prev == '_' {
            i = abs + 9;
            continue;
        }
        let start = abs + 9;
        let mut depth: i32 = 1;
        let mut j = start;
        while j < bytes.len() && depth > 0 {
            match bytes[j] {
                b'(' => depth += 1,
                b')' => depth -= 1,
                _ => {}
            }
            j += 1;
        }
        let body = &src[start..j.saturating_sub(1)];
        let base_line = src[..abs].matches('\n').count();
        for (line_off, line) in body.lines().enumerate() {
            for marker in SIDE_EFFECT_MARKERS {
                if line.contains(marker) {
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: base_line + line_off + 1,
                        column: 1,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "Side effect `{marker}` inside `computed()` — computeds must be pure."
                        ),
                        severity: Severity::Error,
                        span: None,
                    });
                    break;
                }
            }
            let trimmed = line.trim_start();
            if trimmed.contains(".value =") && !trimmed.contains("==") {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: base_line + line_off + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "Assignment to `.value` inside `computed()` — computeds must be pure.".into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
        i = j;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::{AstCheck, CheckCtx};
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_vue_updated::language())
            .expect("vue grammar");
        let tree = parser.parse(source, None).expect("parser");
        Check.check(&CheckCtx::for_test(Path::new("t.vue"), source), &tree)
    }

    #[test]
    fn flags_console_in_computed() {
        let sfc = "<script setup>\nconst c = computed(() => { console.log('x'); return 1 })\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn flags_emit_in_computed() {
        let sfc = "<script setup>\nconst c = computed(() => { emit('x'); return 1 })\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn flags_value_assign_in_computed() {
        let sfc = "<script setup>\nconst c = computed(() => { other.value = 2; return 1 })\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn allows_pure_computed() {
        let sfc = "<script setup>\nconst c = computed(() => x.value * 2)\n</script>";
        assert!(run(sfc).is_empty());
    }
}
