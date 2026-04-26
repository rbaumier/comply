//! vue-watch-immediate-over-onmounted AST backend.
//!
//! Detects a file that contains both a `watch(x, fn)` call AND an
//! `onMounted(...)` where the mounted body references the same `fn`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["component"] => |node, source, ctx, diagnostics|
    let _ = source;
    let src = ctx.source;
    if !src.contains("onMounted(") || !src.contains("watch(") {
        return;
    }
    let lines: Vec<&str> = src.lines().collect();
    let mut watch_fns: Vec<String> = Vec::new();
    for line in &lines {
        if let Some(start) = line.find("watch(") {
            let after = &line[start + 6..];
            let mut depth = 0i32;
            let mut comma = None;
            for (i, b) in after.bytes().enumerate() {
                match b {
                    b'(' | b'[' | b'{' => depth += 1,
                    b')' | b']' | b'}' => depth -= 1,
                    b',' if depth == 0 => {
                        comma = Some(i);
                        break;
                    }
                    _ => {}
                }
                if depth < 0 {
                    break;
                }
            }
            if let Some(c) = comma {
                let second = after[c + 1..].trim_start();
                let ident: String = second
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !ident.is_empty() && !["true", "false", "null"].contains(&ident.as_str()) {
                    watch_fns.push(ident);
                }
            }
        }
    }
    if watch_fns.is_empty() {
        return;
    }

    for (idx, line) in lines.iter().enumerate() {
        if !line.contains("onMounted(") {
            continue;
        }
        let window: String = lines[idx..(idx + 5).min(lines.len())].join("\n");
        for fname in &watch_fns {
            let call = format!("{fname}(");
            if window.contains(&call) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`onMounted` duplicates the `watch` — pass `{{ immediate: true }}` to the watch of `{fname}` instead."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
                break;
            }
        }
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
    fn flags_onmounted_duplicating_watch() {
        let sfc = "<script setup>\nwatch(x, load)\nonMounted(() => load(x.value))\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn allows_watch_with_immediate() {
        let sfc = "<script setup>\nwatch(x, load, { immediate: true })\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_onmounted_unrelated() {
        let sfc = "<script setup>\nwatch(x, load)\nonMounted(() => otherThing())\n</script>";
        assert!(run(sfc).is_empty());
    }
}
