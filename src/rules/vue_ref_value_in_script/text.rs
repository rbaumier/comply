//! vue-ref-value-in-script AST backend.
//!
//! Scans `<script>` section for `ref()` declarations and flags comparisons /
//! conditions that reference the bare identifier without `.value`.

use crate::diagnostic::{Diagnostic, Severity};

fn script_range(source: &str) -> Option<(usize, usize)> {
    let start = source.find("<script")?;
    let after_open = source[start..].find('>')? + start + 1;
    let end_rel = source[after_open..].find("</script>")?;
    Some((after_open, after_open + end_rel))
}

fn collect_refs(script: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in script.lines() {
        let trimmed = line.trim();
        for prefix in ["const ", "let ", "var "] {
            if let Some(rest) = trimmed.strip_prefix(prefix)
                && let Some(eq) = rest.find('=')
            {
                let name = rest[..eq].trim().trim_end_matches(':');
                let after_eq = rest[eq + 1..].trim_start();
                if (after_eq.starts_with("ref(") || after_eq.starts_with("shallowRef("))
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
    let Some((start, end)) = script_range(ctx.source) else {
        return;
    };
    let script = &ctx.source[start..end];
    let names = collect_refs(script);
    if names.is_empty() {
        return;
    }

    let base_line = ctx.source[..start].matches('\n').count();

    for (idx, line) in script.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("const ") || trimmed.starts_with("let ") || trimmed.starts_with("var ") {
            continue;
        }
        for name in &names {
            let patterns = [
                format!("if ({name})"),
                format!("if ({name} "),
                format!("if (!{name})"),
                format!("if (!{name} "),
                format!("while ({name})"),
                format!("while ({name} "),
                format!("({name} === "),
                format!("({name} !== "),
                format!("({name} == "),
                format!("({name} != "),
                format!("({name} > "),
                format!("({name} < "),
                format!("({name} >= "),
                format!("({name} <= "),
            ];
            let mut hit = false;
            for pat in &patterns {
                if line.contains(pat) {
                    hit = true;
                    break;
                }
            }
            if hit {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: base_line + idx + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`{name}` is a ref — comparing it without `.value` compares the Ref object, not the inner value. Use `{name}.value`."
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
    fn flags_bare_ref_in_condition() {
        let sfc = "<script setup>\nconst x = ref(0)\nif (x > 0) {}\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn allows_dot_value() {
        let sfc = "<script setup>\nconst x = ref(0)\nif (x.value > 0) {}\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn ignores_non_ref() {
        let sfc = "<script setup>\nconst x = 0\nif (x > 0) {}\n</script>";
        assert!(run(sfc).is_empty());
    }
}
