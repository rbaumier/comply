//! vue-define-model-over-modelvalue AST backend.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["component"] => |node, source, ctx, diagnostics|
    let _ = source;
    let src = ctx.source;
    if !src.contains("modelValue") {
        return;
    }
    let mut defineprops_line: Option<usize> = None;
    let mut has_update_emit = false;
    for (idx, line) in src.lines().enumerate() {
        if line.contains("defineProps") && line.contains("modelValue") {
            defineprops_line = Some(idx);
        }
        if (line.contains("defineEmits") || line.contains("emit(")) && line.contains("update:modelValue") {
            has_update_emit = true;
        }
    }
    if defineprops_line.is_none() {
        let mut in_dp = false;
        let mut dp_start_line = 0usize;
        for (idx, line) in src.lines().enumerate() {
            if line.contains("defineProps") {
                in_dp = true;
                dp_start_line = idx;
            }
            if in_dp && line.contains("modelValue") {
                defineprops_line = Some(dp_start_line);
            }
            if in_dp && (line.contains("})") || line.contains(">()")) {
                in_dp = false;
            }
        }
    }
    if let Some(line) = defineprops_line
        && has_update_emit
    {
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: line + 1,
            column: 1,
            rule_id: super::META.id.into(),
            message: "Replace `modelValue` prop + `update:modelValue` emit with `defineModel()` (Vue 3.4+).".into(),
            severity: Severity::Warning,
            span: None,
        });
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
    fn flags_modelvalue_with_update_emit() {
        let sfc = "<script setup>\nconst props = defineProps<{ modelValue: string }>()\nconst emit = defineEmits<{ 'update:modelValue': [string] }>()\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn allows_definemodel() {
        let sfc = "<script setup>\nconst model = defineModel<string>()\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_props_without_modelvalue() {
        let sfc = "<script setup>\nconst props = defineProps<{ label: string }>()\n</script>";
        assert!(run(sfc).is_empty());
    }
}
