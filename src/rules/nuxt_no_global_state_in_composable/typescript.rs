//! nuxt-no-global-state-in-composable backend.
//!
//! Composable files (a function named `useFoo()` exported from a single
//! module) must not hold mutable module-level bindings. On the server those
//! bindings are shared between concurrent requests, leaking auth, user data,
//! and form state across users.

use crate::diagnostic::{Diagnostic, Severity};

fn is_composable_file(src: &str) -> bool {
    let nuxt = src.contains("#imports")
        || src.contains("nuxt/app")
        || src.contains("#app")
        || src.contains("useState")
        || src.contains("useRuntimeConfig")
        || src.contains("useNuxtApp");
    if !nuxt {
        return false;
    }
    src.contains("export function use") || src.contains("export const use")
}

crate::ast_check! { on ["lexical_declaration", "variable_declaration"] prefilter = ["useState"] => |node, source, ctx, diagnostics|
    if !is_composable_file(ctx.source) {
        return;
    }
    let Some(parent) = node.parent() else { return };
    if parent.kind() != "program" && parent.kind() != "export_statement" {
        return;
    }
    if parent.kind() == "export_statement" {
        if let Some(grand) = parent.parent() {
            if grand.kind() != "program" {
                return;
            }
        }
    }

    let mut cursor = node.walk();
    let mut keyword = "";
    for child in node.children(&mut cursor) {
        if child.kind() == "let" || child.kind() == "var" || child.kind() == "const" {
            keyword = child.kind();
            break;
        }
    }
    if keyword != "let" && keyword != "var" {
        return;
    }

    let _ = source;
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf().into(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "nuxt-no-global-state-in-composable".into(),
        message: format!(
            "Module-level `{keyword}` in a composable leaks across SSR requests — move inside the composable or use `useState()`."
        ),
        severity: Severity::Error,
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
    fn flags_module_level_let_in_composable() {
        let src = "import {} from '#imports';\nlet cachedUser: User | null = null;\nexport function useCurrentUser() { return cachedUser; }";
        assert!(!run_on(src).is_empty());
    }

    #[test]
    fn allows_const_at_module_level() {
        let src = "import {} from '#imports';\nconst KEY = 'user';\nexport function useCurrentUser() { return useState(KEY, () => null); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_let_inside_composable_body() {
        let src = "import {} from '#imports';\nexport function useCurrentUser() { let local = 0; return local; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_composable_files() {
        let src = "let cachedUser = null;\nfunction helper() {}";
        assert!(run_on(src).is_empty());
    }
}
