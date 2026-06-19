//! zod-transform-requires-pipe oxc backend — flag a single-argument
//! `.transform(fn)` schema modifier that is not chained with `.pipe(z.*)`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Argument, ArrowFunctionExpression, BindingPattern, Expression, LogicalOperator, Statement,
};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Method names that yield a constructively-typed result — re-validating
/// their output with `.pipe(z.string())` (or similar) would be a redundant
/// re-check of a value the function provably emits with the right type.
///
/// All of these return a string or number from a known input type, and
/// either can't fail OR throw on bad input rather than silently producing
/// the wrong type.
const TYPED_OUTPUT_METHODS: &[&str] = &[
    "toISOString",
    "toString",
    "toLocaleString",
    "toFixed",
    "toPrecision",
    "toExponential",
    "valueOf",
    "toJSON",
    "toLowerCase",
    "toUpperCase",
];

/// Casts / serialisers whose return type is fixed by the function itself.
const TYPED_OUTPUT_CALLEES: &[&str] = &[
    "String",
    "Number",
    "Boolean",
    "BigInt",
];

/// True if the transform callback's body produces a value whose runtime
/// type is constructively known. `.pipe(z.*)` re-validation adds no
/// safety in that case — the value's shape comes from the function call,
/// not from external input.
fn body_returns_typed_value(arrow_arg: &Argument) -> bool {
    let arrow = match arrow_arg {
        Argument::ArrowFunctionExpression(a) => a,
        _ => return false,
    };
    let expr = if arrow.expression {
        match arrow.body.statements.first() {
            Some(Statement::ExpressionStatement(es)) => &es.expression,
            _ => return false,
        }
    } else {
        // Single return statement only.
        let returns: Vec<&Expression> = arrow
            .body
            .statements
            .iter()
            .filter_map(|s| {
                if let Statement::ReturnStatement(ret) = s {
                    ret.argument.as_ref()
                } else {
                    None
                }
            })
            .collect();
        if returns.len() != 1 {
            return false;
        }
        returns[0]
    };
    expression_yields_typed_value(expr) || is_default_substitution(expr, arrow)
}

/// The simple identifier name of the arrow's first parameter, if any
/// (`v` in `v => ...`). Destructured or absent params yield `None`.
fn arrow_first_param_name<'a>(arrow: &'a ArrowFunctionExpression<'a>) -> Option<&'a str> {
    match &arrow.params.items.first()?.pattern {
        BindingPattern::BindingIdentifier(id) => Some(id.name.as_str()),
        _ => None,
    }
}

/// True for the `param ?? <default>` idiom that substitutes a default for the
/// `null` case of a `nullable().default(null)` schema. The non-null branch is
/// the already-validated input value itself (`param`), so `.pipe(z.*)` would
/// re-validate a value the schema already constrained. The fallback must be a
/// constructively-typed value or a named/route-level default constant.
fn is_default_substitution(expr: &Expression, arrow: &ArrowFunctionExpression) -> bool {
    let Expression::LogicalExpression(logical) = expr else {
        return false;
    };
    if logical.operator != LogicalOperator::Coalesce {
        return false;
    }
    let Expression::Identifier(left) = &logical.left else {
        return false;
    };
    if Some(left.name.as_str()) != arrow_first_param_name(arrow) {
        return false;
    }
    expression_yields_typed_value(&logical.right)
        || matches!(
            logical.right,
            Expression::Identifier(_) | Expression::StaticMemberExpression(_)
        )
}

fn expression_yields_typed_value(expr: &Expression) -> bool {
    // Literals are obviously typed.
    if matches!(
        expr,
        Expression::StringLiteral(_)
            | Expression::NumericLiteral(_)
            | Expression::BooleanLiteral(_)
            | Expression::TemplateLiteral(_)
    ) {
        return true;
    }
    // Calls: either `x.toISOString()`-style or `String(x)`-style.
    if let Expression::CallExpression(call) = expr {
        match &call.callee {
            Expression::StaticMemberExpression(member) => {
                let method = member.property.name.as_str();
                if TYPED_OUTPUT_METHODS.contains(&method) {
                    return true;
                }
                // `JSON.stringify(...)` / `JSON.parse(...)` etc.
                if let Expression::Identifier(obj) = &member.object
                    && obj.name.as_str() == "JSON"
                    && matches!(method, "stringify")
                {
                    return true;
                }
            }
            Expression::Identifier(id) => {
                let name = id.name.as_str();
                if TYPED_OUTPUT_CALLEES.contains(&name) {
                    return true;
                }
            }
            _ => {}
        }
    }
    let _ = (expr.span(),);
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["transform"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Callee must be a member expression with property `transform`
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "transform" {
            return;
        }

        // Zod's schema `.transform(fn)` takes exactly ONE argument: the transform
        // callback. A call with a different argument count is the parse engine
        // invoking a stored transform *function* with the runtime value and parse
        // context (`def.transform(value, ctx)` / `effect.transform(data, ctx)`),
        // not a user-land schema modifier — there is no `.pipe()` to add.
        if call.arguments.len() != 1 {
            return;
        }

        // Check if the parent is a member expression with property `pipe`
        // (i.e. `.transform(fn).pipe(...)`)
        let parent = semantic.nodes().parent_node(node.id());
        if let AstKind::StaticMemberExpression(parent_member) = parent.kind()
            && parent_member.property.name.as_str() == "pipe" {
                return;
            }

        // Re-validation is redundant when the transform's body produces a
        // constructively-typed value (e.g. `d => d.toISOString()` always
        // yields a string, `n => n.toFixed(2)` always yields a string).
        if let Some(arg) = call.arguments.first()
            && body_returns_typed_value(arg)
        {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.transform()` output is not re-validated — chain `.pipe(z.*)` to assert the output schema.".into(),
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_transform_without_pipe() {
        let src = "const s = z.string().transform(x => parseRich(x));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_transform_with_pipe() {
        let src = "const s = z.string().transform(x => parseRich(x)).pipe(z.object({}));";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_transform_returning_iso_string() {
        // Regression for rbaumier/comply#20.
        let src = "const s = z.date().transform(d => d.toISOString());";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_transform_returning_string_cast() {
        let src = "const s = z.number().transform(n => String(n));";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_transform_returning_json_stringify() {
        let src = "const s = z.unknown().transform(o => JSON.stringify(o));";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_transform_nullish_default_named_const() {
        // Regression for rbaumier/comply#4199: `nullable().default(null)` with
        // a transform that substitutes the null case for a route-level default.
        let src = "const s = sortLiteral.nullable().default(null).transform(v => v ?? defaultSort);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_transform_nullish_default_member() {
        let src = "const s = sortLiteral.nullable().default(null).transform(v => v ?? defaults.sort);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_transform_nullish_default_literal() {
        let src = "const s = sortLiteral.nullable().default(null).transform(v => v ?? \"asc\");";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_transform_nullish_left_not_param() {
        // The left operand is an arbitrary call, not the validated input value,
        // so the output is not guaranteed and must still be flagged.
        let src = "const s = z.string().transform(v => parseRich(v) ?? fallback);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_transform_nullish_left_other_identifier() {
        // The left operand is an outer binding, not the validated param, so the
        // output is not guaranteed and must still be flagged.
        let src = "const s = z.string().transform(v => outer ?? fallback);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_internal_def_transform_invocation() {
        // Regression for rbaumier/comply#4449: the Zod parse engine invoking a
        // stored transform function with the runtime value AND the parse context
        // (two args) is not a user-land schema modifier.
        let src = "const _out = def.transform(payload.value, payload);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_internal_effect_transform_invocation() {
        // Regression for rbaumier/comply#4449: `ZodEffects` parse logic invoking
        // the stored transform with the data and check context (two args).
        let src = "const processed = effect.transform(ctx.data, checkCtx);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_transform_with_named_callback_reference() {
        // A single function-reference argument is still a real schema transform
        // (one arg), so it must stay flagged — the fix keys on argument COUNT,
        // not on the argument being an inline arrow.
        let src = "const s = schema.transform(namedFn);";
        assert_eq!(run(src).len(), 1);
    }
}
