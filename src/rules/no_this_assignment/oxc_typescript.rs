//! no-this-assignment OXC backend — flag `const self = this` and `self = this`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclaration, AstType::AssignmentExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::VariableDeclaration(decl) => {
                for declarator in decl.declarations.iter() {
                    let Some(init) = &declarator.init else { continue };
                    if !matches!(init, Expression::ThisExpression(_)) {
                        continue;
                    }
                    let oxc_ast::ast::BindingPattern::BindingIdentifier(id) = &declarator.id else {
                        continue;
                    };
                    let var_name = id.name.as_str();
                    let (line, column) = byte_offset_to_line_col(ctx.source, declarator.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!("Do not assign `this` to `{var_name}`. Use an arrow function instead."),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            AstKind::AssignmentExpression(assign) => {
                if !matches!(&assign.right, Expression::ThisExpression(_)) {
                    return;
                }
                let oxc_ast::ast::AssignmentTarget::AssignmentTargetIdentifier(id) = &assign.left else {
                    return;
                };
                let var_name = id.name.as_str();
                let (line, column) = byte_offset_to_line_col(ctx.source, assign.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!("Do not assign `this` to `{var_name}`. Use an arrow function instead."),
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
    fn flags_const_self_equals_this() {
        let d = run_on("const self = this;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("self"));
    }


    #[test]
    fn flags_let_that_equals_this() {
        let d = run_on("let that = this;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("that"));
    }


    #[test]
    fn flags_assignment_expression() {
        let d = run_on("let x; x = this;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`x`"));
    }


    #[test]
    fn allows_normal_assignment() {
        assert!(run_on("const x = 42;").is_empty());
    }


    #[test]
    fn allows_this_member_access() {
        assert!(run_on("const x = this.foo;").is_empty());
    }


    #[test]
    fn flags_var_this_equals_this() {
        let d = run_on("var _this = this;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("_this"));
    }
}
