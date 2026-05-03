//! ts-init-declarations OXC backend — flag `let`/`var` declarations
//! without an initializer, skipping `declare` and `const`.

use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::VariableDeclarationKind;

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            let AstKind::VariableDeclaration(decl) = node.kind() else {
                continue;
            };
            // Skip `const` — TS/JS already errors on uninitialized const.
            if decl.kind == VariableDeclarationKind::Const {
                continue;
            }
            // Skip `declare` contexts.
            if decl.declare {
                continue;
            }
            for declarator in &decl.declarations {
                if declarator.init.is_some() {
                    continue;
                }
                let name = match &declarator.id {
                    oxc_ast::ast::BindingPattern::BindingIdentifier(ident) => {
                        ident.name.as_str()
                    }
                    _ => "variable",
                };
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, declarator.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`{name}` is declared without initialization — \
                         assign a value at declaration."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diagnostics
    }
}
