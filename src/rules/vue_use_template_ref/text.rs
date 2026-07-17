//! vue-use-template-ref AST backend.
//!
//! Detects `const NAME = ref(null)` where NAME is used as a template `ref="NAME"`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["component"] => |node, source, ctx, diagnostics|
    let _ = source;
    // `useTemplateRef` was introduced in Vue 3.5.0. Suppress when the nearest
    // package.json provably declares a Vue floor below 3.5: the `ref(null)` +
    // `ref="…"` form is required there and `useTemplateRef` would not exist.
    // Fire when the declared floor is >= 3.5, or when no `vue` range is declared
    // at all.
    if matches!(
        ctx.project.nearest_dependency_version_min(ctx.path, "vue"),
        Some(v) if v < (3, 5)
    ) {
        return;
    }
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
                path: std::sync::Arc::clone(&ctx.path_arc),
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
    use crate::project::ProjectCtx;
    use crate::rules::backend::{AstCheck, CheckCtx};
    use std::path::Path;
    use tempfile::TempDir;

    fn parse(source: &str) -> tree_sitter::Tree {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_vue_updated::language())
            .expect("vue grammar");
        parser.parse(source, None).expect("parser")
    }

    fn run(source: &str) -> Vec<Diagnostic> {
        let tree = parse(source);
        Check.check(&CheckCtx::for_test(Path::new("t.vue"), source), &tree)
    }

    /// Run the rule against a `.vue` file inside a tempdir whose `package.json`
    /// is `pkg_json`, so the Vue version gate can resolve the declared range.
    fn run_with_pkg(pkg_json: &str, source: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("package.json"), pkg_json).unwrap();
        let vue_path = dir.path().join("t.vue");
        let project = ProjectCtx::empty();
        let tree = parse(source);
        Check.check(
            &CheckCtx::for_test_with_project(&vue_path, source, &project),
            &tree,
        )
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

    const SEGMENTED_SFC: &str = "<script setup lang=\"ts\">\nconst segmentedRef = ref<HTMLElement | null>(null)\n</script>\n<template>\n<div ref=\"segmentedRef\"></div>\n</template>";

    #[test]
    fn suppressed_when_vue_floor_below_3_5_peer_dep() {
        // Regression for #7634 — a library declaring `vue: ^3.3.7` supports Vue
        // < 3.5, where `useTemplateRef` does not exist, so the `ref(null)` +
        // `ref="…"` form is required and must not be flagged.
        assert!(run_with_pkg(r#"{"peerDependencies":{"vue":"^3.3.7"}}"#, SEGMENTED_SFC).is_empty());
    }

    #[test]
    fn suppressed_when_vue_floor_below_3_5_dependency() {
        // Same gate via `dependencies` — a declared `vue: ^3.4.0` floor is below
        // 3.5, so the suggestion is suppressed.
        assert!(run_with_pkg(r#"{"dependencies":{"vue":"^3.4.0"}}"#, SEGMENTED_SFC).is_empty());
    }

    #[test]
    fn still_flags_when_vue_floor_at_least_3_5() {
        // A declared `vue: ^3.5.13` floor guarantees `useTemplateRef`, so the
        // suggestion stays.
        assert_eq!(
            run_with_pkg(r#"{"dependencies":{"vue":"^3.5.13"}}"#, SEGMENTED_SFC).len(),
            1
        );
    }

    #[test]
    fn still_flags_when_no_vue_declared() {
        // No `vue` range declared — following the react-version gate's precedent,
        // the rule keeps firing rather than assuming an unsupported floor.
        assert_eq!(
            run_with_pkg(r#"{"dependencies":{"lodash":"^4.0.0"}}"#, SEGMENTED_SFC).len(),
            1
        );
    }
}
