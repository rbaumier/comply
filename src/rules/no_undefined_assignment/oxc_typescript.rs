//! no-undefined-assignment oxc backend — flag `= undefined` assignments.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclarator, AstType::AssignmentExpression]
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
        let (is_undefined, span_start) = match node.kind() {
            AstKind::VariableDeclarator(decl) => {
                let Some(init) = &decl.init else { return };
                let is_undef =
                    matches!(init, Expression::Identifier(id) if id.name.as_str() == "undefined");
                (is_undef, decl.span.start)
            }
            AstKind::AssignmentExpression(assign) => {
                let is_undef = matches!(
                    &assign.right,
                    Expression::Identifier(id) if id.name.as_str() == "undefined"
                );
                (is_undef, assign.span.start)
            }
            _ => return,
        };

        if !is_undefined {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Do not assign `undefined` \u{2014} use `let x;` or `delete obj.prop` instead."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    #[test]
    fn flags_let_undefined() {
        let d = crate::rules::test_helpers::run_oxc_ts("let x = undefined;", &Check);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-undefined-assignment");
    }


    #[test]
    fn flags_reassignment_undefined() {
        let d = crate::rules::test_helpers::run_oxc_ts("x = undefined;", &Check);
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_comparison_equals() {
        let d = crate::rules::test_helpers::run_oxc_ts("if (x == undefined) {}", &Check);
        assert!(d.is_empty());
    }


    #[test]
    fn allows_strict_comparison() {
        let d = crate::rules::test_helpers::run_oxc_ts("if (x === undefined) {}", &Check);
        assert!(d.is_empty());
    }
}
