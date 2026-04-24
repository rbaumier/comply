//! vue-typed-define-props-emits AST backend.
//!
//! Only fires in SFCs with `<script ... lang="ts">`.

use crate::diagnostic::{Diagnostic, Severity};

fn has_ts_script(source: &str) -> bool {
    for line in source.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("<script")
            && (trimmed.contains("lang=\"ts\"") || trimmed.contains("lang='ts'"))
        {
            return true;
        }
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "component" { return; }
    let _ = source;
    if !has_ts_script(ctx.source) {
        return;
    }
    for (idx, line) in ctx.source.lines().enumerate() {
        for (name, runtime_starts) in [
            ("defineProps", ["defineProps({", "defineProps(["]),
            ("defineEmits", ["defineEmits({", "defineEmits(["]),
        ] {
            for pat in runtime_starts {
                if line.contains(pat) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "In `lang=\"ts\"` SFCs use the type form: `{name}<{{ ... }}>()` instead of the runtime object/array form."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                    break;
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
    fn flags_runtime_props_in_ts() {
        let sfc = "<script setup lang=\"ts\">\nconst p = defineProps({ msg: String })\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn flags_runtime_emits_in_ts() {
        let sfc = "<script setup lang=\"ts\">\nconst e = defineEmits(['change'])\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn allows_type_form() {
        let sfc = "<script setup lang=\"ts\">\nconst p = defineProps<{ msg: string }>()\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn ignores_non_ts() {
        let sfc = "<script setup>\nconst p = defineProps({ msg: String })\n</script>";
        assert!(run(sfc).is_empty());
    }
}
