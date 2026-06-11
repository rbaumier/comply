//! no-let oxc backend — flag `let` declarations.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclaration]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["let"])
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
        if decl.kind != oxc_ast::ast::VariableDeclarationKind::Let {
            return;
        }
        // Exempt uninitialised module-scope `let` in test files — the standard
        // pattern for state variables assigned inside beforeAll/beforeEach.
        if ctx.file.path_segments.in_test_dir
            && node.scope_id() == semantic.scoping().root_scope_id()
            && decl.declarations.iter().all(|d| d.init.is_none())
        {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, decl.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`let` creates a mutable binding — use `const` instead.".into(),
            severity: Severity::Warning,
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
    use crate::rules::test_helpers::{run_rule, run_rule_gated};

    fn run(src: &str) -> Vec<Diagnostic> {
        run_rule(&Check, src, "t.ts")
    }

    fn run_spec(src: &str) -> Vec<Diagnostic> {
        run_rule_gated(&Check, src, "t.spec.ts")
    }

    #[test]
    fn flags_let_with_initializer_non_test() {
        assert_eq!(run("let x = 1;").len(), 1);
    }

    #[test]
    fn ignores_const() {
        assert!(run("const x = 1;").is_empty());
    }

    #[test]
    fn flags_uninit_let_in_non_test_file() {
        // Outside test files, uninitialised let at module scope is still flagged.
        assert_eq!(run("let x: number;").len(), 1);
    }

    #[test]
    fn ignores_uninit_module_scope_let_in_spec_file() {
        // Regression for #986 — beforeAll/beforeEach deferred assignment pattern.
        assert!(run_spec("let betaCommunity: CommunityView | undefined;").is_empty());
    }

    #[test]
    fn flags_init_let_in_spec_file() {
        // Has initialiser → can be const → still flagged.
        assert_eq!(run_spec("let x = 1;").len(), 1);
    }

    #[test]
    fn flags_let_inside_function_in_spec_file() {
        // Inside a function scope, not module scope → still flagged.
        assert_eq!(run_spec("function f() { let x = 1; }").len(), 1);
    }
}
