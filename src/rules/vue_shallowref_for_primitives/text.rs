//! vue-shallowref-for-primitives AST backend.
//!
//! Detects `ref(<literal>)` where the literal is a number, string, or boolean.

use crate::diagnostic::{Diagnostic, Severity};

fn is_primitive_arg(arg: &str) -> bool {
    let arg = arg.trim();
    if arg.is_empty() {
        return false;
    }
    if arg.chars().next().is_some_and(|c| c.is_ascii_digit() || c == '-') {
        let rest = arg.trim_start_matches('-');
        if rest.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '_') {
            return true;
        }
    }
    if (arg.starts_with('"') && arg.ends_with('"'))
        || (arg.starts_with('\'') && arg.ends_with('\''))
        || (arg.starts_with('`') && arg.ends_with('`'))
    {
        return true;
    }
    matches!(arg, "true" | "false" | "null" | "undefined")
}

crate::ast_check! { on ["component"] => |node, source, ctx, diagnostics|
    let _ = source;
    for (idx, line) in ctx.source.lines().enumerate() {
        let mut search_from = 0;
        while let Some(pos) = line[search_from..].find("ref(") {
            let abs = search_from + pos;
            let prev = if abs == 0 { ' ' } else { line.as_bytes()[abs - 1] as char };
            if prev.is_alphanumeric() || prev == '_' {
                search_from = abs + 4;
                continue;
            }
            let after = abs + 4;
            if let Some(close_rel) = line[after..].find(')') {
                let arg = &line[after..after + close_rel];
                if is_primitive_arg(arg) {
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: idx + 1,
                        column: abs + 1,
                        rule_id: super::META.id.into(),
                        message: "`ref(<primitive>)` installs a deep reactive proxy — use `shallowRef` for primitives.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            search_from = after;
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
    fn flags_ref_number() {
        assert_eq!(run("<script setup>\nconst x = ref(42)\n</script>").len(), 1);
    }

    #[test]
    fn flags_ref_string() {
        assert_eq!(run("<script setup>\nconst s = ref('hi')\n</script>").len(), 1);
    }

    #[test]
    fn flags_ref_bool() {
        assert_eq!(run("<script setup>\nconst b = ref(true)\n</script>").len(), 1);
    }

    #[test]
    fn allows_ref_object() {
        assert!(run("<script setup>\nconst o = ref({ n: 1 })\n</script>").is_empty());
    }

    #[test]
    fn allows_shallowref() {
        assert!(run("<script setup>\nconst x = shallowRef(42)\n</script>").is_empty());
    }
}
