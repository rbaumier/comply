//! vue-inject-key-typed AST backend.
//!
//! Flags `provide('lit', ...)` / `inject('lit')` where the first argument is a
//! string literal (single, double, or backtick quoted).

use crate::diagnostic::{Diagnostic, Severity};

fn starts_with_string_literal(arg: &str) -> bool {
    let arg = arg.trim_start();
    matches!(arg.chars().next(), Some('"') | Some('\'') | Some('`'))
}

crate::ast_check! { on ["component"] => |node, source, ctx, diagnostics|
    let _ = source;
    for (idx, line) in ctx.source.lines().enumerate() {
        for fn_name in ["provide(", "inject("] {
            if let Some(pos) = line.find(fn_name) {
                let prev = line[..pos].chars().last().unwrap_or(' ');
                if prev == '.' {
                    continue;
                }
                let after = &line[pos + fn_name.len()..];
                if starts_with_string_literal(after) {
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: idx + 1,
                        column: pos + 1,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "`{}` uses a string key — use a typed `InjectionKey<T>` symbol instead.",
                            fn_name.trim_end_matches('(')
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
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
    fn flags_provide_string_key() {
        assert_eq!(
            run("<script setup>\nprovide('user', user)\n</script>").len(),
            1
        );
    }

    #[test]
    fn flags_inject_string_key() {
        assert_eq!(
            run("<script setup>\nconst u = inject('user')\n</script>").len(),
            1
        );
    }

    #[test]
    fn allows_symbol_key() {
        assert!(run("<script setup>\nprovide(USER_KEY, user)\n</script>").is_empty());
    }

    #[test]
    fn allows_dot_provide() {
        assert!(run("<script setup>\napp.provide('user', user)\n</script>").is_empty());
    }
}
