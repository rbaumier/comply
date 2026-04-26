//! vue-no-watch-reactive-property AST backend.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["component"] => |node, source, ctx, diagnostics|
    let _ = source;
    for (idx, line) in ctx.source.lines().enumerate() {
        let trimmed = line.trim_start();
        if !(trimmed.starts_with("watch(") || trimmed.contains(" watch(") || trimmed.contains("\twatch(")) {
            continue;
        }
        let Some(start) = trimmed.find("watch(") else { continue };
        let after = &trimmed[start + 6..];
        let mut depth = 0i32;
        let mut end_arg: Option<usize> = None;
        for (i, b) in after.bytes().enumerate() {
            match b {
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth -= 1,
                b',' if depth == 0 => {
                    end_arg = Some(i);
                    break;
                }
                _ => {}
            }
            if depth < 0 {
                break;
            }
        }
        let Some(end) = end_arg else { continue };
        let arg = after[..end].trim();
        if arg.starts_with("()") || arg.starts_with("(") && arg.contains("=>") {
            continue;
        }
        if arg.contains('.') && !arg.ends_with(".value") {
            if arg.starts_with('[') || arg.starts_with('{') {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: super::META.id.into(),
                message: format!(
                    "`watch({arg}, ...)` passes a snapshot — the watcher won't react. Use a getter: `watch(() => {arg}, ...)`."
                ),
                severity: Severity::Error,
                span: None,
            });
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
    fn flags_member_access() {
        assert_eq!(run("<script setup>\nwatch(state.count, () => {})\n</script>").len(), 1);
    }

    #[test]
    fn allows_getter() {
        assert!(run("<script setup>\nwatch(() => state.count, () => {})\n</script>").is_empty());
    }

    #[test]
    fn allows_bare_ref() {
        assert!(run("<script setup>\nwatch(count, () => {})\n</script>").is_empty());
    }

    #[test]
    fn allows_dot_value() {
        assert!(run("<script setup>\nwatch(x.value, () => {})\n</script>").is_empty());
    }
}
