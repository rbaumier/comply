//! nuxt-no-manual-imports backend.

use crate::diagnostic::{Diagnostic, Severity};

fn is_nuxt_source(src: &str) -> bool {
    src.contains("#imports")
        || src.contains("nuxt/app")
        || src.contains("#app")
        || src.contains("defineNuxtConfig")
        || src.contains("defineNuxtPlugin")
        || src.contains("defineNuxtRouteMiddleware")
}

fn module_source<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let source_node = node.child_by_field_name("source")?;
    let raw = source_node.utf8_text(source).ok()?;
    Some(raw.trim_matches(|c| c == '"' || c == '\''))
}

crate::ast_check! { on ["import_statement"] => |node, source, ctx, diagnostics|
    if !is_nuxt_source(ctx.source) {
        return;
    }
    let Some(module) = module_source(node, source) else { return };
    if module != "#imports" && module != "#app" && module != "nuxt/app" {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf().into(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "nuxt-no-manual-imports".into(),
        message: "Nuxt auto-imports composables from `#imports`/`#app` — drop the explicit import.".into(),
        severity: Severity::Warning,
        span: Some((node.byte_range().start, node.byte_range().len())),
    });
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_imports_from_pound_imports() {
        let src = "import { useRuntimeConfig } from '#imports';\nconst cfg = useRuntimeConfig();\nconst plugin = defineNuxtPlugin(() => {});";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_imports_from_pound_app() {
        let src = "import { useNuxtApp } from '#app';\nconst plugin = defineNuxtPlugin(() => {});";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_other_imports_in_nuxt_file() {
        let src = "import { ref } from 'vue';\nconst plugin = defineNuxtPlugin(() => {});";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_nuxt_files() {
        // No Nuxt markers at all (no `#imports`, no `defineNuxtPlugin`, etc.).
        let src = "import { foo } from 'lodash';\nconst x = foo();";
        assert!(run_on(src).is_empty());
    }
}
