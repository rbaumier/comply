use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut template_line: Option<usize> = None;
        let mut script_line: Option<usize> = None;
        for (i, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if t.starts_with("<template") && template_line.is_none() {
                template_line = Some(i);
            }
            if t.starts_with("<script") && script_line.is_none() {
                script_line = Some(i);
            }
        }
        match (template_line, script_line) {
            (Some(tl), Some(sl)) if tl < sl => {
                vec![Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: tl + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "`<template>` appears before `<script>` — the canonical SFC order is: script → template → style.".into(),
                    severity: Severity::Warning,
                    span: None,
                }]
            }
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("Comp.vue"), src))
    }
    #[test]
    fn flags_template_before_script() {
        assert_eq!(
            run("<template><div /></template>\n<script setup lang=\"ts\">\n</script>").len(),
            1
        );
    }
    #[test]
    fn allows_script_before_template() {
        assert!(
            run("<script setup lang=\"ts\">\n</script>\n<template><div /></template>").is_empty()
        );
    }

    /// A template-before-script fixture under `tests/` is a deliberate Vue
    /// 2-style test input — `skip_in_test_dir` must suppress the rule there,
    /// while a real component in `src/` with the same order is still flagged.
    #[test]
    fn skips_template_before_script_fixture_in_test_dir() {
        use crate::files::Language;
        use crate::project::default_static_project_ctx;
        use crate::rules::file_ctx::FileCtx;
        let src = "<template>\n  <button @click=\"greet\" />\n</template>\n<script lang=\"ts\">\nexport default { setup() { return {} } }\n</script>";
        let project = default_static_project_ctx();
        let fixture = FileCtx::build(
            Path::new("tests/components/EmitsEvent.vue"),
            src,
            Language::Vue,
            project,
        );
        let component =
            FileCtx::build(Path::new("src/components/Emits.vue"), src, Language::Vue, project);
        assert!(
            !super::super::META.applies_to_file(&fixture),
            "test-fixture SFC must be skipped"
        );
        assert!(
            super::super::META.applies_to_file(&component),
            "real src/ component must still be checked"
        );
        assert_eq!(run(src).len(), 1, "the underlying violation must still fire");
    }
}
