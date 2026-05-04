//! top-level-function OxcCheck backend — flag top-level
//! `const foo = () => {...}` arrow functions.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
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
        let AstKind::VariableDeclaration(decl) = node.kind() else { return };

        // Must be at top level: parent is Program, or parent is
        // ExportNamedDeclaration whose parent is Program.
        let parent = semantic.nodes().parent_node(node.id());
        let is_top_level = match parent.kind() {
            AstKind::Program(_) => true,
            AstKind::ExportNamedDeclaration(_) => {
                let gp = semantic.nodes().parent_node(parent.id());
                matches!(gp.kind(), AstKind::Program(_))
            }
            _ => false,
        };
        if !is_top_level {
            return;
        }

        for declarator in &decl.declarations {
            let Some(init) = &declarator.init else { continue };
            if !matches!(init, oxc_ast::ast::Expression::ArrowFunctionExpression(_)) {
                continue;
            }

            let name = match &declarator.id {
                oxc_ast::ast::BindingPattern::BindingIdentifier(id) => id.name.as_str(),
                _ => "<unknown>",
            };

            let (line, column) =
                byte_offset_to_line_col(ctx.source, declarator.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Top-level `const {name} = () => ...` — prefer `function {name}(...) {{ ... }}` \
                     for a named binding, hoisting, and better stack traces."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
