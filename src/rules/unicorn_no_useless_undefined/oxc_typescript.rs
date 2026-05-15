//! unicorn-no-useless-undefined oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

fn is_undefined_identifier(expr: &Expression) -> bool {
    matches!(expr, Expression::Identifier(id) if id.name.as_str() == "undefined")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ReturnStatement, AstType::VariableDeclarator]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["undefined"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::ReturnStatement(ret) => {
                let Some(arg) = &ret.argument else { return };
                if !is_undefined_identifier(arg) {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, ret.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`return undefined` is redundant — drop the `undefined` \
                              and let the implicit return take over."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            AstKind::VariableDeclarator(decl) => {
                let Some(init) = &decl.init else { return };
                if !is_undefined_identifier(init) {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, decl.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Explicit `= undefined` is redundant — `let x;` is already \
                              undefined."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_return_undefined() {
        let src = "function f() { return undefined; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_let_assigned_undefined() {
        let src = "let x = undefined;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_bare_return() {
        let src = "function f() { return; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_uninitialised_let() {
        let src = "let x;";
        assert!(run(src).is_empty());
    }
}
