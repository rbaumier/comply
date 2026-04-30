//! vue-no-value-on-reactive AST backend.
//!
//! Tracks variables assigned from `reactive(...)` and flags any usage of
//! `name.value` for those variables.

use crate::diagnostic::{Diagnostic, Severity};

fn collect_reactives(source: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        for prefix in ["const ", "let ", "var "] {
            if let Some(rest) = trimmed.strip_prefix(prefix)
                && let Some(eq) = rest.find('=')
            {
                let name = rest[..eq].trim().trim_end_matches(':');
                let after_eq = rest[eq + 1..].trim_start();
                if after_eq.starts_with("reactive(")
                    && !name.is_empty()
                    && name.chars().all(|c| c.is_alphanumeric() || c == '_')
                {
                    names.push(name.to_string());
                }
            }
        }
    }
    names
}

crate::ast_check! { on ["component"] => |node, source, ctx, diagnostics|
    let _ = source;
    let names = collect_reactives(ctx.source);
    if names.is_empty() {
        return;
    }
    for (idx, line) in ctx.source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("const ") || trimmed.starts_with("let ") || trimmed.starts_with("var ") {
            continue;
        }
        for name in &names {
            let pattern = format!("{name}.value");
            if line.contains(&pattern) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`{name}` is a reactive proxy — `{name}.value` is undefined. Access its keys directly."
                    ),
                    severity: Severity::Error,
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
    fn flags_value_on_reactive() {
        let sfc =
            "<script setup>\nconst state = reactive({ n: 0 })\nconsole.log(state.value)\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn allows_direct_key_access() {
        let sfc =
            "<script setup>\nconst state = reactive({ n: 0 })\nconsole.log(state.n)\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_value_on_ref() {
        let sfc = "<script setup>\nconst x = ref(0)\nconsole.log(x.value)\n</script>";
        assert!(run(sfc).is_empty());
    }
}
