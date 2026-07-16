//! vue-no-setup-props-reactivity-loss text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.contains("defineProps") {
            return Vec::new();
        }
        // Reactive Props Destructuring is stable since Vue 3.5: the SFC compiler
        // rewrites destructured prop refs back to `props.x`, preserving reactivity,
        // so this rule does not apply. Suppress when the project provably declares a
        // stack that ships that compiler transform: vue >= 3.5; nuxt >= 3.13 (from
        // the 3.13 line onward Vue 3.5+ is bundled transitively — 3.13.2 pinned
        // `vue ^3.5` and every later 3.x follows — so such projects often declare
        // only `nuxt` with no direct `vue`); or `@vue/compiler-sfc` >= 3.5 — the
        // package that actually implements the transform, declared directly and
        // often only in devDependencies.
        let vue_ok = matches!(
            ctx.project.nearest_dependency_version_min(ctx.path, "vue"),
            Some(v) if v >= (3, 5)
        );
        let nuxt_ok = matches!(
            ctx.project.nearest_dependency_version_min(ctx.path, "nuxt"),
            Some(v) if v >= (3, 13)
        );
        let compiler_sfc_ok = matches!(
            ctx.project.nearest_dependency_version_min(ctx.path, "@vue/compiler-sfc"),
            Some(v) if v >= (3, 5)
        );
        if vue_ok || nuxt_ok || compiler_sfc_ok {
            return Vec::new();
        }
        let mut diags = Vec::new();
        for (i, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("*") {
                continue;
            }
            // Look for `const { … } = defineProps` (any whitespace, no `:` to
            // skip the optional type-annotation case).
            let Some(eq_idx) = trimmed.find('=') else { continue };
            let lhs = trimmed[..eq_idx].trim_end();
            let rhs = trimmed[eq_idx + 1..].trim_start();
            if !rhs.starts_with("defineProps") {
                continue;
            }
            let starts_with_kw = lhs.starts_with("const ") || lhs.starts_with("let ");
            if !starts_with_kw {
                continue;
            }
            // The binding pattern starts after the keyword.
            let after_kw = lhs.split_once(' ').map(|(_, r)| r.trim_start()).unwrap_or("");
            if !after_kw.starts_with('{') {
                continue;
            }
            diags.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: i + 1,
                column: 1,
                rule_id: super::META.id.into(),
                message: "Destructuring `defineProps()` strips reactivity — keep the \
                          object and read `props.foo`."
                    .into(),
                severity: Severity::Error,
                span: None,
            });
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::ProjectCtx;
    use std::path::Path;
    use tempfile::TempDir;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("App.vue"), src))
    }

    /// Run the rule against a `.vue` file inside a tempdir whose `package.json`
    /// declares the given `vue` range, so the version gate can resolve it.
    fn run_with_vue(vue_range: &str, src: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            format!(r#"{{"dependencies":{{"vue":"{vue_range}"}}}}"#),
        )
        .unwrap();
        let vue_path = dir.path().join("App.vue");
        let project = ProjectCtx::empty();
        Check.check(&CheckCtx::for_test_with_project(&vue_path, src, &project))
    }

    /// Like `run_with_vue`, but declares ONLY a `nuxt` dependency and no `vue` —
    /// the Nuxt case where Vue 3.5+ is provided transitively.
    fn run_with_nuxt(nuxt_range: &str, src: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            format!(r#"{{"dependencies":{{"nuxt":"{nuxt_range}"}}}}"#),
        )
        .unwrap();
        let vue_path = dir.path().join("App.vue");
        let project = ProjectCtx::empty();
        Check.check(&CheckCtx::for_test_with_project(&vue_path, src, &project))
    }

    /// Declares ONLY `@vue/compiler-sfc` in devDependencies and no direct
    /// `vue`/`nuxt` — the package that implements the reactive-props-destructure
    /// transform, so its version is the most precise signal.
    fn run_with_compiler_sfc(range: &str, src: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            format!(r#"{{"devDependencies":{{"@vue/compiler-sfc":"{range}"}}}}"#),
        )
        .unwrap();
        let vue_path = dir.path().join("App.vue");
        let project = ProjectCtx::empty();
        Check.check(&CheckCtx::for_test_with_project(&vue_path, src, &project))
    }

    #[test]
    fn flags_destructured_defineprops() {
        let src = "const { foo } = defineProps<{ foo: string }>();";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_object_defineprops() {
        let src = "const props = defineProps<{ foo: string }>();";
        assert!(run(src).is_empty());
    }

    #[test]
    fn suppressed_on_vue_3_5() {
        // Regression for #3734 — Reactive Props Destructuring is stable in Vue 3.5,
        // so a destructured `defineProps` must not be flagged.
        let src = "const { items } = defineProps<{ items: string[] }>();";
        assert!(run_with_vue("^3.5.4", src).is_empty());
    }

    #[test]
    fn still_flags_below_vue_3_5() {
        // The same source under Vue < 3.5 still loses reactivity, so it warns.
        let src = "const { items } = defineProps<{ items: string[] }>();";
        assert_eq!(run_with_vue("^3.4.0", src).len(), 1);
    }

    #[test]
    fn suppressed_on_nuxt_4() {
        // Regression for #4454 — Nuxt 4 ships Vue 3.5+ transitively, so a Nuxt 4
        // project declaring only `nuxt` (no direct `vue`) must not be flagged.
        let src = "<script setup lang=\"ts\">\nconst { type } = defineProps({ type: { type: String, required: true } })\n</script>";
        assert!(run_with_nuxt("^4.4.5", src).is_empty());
    }

    #[test]
    fn suppressed_on_nuxt_3_17() {
        // Regression for #7554 — Nuxt 3.13+ bundles Vue 3.5+, so a Nuxt 3.17
        // project declaring only `nuxt` (no direct `vue`) must not be flagged.
        let src = "<script setup lang=\"ts\">\nconst { modelValue } = defineProps<{ modelValue?: number | null }>()\n</script>";
        assert!(run_with_nuxt("3.17.4", src).is_empty());
    }

    #[test]
    fn suppressed_on_compiler_sfc_3_5() {
        // Regression for #7554 — `@vue/compiler-sfc` >= 3.5 implements the
        // reactive-props-destructure transform, so a project declaring it (only in
        // devDependencies, no direct `vue`/`nuxt`) must not be flagged.
        let src = "<script setup lang=\"ts\">\nconst { modelValue } = defineProps<{ modelValue?: number | null }>()\n</script>";
        assert!(run_with_compiler_sfc("^3.5.13", src).is_empty());
    }

    #[test]
    fn still_flags_on_nuxt_3_below_3_13() {
        // Nuxt < 3.13 bundles Vue < 3.5, so the destructure still loses
        // reactivity and the warning stays.
        let src = "<script setup lang=\"ts\">\nconst { type } = defineProps({ type: { type: String, required: true } })\n</script>";
        assert_eq!(run_with_nuxt("3.10.0", src).len(), 1);
    }

    #[test]
    fn still_flags_on_compiler_sfc_below_3_5() {
        // `@vue/compiler-sfc` < 3.5 predates the transform, so the destructure
        // still loses reactivity and the warning stays.
        let src = "<script setup lang=\"ts\">\nconst { type } = defineProps({ type: { type: String, required: true } })\n</script>";
        assert_eq!(run_with_compiler_sfc("^3.4.0", src).len(), 1);
    }
}
