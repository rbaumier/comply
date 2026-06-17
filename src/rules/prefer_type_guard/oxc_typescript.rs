//! prefer-type-guard oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, BindingPattern, Expression, UnaryOperator};
use oxc_span::GetSpan;
use std::sync::Arc;

/// True when some `return` statement inside `[start, end)` returns an
/// expression that type-checks `param` itself — `typeof param` or
/// `param instanceof X`.
///
/// A type predicate (`param is T`) narrows exactly one named parameter, so the
/// `typeof`/`instanceof` operand must be that parameter directly. A member
/// access (`typeof param.value`), an unrelated local, or a `typeof`/`instanceof`
/// used only to gate a branch (while the returns are unrelated booleans) cannot
/// be expressed as `param is T`, so they are not candidates.
fn returns_a_type_check(
    semantic: &oxc_semantic::Semantic,
    param: &str,
    start: u32,
    end: u32,
) -> bool {
    semantic.nodes().iter().any(|n| {
        let AstKind::ReturnStatement(ret) = n.kind() else { return false };
        if ret.span.start < start || ret.span.end > end {
            return false;
        }
        let Some(arg) = &ret.argument else { return false };
        expr_checks_param_type(arg, param)
    })
}

/// True when `expr` contains a `typeof param` or `param instanceof X` whose
/// operand is the identifier `param`, recursing through the boolean/grouping
/// operators a returned predicate is composed of (`&&`, `||`, `!`, parens,
/// comparisons such as `typeof param === "string"`).
fn expr_checks_param_type(expr: &Expression, param: &str) -> bool {
    match expr {
        Expression::UnaryExpression(unary) => {
            if unary.operator == UnaryOperator::Typeof
                && is_param_identifier(&unary.argument, param)
            {
                return true;
            }
            // e.g. `!(typeof param === ...)`.
            expr_checks_param_type(&unary.argument, param)
        }
        Expression::BinaryExpression(binary) => {
            if binary.operator == BinaryOperator::Instanceof
                && is_param_identifier(&binary.left, param)
            {
                return true;
            }
            // e.g. `typeof param === "string"` (the `typeof` is the left operand).
            expr_checks_param_type(&binary.left, param)
                || expr_checks_param_type(&binary.right, param)
        }
        Expression::LogicalExpression(logical) => {
            expr_checks_param_type(&logical.left, param)
                || expr_checks_param_type(&logical.right, param)
        }
        Expression::ParenthesizedExpression(paren) => {
            expr_checks_param_type(&paren.expression, param)
        }
        Expression::ConditionalExpression(cond) => {
            expr_checks_param_type(&cond.test, param)
                || expr_checks_param_type(&cond.consequent, param)
                || expr_checks_param_type(&cond.alternate, param)
        }
        _ => false,
    }
}

/// True when `expr` is exactly the identifier `param` (not a member access of
/// it, nor an unrelated name).
fn is_param_identifier(expr: &Expression, param: &str) -> bool {
    matches!(expr, Expression::Identifier(id) if id.name.as_str() == param)
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Function]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::Function(func) = node.kind() else {
            return;
        };

        // Must have a name starting with "is" + uppercase.
        let Some(id) = &func.id else { return };
        let name = id.name.as_str();
        if !name.starts_with("is") {
            return;
        }
        let after_is = &name[2..];
        if after_is.is_empty() || !after_is.starts_with(|c: char| c.is_ascii_uppercase()) {
            return;
        }

        // A type predicate (`x is T`) narrows exactly one named parameter, so it
        // requires exactly one simple (non-destructured, non-rest) parameter
        // bound to an identifier. Zero parameters means runtime
        // environment/feature detection (e.g. `isSafari` guarding `typeof
        // navigator` for SSR safety); destructured/rest/multiple parameters have
        // no single name to put after `is`. Bail in all those cases.
        if func.params.rest.is_some() || func.params.items.len() != 1 {
            return;
        }
        let BindingPattern::BindingIdentifier(param_id) = &func.params.items[0].pattern else {
            return;
        };
        let param = param_id.name.as_str();

        // Return type must be `: boolean` (not a type predicate).
        let Some(ret) = &func.return_type else { return };
        let rt_span = ret.span;
        let rt_text = &ctx.source[rt_span.start as usize..rt_span.end as usize];
        let rt_inner = rt_text.trim().strip_prefix(':').unwrap_or(rt_text.trim()).trim();
        if rt_inner != "boolean" {
            return;
        }

        // Only flag when a `return` directly yields a type check whose operand
        // is the narrowable parameter (`typeof param`, `param instanceof X`).
        let Some(body) = &func.body else { return };
        if !returns_a_type_check(semantic, param, body.span.start, body.span.end) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, func.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Function `isX` returns `boolean` with type checks \u{2014} use a type predicate (`x is Type`) instead.".into(),
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_returned_typeof() {
        assert_eq!(
            run("function isString(x: unknown): boolean { return typeof x === \"string\"; }").len(),
            1
        );
    }

    #[test]
    fn flags_returned_instanceof() {
        assert_eq!(
            run("function isError(x: unknown): boolean { return x instanceof Error; }").len(),
            1
        );
    }

    #[test]
    fn allows_instanceof_used_for_branching() {
        // Regression for issue #567: `instanceof` gates a branch, but the returns
        // are unrelated booleans (returns `true` for all non-ProblemErrors), so a
        // `error is ProblemError` predicate would be semantically wrong.
        let src = "function isUnexpectedError(error: Error): boolean {\n\
                   if (error instanceof ProblemError) { return error.problem.status >= 500; }\n\
                   return true;\n\
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_type_predicate() {
        assert!(
            run("function isString(x: unknown): x is string { return typeof x === \"string\"; }")
                .is_empty()
        );
    }

    #[test]
    fn allows_zero_param_env_detection() {
        // Regression for issue #2251: environment/feature-detection functions take
        // no parameter, so a type predicate (`x is T`) is impossible — `typeof`
        // guards a global (`navigator`) for SSR safety, not parameter narrowing.
        let safari = "function isSafari(): boolean {\n\
                      return typeof navigator !== 'undefined'\n\
                      ? /^((?!chrome|android).)*safari/i.test(navigator.userAgent)\n\
                      : false;\n\
                      }";
        assert!(run(safari).is_empty());

        let macos = "function isMacOS(): boolean {\n\
                     return typeof navigator !== 'undefined' ? /Mac/.test(navigator.platform) : false;\n\
                     }";
        assert!(run(macos).is_empty());
    }

    #[test]
    fn still_flags_param_typeof() {
        // Negative-space guard: a genuine type guard whose parameter IS the
        // `typeof` operand must still be flagged (should be `x is string`).
        assert_eq!(
            run("function isString(x: unknown): boolean { return typeof x === \"string\"; }").len(),
            1
        );
    }

    #[test]
    fn allows_typeof_on_member_access() {
        // Regression for issue #3954: the `typeof` operand is `node.value`
        // (a *member* of the param), not `node`. There is no nameable TS type
        // for "a Literal whose `.value` is a string", so `node is T` is
        // impossible.
        let src = "function isStringOrTemplateLiteral(node): boolean {\n\
                   return node.type === 'Literal' && typeof node.value === 'string';\n\
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_destructured_param() {
        // A type predicate narrows exactly one *named* parameter; a destructured
        // `{ a }` param has no single name to put after `is`.
        let src = "function isInterpolation({ a }): boolean { return typeof a.value === 'string'; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_multi_param_relational_check() {
        // Two parameters and a relational check between members of each: no
        // single parameter is narrowed.
        let src = "function isLengthExpression(a, b): boolean { return typeof a.value === typeof b.value; }";
        assert!(run(src).is_empty());
    }
}
