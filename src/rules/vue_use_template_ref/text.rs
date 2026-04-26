//! vue-use-template-ref AST backend.
//!
//! Detects `const NAME = ref(null)` where NAME is used as a template `ref="NAME"`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["component"] => |node, source, ctx, diagnostics|
    let _ = source;
    let src = ctx.source;
    let mut candidates: Vec<(usize, String)> = Vec::new();
    for (idx, line) in src.lines().enumerate() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("const ")
            && let Some(eq) = rest.find('=')
        {
            let name = rest[..eq].trim().trim_end_matches(':');
            let after = rest[eq + 1..].trim_start();
            if (after.starts_with("ref(null") || after.starts_with("ref<") && after.contains("(null"))
                && !name.is_empty()
                && name.chars().all(|c| c.is_alphanumeric() || c == '_')
            {
                candidates.push((idx, name.to_string()));
            }
        }
    }
    for (idx, name) in candidates {
        let attr = format!("ref=\"{name}\"");
        if src.contains(&attr) {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: super::META.id.into(),
                message: format!(
                    "`{name}` is a template ref — replace with `const {name} = useTemplateRef('{name}')` (Vue 3.5+)."
                ),
                severity: Severity::Warning,
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
    fn flags_ref_null_used_as_template_ref() {
        let sfc = "<script setup>\nconst el = ref(null)\n</script>\n<template>\n<div ref=\"el\"></div>\n</template>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn allows_use_template_ref() {
        let sfc = "<script setup>\nconst el = useTemplateRef('el')\n</script>\n<template>\n<div ref=\"el\"></div>\n</template>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_ref_null_no_template_usage() {
        let sfc = "<script setup>\nconst x = ref(null)\n</script>";
        assert!(run(sfc).is_empty());
    }
}
