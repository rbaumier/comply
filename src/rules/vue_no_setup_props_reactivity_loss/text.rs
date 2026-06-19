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
        // so this rule does not apply. Suppress when the project provably declares
        // vue >= 3.5; otherwise (vue < 3.5, or no declared vue version) keep warning.
        if matches!(
            ctx.project.nearest_dependency_version_min(ctx.path, "vue"),
            Some(v) if v >= (3, 5)
        ) {
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
}
