use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if src.contains("<script setup") {
            return vec![];
        }
        if !src.contains("setup()") && !src.contains("setup(props") {
            return vec![];
        }
        let mut diags = Vec::new();
        for (i, line) in src.lines().enumerate() {
            let t = line.trim();
            if (t.starts_with("<script>") || t.starts_with("<script lang=")) && !t.contains("setup")
            {
                diags.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message:
                        "Use `<script setup>` instead of `<script>` with a `setup()` function."
                            .into(),
                    severity: Severity::Error,
                    span: None,
                });
                break;
            }
        }
        diags
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
    fn flags_script_with_setup_fn() {
        assert_eq!(
            run("<script lang=\"ts\">\nexport default { setup() { return {} } }\n</script>").len(),
            1
        );
    }
    #[test]
    fn allows_script_setup() {
        assert!(run("<script setup lang=\"ts\">\nconst x = 1\n</script>").is_empty());
    }

    /// An Options-API `<script setup()>` fixture under `tests/` deliberately
    /// exercises the non-`<script setup>` style — `skip_in_test_dir` must
    /// suppress the rule there, while the same component in `src/` is flagged.
    #[test]
    fn skips_options_api_fixture_in_test_dir() {
        use crate::files::Language;
        use crate::project::default_static_project_ctx;
        use crate::rules::file_ctx::FileCtx;
        let src = "<script lang=\"ts\">\nimport { defineComponent } from 'vue'\nexport default defineComponent({ setup(props) { return {} } })\n</script>";
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
