//! OxcCheck backend for next-no-hardcoded-revalidate-zero.
//!
//! Flags top-level `export const revalidate = 0;` in Next.js app router
//! segment config files.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::project::Framework;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if ctx.project.framework != Framework::NextJs {
            return;
        }

        let AstKind::VariableDeclaration(decl) = node.kind() else {
            return;
        };

        // Must be `const`
        if !decl.kind.is_const() {
            return;
        }

        // Must be at program top-level inside an export
        let parent = semantic.nodes().parent_node(node.id());
        let AstKind::ExportNamedDeclaration(_) = parent.kind() else {
            return;
        };
        let grandparent = semantic.nodes().parent_node(parent.id());
        if !matches!(grandparent.kind(), AstKind::Program(_)) {
            return;
        }

        for declarator in &decl.declarations {
            let Some(name) = declarator.id.get_identifier() else {
                continue;
            };
            if name != "revalidate" {
                continue;
            }
            let Some(init) = &declarator.init else {
                continue;
            };
            if let Expression::NumericLiteral(lit) = init {
                if lit.value != 0.0 {
                    continue;
                }
            } else {
                continue;
            }

            let (line, column) =
                byte_offset_to_line_col(ctx.source, declarator.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Replace `export const revalidate = 0` with `export const dynamic = 'force-dynamic'`.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;
    use crate::project::{Framework, ProjectCtx};

    fn next_project() -> ProjectCtx {
        let mut project = ProjectCtx::empty();
        project.framework = Framework::NextJs;
        project
    }

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx_with_project(source, &Check, &next_project())
    }

    #[test]
    fn flags_revalidate_zero() {
        assert_eq!(run("export const revalidate = 0;").len(), 1);
    }

    #[test]
    fn allows_revalidate_60() {
        assert!(run("export const revalidate = 60;").is_empty());
    }

    #[test]
    fn allows_dynamic_force_dynamic() {
        assert!(run("export const dynamic = 'force-dynamic';").is_empty());
    }

    #[test]
    fn allows_local_revalidate_zero() {
        assert!(run("function f() { const revalidate = 0; return revalidate; }").is_empty());
    }
}
