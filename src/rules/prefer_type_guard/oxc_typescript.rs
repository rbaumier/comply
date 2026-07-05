//! prefer-type-guard oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    BinaryOperator, BindingPattern, Expression, FormalParameter, TSType, UnaryOperator,
};
use oxc_span::GetSpan;
use std::sync::Arc;

/// The primitive keyword (`"string"`/`"number"`/`"boolean"`) the parameter is
/// explicitly, non-optionally annotated with, if any.
///
/// A `typeof param === '<that primitive>'` test on such a parameter narrows its
/// static type to itself, so the corresponding `param is T` predicate would be
/// vacuous. An absent, optional, `unknown`/`any`, or union annotation returns
/// `None`, because there a `typeof`/`instanceof` genuinely narrows.
fn param_primitive_keyword(param: &FormalParameter) -> Option<&'static str> {
    if param.optional {
        return None;
    }
    match &param.type_annotation.as_ref()?.type_annotation {
        TSType::TSStringKeyword(_) => Some("string"),
        TSType::TSNumberKeyword(_) => Some("number"),
        TSType::TSBooleanKeyword(_) => Some("boolean"),
        _ => None,
    }
}

/// True when some `return` statement inside `[start, end)` returns an
/// expression that meaningfully narrows `param` — `typeof param` or
/// `param instanceof X`.
///
/// A type predicate (`param is T`) narrows exactly one named parameter, so the
/// `typeof`/`instanceof` operand must be that parameter directly. A member
/// access (`typeof param.value`), an unrelated local, or a `typeof`/`instanceof`
/// used only to gate a branch (while the returns are unrelated booleans) cannot
/// be expressed as `param is T`, so they are not candidates. When
/// `vacuous_primitive` is set, a `typeof param === '<that primitive>'` test does
/// not narrow the already-primitive parameter to a subtype, so it does not count.
fn returns_a_narrowing_check(
    semantic: &oxc_semantic::Semantic,
    param: &str,
    vacuous_primitive: Option<&str>,
    start: u32,
    end: u32,
) -> bool {
    semantic.nodes().iter().any(|n| {
        let AstKind::ReturnStatement(ret) = n.kind() else { return false };
        if ret.span.start < start || ret.span.end > end {
            return false;
        }
        let Some(arg) = &ret.argument else { return false };
        expr_narrows_param(arg, param, vacuous_primitive)
    })
}

/// True when `expr` contains a `typeof param` or `param instanceof X` whose
/// operand is the identifier `param`, recursing through the boolean/grouping
/// operators a returned predicate is composed of (`&&`, `||`, `!`, parens,
/// comparisons such as `typeof param === "string"`).
///
/// A `typeof param === '<lit>'` comparison (either operand order) whose `<lit>`
/// equals `vacuous_primitive` narrows nothing — the parameter already has that
/// primitive type — so it does not make the function a type-guard candidate.
fn expr_narrows_param(expr: &Expression, param: &str, vacuous_primitive: Option<&str>) -> bool {
    match expr {
        Expression::UnaryExpression(unary) => {
            if unary.operator == UnaryOperator::Typeof
                && is_param_identifier(&unary.argument, param)
            {
                return true;
            }
            // e.g. `!(typeof param === ...)`.
            expr_narrows_param(&unary.argument, param, vacuous_primitive)
        }
        Expression::BinaryExpression(binary) => {
            if binary.operator == BinaryOperator::Instanceof
                && is_param_identifier(&binary.left, param)
            {
                return true;
            }
            // `typeof param === "<lit>"` / `"<lit>" === typeof param`: narrows
            // nothing when `<lit>` is the parameter's own declared primitive.
            if matches!(
                binary.operator,
                BinaryOperator::StrictEquality | BinaryOperator::Equality
            ) {
                let compared = if is_typeof_param(&binary.left, param) {
                    string_literal_value(&binary.right)
                } else if is_typeof_param(&binary.right, param) {
                    string_literal_value(&binary.left)
                } else {
                    None
                };
                if let Some(lit) = compared {
                    return Some(lit) != vacuous_primitive;
                }
            }
            expr_narrows_param(&binary.left, param, vacuous_primitive)
                || expr_narrows_param(&binary.right, param, vacuous_primitive)
        }
        Expression::LogicalExpression(logical) => {
            expr_narrows_param(&logical.left, param, vacuous_primitive)
                || expr_narrows_param(&logical.right, param, vacuous_primitive)
        }
        Expression::ParenthesizedExpression(paren) => {
            expr_narrows_param(&paren.expression, param, vacuous_primitive)
        }
        Expression::ConditionalExpression(cond) => {
            expr_narrows_param(&cond.test, param, vacuous_primitive)
                || expr_narrows_param(&cond.consequent, param, vacuous_primitive)
                || expr_narrows_param(&cond.alternate, param, vacuous_primitive)
        }
        _ => false,
    }
}

/// True when `expr` is exactly `typeof param` — a `typeof` whose operand is the
/// identifier `param` (not a member access of it).
fn is_typeof_param(expr: &Expression, param: &str) -> bool {
    matches!(
        expr,
        Expression::UnaryExpression(unary)
            if unary.operator == UnaryOperator::Typeof
                && is_param_identifier(&unary.argument, param)
    )
}

/// The value of `expr` when it is a string literal, else `None`.
fn string_literal_value<'a>(expr: &'a Expression) -> Option<&'a str> {
    match expr {
        Expression::StringLiteral(s) => Some(s.value.as_str()),
        _ => None,
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

        // A `typeof param === '<primitive>'` test on a parameter already declared
        // that non-optional primitive narrows nothing — the `param is T` predicate
        // would be vacuous — so such a check is not a candidate.
        let vacuous_primitive = param_primitive_keyword(&func.params.items[0]);

        // Only flag when a `return` directly yields a type check whose operand
        // is the narrowable parameter (`typeof param`, `param instanceof X`).
        let Some(body) = &func.body else { return };
        if !returns_a_narrowing_check(
            semantic,
            param,
            vacuous_primitive,
            body.span.start,
            body.span.end,
        ) {
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

    #[test]
    fn allows_typeof_matching_already_primitive_param() {
        // Regression for issue #7405: the parameter is already declared the
        // primitive being tested, so `typeof key === 'string'` narrows `string`
        // to `string` — nothing. A `key is string` predicate would be a vacuous
        // no-op; these are value validators (regex match), not type guards.
        let key = "function isValidKey(key: string): boolean { return typeof key === 'string' && REGEX.test(key); }";
        assert!(run(key).is_empty());

        let num = "function isPos(n: number): boolean { return typeof n === 'number' && n > 0; }";
        assert!(run(num).is_empty());
    }

    #[test]
    fn still_flags_typeof_on_wider_param() {
        // The `typeof` genuinely narrows when the parameter's static type is
        // wider than the tested type, so a `param is T` predicate stays
        // meaningful: `unknown`/`any`, an optional (`?` adds `undefined`), or a
        // union all keep flagging.
        let unknown = "function isTransient(err: unknown): boolean { return typeof err === 'object'; }";
        assert_eq!(run(unknown).len(), 1);

        let optional = "function isActive(x?: string): boolean { return typeof x === 'string'; }";
        assert_eq!(run(optional).len(), 1);

        let union = "function isStr(x: string | number): boolean { return typeof x === 'string'; }";
        assert_eq!(run(union).len(), 1);

        let any = "function isAny(v: any): boolean { return typeof v === 'number'; }";
        assert_eq!(run(any).len(), 1);

        // A primitive-annotated param still flags when the tested type differs
        // from its declared primitive — the `typeof` is not vacuous there.
        let mismatch = "function isNum(x: string): boolean { return typeof x === 'number'; }";
        assert_eq!(run(mismatch).len(), 1);
    }
}
