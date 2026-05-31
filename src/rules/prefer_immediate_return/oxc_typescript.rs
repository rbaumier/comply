//! prefer-immediate-return OXC backend — flag `const x = expr; return x;`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, Statement};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::FunctionBody]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::FunctionBody(body) = node.kind() else {
            return;
        };

        let stmts = &body.statements;
        if stmts.len() < 2 {
            return;
        }

        for window in stmts.windows(2) {
            let decl_stmt = &window[0];
            let ret_stmt = &window[1];

            // First must be a variable declaration
            let Statement::VariableDeclaration(var_decl) = decl_stmt else {
                continue;
            };

            // Only single declarators
            if var_decl.declarations.len() != 1 {
                continue;
            }
            let declarator = &var_decl.declarations[0];

            // An explicit type annotation makes the variable a typed boundary,
            // not a redundant alias: `const x: unknown = Reflect.get(...)`
            // narrows the RHS's `any` to `unknown` without a type assertion.
            // Inlining would force `as unknown` (no-type-assertion) or leak
            // `any` (no-unsafe-return) — the variable carries the annotation
            // those rules require. Don't flag it. (Closes #656)
            if declarator.type_annotation.is_some() {
                continue;
            }

            // Must be a simple identifier (not destructuring)
            let oxc_ast::ast::BindingPattern::BindingIdentifier(ref binding) = declarator.id
            else {
                continue;
            };
            let var_name = binding.name.as_str();

            // Next must be a return statement
            let Statement::ReturnStatement(ret) = ret_stmt else {
                continue;
            };

            // Return value must be an identifier matching the declared name
            let Some(Expression::Identifier(ret_id)) = &ret.argument else {
                continue;
            };
            if ret_id.name.as_str() != var_name {
                continue;
            }

            let (line, column) =
                byte_offset_to_line_col(ctx.source, var_decl.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Variable `{var_name}` is assigned and immediately \
                     returned — return the expression directly."
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
    use crate::rules::test_helpers::run_oxc_ts;

    #[test]
    fn flags_untyped_assign_then_return() {
        let d = run_oxc_ts("function f() { const result = computeValue(); return result; }", &Check);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_typed_intermediate_variable_issue_656() {
        // `const x: unknown = Reflect.*(...)` narrows the RHS's `any` to
        // `unknown` without a type assertion — the annotation is the type
        // safety mechanism, not a redundant alias.
        let d = run_oxc_ts(
            "function trap(source, args) { const proxyResult: unknown = Reflect.apply(source, null, args); return proxyResult; }",
            &Check,
        );
        assert!(d.is_empty(), "got {d:?}");
    }
}
