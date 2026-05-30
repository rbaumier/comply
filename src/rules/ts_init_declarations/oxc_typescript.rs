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
            // Skip `declare` contexts, including `var` inside `declare global`
            // / `declare module` blocks, which are ambient type-level bindings.
            if decl.declare
                || crate::oxc_helpers::is_in_ambient_declaration(node.id(), semantic)
            {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(s, &Check)
    }

    #[test]
    fn no_fp_on_var_in_declare_global() {
        // `var` inside `declare global` is an ambient type-level binding —
        // it never has an initializer and must not be flagged. (Closes #339)
        assert!(
            run("declare global {\n  var BASE_UI_ANIMATIONS_DISABLED: boolean;\n}\nexport {};")
                .is_empty()
        );
    }

    #[test]
    fn still_flags_uninitialized_let_at_runtime() {
        assert_eq!(run("let x: number;").len(), 1);
    }
}
