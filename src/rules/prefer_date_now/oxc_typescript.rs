use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression, AstType::UnaryExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::CallExpression(call) => {
                // Pattern 1: `new Date().getTime()` / `new Date().valueOf()`
                if let Expression::StaticMemberExpression(member) = &call.callee {
                    let prop = member.property.name.as_str();
                    if (prop == "getTime" || prop == "valueOf")
                        && is_new_date_no_args(&member.object)
                    {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, call.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message:
                                "Prefer `Date.now()` over `new Date().getTime()`/`.valueOf()`."
                                    .into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                        return;
                    }
                }

                // Pattern 3: `Number(new Date())`
                if let Expression::Identifier(ident) = &call.callee
                    && ident.name.as_str() == "Number"
                        && call.arguments.len() == 1
                        && let Some(arg) = call.arguments.first()
                        && let Some(expr) = arg.as_expression()
                        && is_new_date_no_args(expr)
                    {
                        let (line, column) = byte_offset_to_line_col(
                            ctx.source,
                            call.span.start as usize,
                        );
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "Prefer `Date.now()` over `Number(new Date())`."
                                .into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
            }
            AstKind::UnaryExpression(unary) => {
                // Pattern 2: `+new Date()`
                if unary.operator == oxc_ast::ast::UnaryOperator::UnaryPlus
                    && is_new_date_no_args(&unary.argument)
                {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, unary.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "Prefer `Date.now()` over `+new Date()`.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            _ => {}
        }
    }
}

fn is_new_date_no_args(expr: &Expression) -> bool {
    let Expression::NewExpression(new_expr) = expr else {
        return false;
    };
    let Expression::Identifier(ident) = &new_expr.callee else {
        return false;
    };
    ident.name.as_str() == "Date" && new_expr.arguments.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_get_time() {
        let d = run_on("const ts = new Date().getTime();");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_value_of() {
        let d = run_on("const ts = new Date().valueOf();");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_unary_plus() {
        let d = run_on("const ts = +new Date();");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_number_coercion() {
        let d = run_on("const ts = Number(new Date());");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_date_now() {
        assert!(run_on("const ts = Date.now();").is_empty());
    }

    #[test]
    fn allows_new_date_with_args() {
        assert!(run_on("const d = new Date(2024, 0, 1).getTime();").is_empty());
    }
}
