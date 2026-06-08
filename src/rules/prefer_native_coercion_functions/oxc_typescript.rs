//! prefer-native-coercion-functions OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const COERCION_FUNCTIONS: &[&str] = &["String", "Number", "BigInt", "Boolean", "Symbol"];

/// Extract the single parameter name from an arrow function, or None.
fn single_param_name<'a>(
    params: &'a oxc_ast::ast::FormalParameters<'a>,
) -> Option<&'a str> {
    if params.items.len() != 1 || params.rest.is_some() {
        return None;
    }
    let param = &params.items[0];
    let oxc_ast::ast::BindingPattern::BindingIdentifier(ident) = &param.pattern else {
        return None;
    };
    Some(ident.name.as_str())
}

/// Check if a call expression is `COERCION(param_name)` with exactly one arg.
fn is_coercion_call<'a>(
    expr: &'a oxc_ast::ast::Expression<'a>,
    param_name: &str,
) -> Option<&'a str> {
    let oxc_ast::ast::Expression::CallExpression(call) = expr else {
        return None;
    };
    let oxc_ast::ast::Expression::Identifier(ident) = &call.callee else {
        return None;
    };
    let func_name = ident.name.as_str();
    if !COERCION_FUNCTIONS.contains(&func_name) {
        return None;
    }
    if call.arguments.len() != 1 {
        return None;
    }
    let oxc_ast::ast::Argument::Identifier(arg_ident) = &call.arguments[0] else {
        return None;
    };
    if arg_ident.name.as_str() != param_name {
        return None;
    }
    Some(func_name)
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ArrowFunctionExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ArrowFunctionExpression(arrow) = node.kind() else {
            return;
        };

        let Some(param_name) = single_param_name(&arrow.params) else {
            return;
        };

        // Check body: either expression body or block body with single return
        let func_name = if arrow.expression {
            // Expression body: `x => Number(x)`
            let oxc_ast::ast::Statement::ExpressionStatement(expr_stmt) =
                &arrow.body.statements[0]
            else {
                return;
            };
            let Some(name) = is_coercion_call(&expr_stmt.expression, param_name) else {
                return;
            };
            name
        } else {
            // Block body: `x => { return Number(x); }`
            if arrow.body.statements.len() != 1 {
                return;
            }
            let oxc_ast::ast::Statement::ReturnStatement(ret) = &arrow.body.statements[0] else {
                return;
            };
            let Some(arg) = &ret.argument else {
                return;
            };
            let Some(name) = is_coercion_call(arg, param_name) else {
                return;
            };
            name
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, arrow.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Prefer `{func_name}` directly over wrapping it in a function. \
                 Use `.map({func_name})` instead of `.map(x => {func_name}(x))`."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_map_arrow_number() {
        let d = run_on("arr.map(x => Number(x))");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Number"));
    }


    #[test]
    fn flags_map_arrow_string_parens() {
        let d = run_on("arr.map((s) => String(s))");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("String"));
    }


    #[test]
    fn flags_map_arrow_boolean() {
        let d = run_on("arr.filter(v => Boolean(v))");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Boolean"));
    }


    #[test]
    fn flags_block_body_return() {
        let d = run_on("arr.map(x => { return Number(x); })");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_direct_usage() {
        assert!(run_on("arr.map(Number)").is_empty());
    }


    #[test]
    fn allows_different_param() {
        assert!(run_on("arr.map(x => Number(y))").is_empty());
    }


    #[test]
    fn allows_multiple_args() {
        assert!(run_on("arr.map(x => Number(x, 10))").is_empty());
    }


    #[test]
    fn flags_bigint_coercion() {
        let d = run_on("items.map(v => BigInt(v))");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("BigInt"));
    }


    #[test]
    fn allows_non_coercion_function() {
        assert!(run_on("arr.map(x => parseInt(x))").is_empty());
    }
}
