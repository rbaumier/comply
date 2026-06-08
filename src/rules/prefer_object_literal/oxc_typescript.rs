//! prefer-object-literal oxc backend — flag `new Object()` and `Object.create(null)`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression, AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            // `new Object()`
            AstKind::NewExpression(new_expr) => {
                let Expression::Identifier(id) = &new_expr.callee else { return };
                if id.name.as_str() != "Object" {
                    return;
                }
                let (line, column) = byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Use `{}` instead of `new Object()`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            // `Object.create(null)`
            AstKind::CallExpression(call) => {
                let Expression::StaticMemberExpression(member) = &call.callee else { return };
                let Expression::Identifier(obj) = &member.object else { return };
                if obj.name.as_str() != "Object" || member.property.name.as_str() != "create" {
                    return;
                }
                if call.arguments.len() != 1 {
                    return;
                }
                let Some(Expression::NullLiteral(_)) = call.arguments[0].as_expression() else { return };
                let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Prefer an object literal over `Object.create(null)`.".into(),
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



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_new_object() {
        let d = run_on("const obj = new Object();");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("new Object()"));
    }


    #[test]
    fn flags_object_create_null() {
        let d = run_on("const obj = Object.create(null);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Object.create(null)"));
    }


    #[test]
    fn allows_object_literal() {
        assert!(run_on("const obj = {};").is_empty());
    }


    #[test]
    fn allows_object_create_with_prototype() {
        assert!(run_on("const obj = Object.create(proto);").is_empty());
    }
}
