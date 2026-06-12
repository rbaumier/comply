//! OxcCheck backend for nuxt-no-global-state-in-composable.
//!
//! Flags module-level `let`/`var` in composable files. These bindings leak
//! state across SSR requests on the server.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, source_contains};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::VariableDeclarationKind;
use std::sync::Arc;

pub struct Check;

fn is_composable_file(src: &str) -> bool {
    let nuxt = source_contains(src, "#imports")
        || source_contains(src, "nuxt/app")
        || source_contains(src, "#app")
        || source_contains(src, "useState")
        || source_contains(src, "useRuntimeConfig")
        || source_contains(src, "useNuxtApp");
    if !nuxt {
        return false;
    }
    source_contains(src, "export function use") || source_contains(src, "export const use")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclaration]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["useState"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::VariableDeclaration(decl) = node.kind() else {
            return;
        };

        // Must be `let` or `var`
        let keyword = match decl.kind {
            VariableDeclarationKind::Let => "let",
            VariableDeclarationKind::Var => "var",
            _ => return,
        };

        if !is_composable_file(ctx.source) {
            return;
        }

        // Must be at program top level (possibly inside an export)
        let parent = semantic.nodes().parent_node(node.id());
        match parent.kind() {
            AstKind::Program(_) => {}
            AstKind::ExportNamedDeclaration(_) => {
                let grandparent = semantic.nodes().parent_node(parent.id());
                if !matches!(grandparent.kind(), AstKind::Program(_)) {
                    return;
                }
            }
            _ => return,
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, decl.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Module-level `{keyword}` in a composable leaks across SSR requests — move inside the composable or use `useState()`."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
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
        let src = "import {} from '#imports';\nlet cachedUser: any = null;\nexport function useCurrentUser() { return cachedUser; }";
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
