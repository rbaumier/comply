//! prefer-native-coercion-functions OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, type_annotation_is_type_predicate};
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

        // A `x is T` / `this is T` return type narrows per call site. Replacing the
        // arrow with a bare coercion (`Boolean`/`String`/…) erases that narrowing —
        // e.g. `.filter((o): o is Opt => Boolean(o))` keeps `(Opt | undefined)[]`
        // under `.filter(Boolean)`. The predicate is the reason the arrow exists.
        if type_annotation_is_type_predicate(arrow.return_type.as_deref()) {
            return;
        }

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
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    // Regression for #3744: a `x is T` type predicate narrows per call site, which
    // a bare `Boolean` cannot reproduce — `.filter(Boolean)` keeps the element type.
    #[test]
    fn allows_type_predicate_expression_body() {
        let src = "const r = arr.filter((option): option is SyncOption => Boolean(option));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_type_predicate_block_body() {
        let src = "const r = arr.filter((x): x is Foo => { return Boolean(x); });";
        assert!(run_on(src).is_empty());
    }

    // The predicate also guards String/Number, not just Boolean.
    #[test]
    fn allows_type_predicate_string() {
        assert!(run_on("const f = (x): x is string => String(x);").is_empty());
    }

    // A plain `boolean` return type is type-preserving once rewritten to `Boolean`,
    // so this genuine redundant wrapper must still be flagged.
    #[test]
    fn flags_plain_boolean_return_type() {
        let d = run_on("const f = (x): boolean => Boolean(x);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Boolean"));
    }

    #[test]
    fn flags_unannotated_boolean_wrapper() {
        let d = run_on("const f = (x) => Boolean(x);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Boolean"));
    }

    #[test]
    fn flags_unannotated_number_wrapper() {
        let d = run_on("const f = (x) => Number(x);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Number"));
    }
}
