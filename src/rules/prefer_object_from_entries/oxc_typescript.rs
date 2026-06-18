//! prefer-object-from-entries OXC backend — flag `.reduce(…, {})` building objects.
//!
//! Only the canonical single-entry accumulator is flagged: a reducer that adds
//! exactly one `[key, value]` entry per source element, which `Object.fromEntries`
//! can express directly. Reducers that fan out (a nested loop writing a variable
//! number of keys) or merge whole objects (`Object.assign(acc, obj)`, spread of a
//! non-accumulator object) have no equivalent `fromEntries` rewrite and are left
//! alone.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Argument, AssignmentOperator, AssignmentTarget, BindingPattern, Expression, FormalParameters,
    FunctionBody, ObjectPropertyKind, Statement,
};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["reduce"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Must be a `.reduce(` call.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "reduce" {
            return;
        }

        // Must have 2 arguments: callback and initial value.
        if call.arguments.len() != 2 {
            return;
        }

        // The seed (second argument) must be an empty object. `{} as Record<…>`
        // and parenthesized forms unwrap to the bare `{}`.
        if !seed_is_empty_object(&call.arguments[1]) {
            return;
        }

        // The callback (first argument) must be the canonical single-entry
        // accumulator. Any other shape (nested loop, merge, conditional write)
        // has no `fromEntries` rewrite and is not flagged.
        if !is_single_entry_accumulator(&call.arguments[0]) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Prefer `Object.fromEntries()` over `Array#reduce()` to build an object."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// True when `arg` is an empty `{}` or `Object.create(null)` seed. A typed seed
/// such as `{} as Record<string, number>` unwraps through the `as`/parenthesized
/// layers before the check.
fn seed_is_empty_object(arg: &Argument) -> bool {
    let Some(expr) = arg.as_expression() else {
        return false;
    };
    match expr.get_inner_expression() {
        // `{}`
        Expression::ObjectExpression(obj) => obj.properties.is_empty(),
        // `Object.create(null)`
        Expression::CallExpression(inner_call) => {
            let Expression::StaticMemberExpression(m) = &inner_call.callee else {
                return false;
            };
            let Expression::Identifier(obj) = &m.object else {
                return false;
            };
            obj.name.as_str() == "Object"
                && m.property.name.as_str() == "create"
                && inner_call.arguments.len() == 1
                && matches!(inner_call.arguments.first(), Some(Argument::NullLiteral(_)))
        }
        _ => false,
    }
}

/// True when `arg` is a reducer callback that adds exactly one entry to the
/// accumulator per element — the only shape `Object.fromEntries` can express.
///
/// Accepted shapes (with `acc` the first parameter):
/// - block body `{ acc[key] = value; return acc; }` — exactly one computed or
///   static assignment whose object is `acc`, followed by `return acc;`;
/// - concise body `(acc, x) => ({ ...acc, [key]: value })` — an object literal
///   with a single spread of `acc` plus exactly one added property.
///
/// Every other body (nested loop, `Object.assign`/spread merge, multiple or
/// conditional writes, returning something other than `acc`) is rejected.
fn is_single_entry_accumulator(arg: &Argument) -> bool {
    let Some(expr) = arg.as_expression() else {
        return false;
    };
    let (params, body): (&FormalParameters, &FunctionBody) = match expr {
        Expression::ArrowFunctionExpression(arrow) => (&arrow.params, &arrow.body),
        Expression::FunctionExpression(func) => {
            let Some(body) = &func.body else { return false };
            (&func.params, body)
        }
        _ => return false,
    };

    let Some(acc) = first_param_name(params) else {
        return false;
    };

    // A concise arrow body is a single `ExpressionStatement` wrapping the
    // returned expression; a block body holds the explicit statements.
    let [Statement::ExpressionStatement(stmt)] = body.statements.as_slice() else {
        return is_block_accumulator(&body.statements, acc);
    };
    is_spread_object_entry(&stmt.expression, acc)
}

/// True for a block body that is exactly `acc[key] = value; return acc;`.
fn is_block_accumulator(statements: &[Statement], acc: &str) -> bool {
    let [Statement::ExpressionStatement(assign_stmt), Statement::ReturnStatement(ret)] = statements
    else {
        return false;
    };

    // First statement: a single `=` assignment whose target is `acc[...]` or `acc.x`.
    let Expression::AssignmentExpression(assign) = &assign_stmt.expression else {
        return false;
    };
    if assign.operator != AssignmentOperator::Assign {
        return false;
    }
    if !assignment_target_object_is(&assign.left, acc) {
        return false;
    }

    // Second statement: `return acc;`.
    matches!(&ret.argument, Some(Expression::Identifier(id)) if id.name.as_str() == acc)
}

/// True when the assignment target is a member access (`acc[k]` or `acc.x`)
/// whose object is the identifier `acc`.
fn assignment_target_object_is(target: &AssignmentTarget, acc: &str) -> bool {
    let object = match target {
        AssignmentTarget::ComputedMemberExpression(m) => &m.object,
        AssignmentTarget::StaticMemberExpression(m) => &m.object,
        _ => return false,
    };
    matches!(object, Expression::Identifier(id) if id.name.as_str() == acc)
}

/// True for a concise body `({ ...acc, [key]: value })`: an object literal with
/// exactly one spread of `acc` and exactly one added property.
fn is_spread_object_entry(expr: &Expression, acc: &str) -> bool {
    let Expression::ObjectExpression(obj) = expr.get_inner_expression() else {
        return false;
    };
    if obj.properties.len() != 2 {
        return false;
    }
    let mut spreads_acc = false;
    let mut added_props = 0u32;
    for prop in &obj.properties {
        match prop {
            ObjectPropertyKind::SpreadProperty(spread) => {
                // The spread must be of the accumulator, not a foreign object.
                if !matches!(&spread.argument, Expression::Identifier(id) if id.name.as_str() == acc)
                {
                    return false;
                }
                spreads_acc = true;
            }
            ObjectPropertyKind::ObjectProperty(_) => added_props += 1,
        }
    }
    spreads_acc && added_props == 1
}

/// Name of the reducer's first (accumulator) parameter when it is a plain
/// identifier. Destructured or rest accumulators are not the canonical shape.
fn first_param_name<'a>(params: &'a FormalParameters) -> Option<&'a str> {
    let first = params.items.first()?;
    let BindingPattern::BindingIdentifier(id) = &first.pattern else {
        return None;
    };
    Some(id.name.as_str())
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

    #[test]
    fn flags_concise_spread_accumulator() {
        let d = run_on("const obj = pairs.reduce((acc, [k, v]) => ({ ...acc, [k]: v }), {});");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_block_accumulator() {
        let d = run_on(
            "const obj = pairs.reduce((acc, [k, v]) => { acc[k] = v; return acc; }, {});",
        );
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_block_accumulator_static_member() {
        let d =
            run_on("const obj = items.reduce((acc, x) => { acc.id = x; return acc; }, {});");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_block_accumulator_with_object_create_null() {
        let d = run_on(
            "const obj = pairs.reduce((acc, [k, v]) => { acc[k] = v; return acc; }, Object.create(null));",
        );
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_typed_empty_object_seed() {
        // `{} as Record<…>` unwraps to the empty `{}` seed.
        let d = run_on(
            "const obj = pairs.reduce((acc, [k, v]) => { acc[k] = v; return acc; }, {} as Record<string, number>);",
        );
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_nested_foreach_fan_out() {
        // One element writes a variable number of keys — needs a flatMap first,
        // no clean `fromEntries` rewrite.
        assert!(
            run_on(
                "const h = styles.reduce((acc, style) => { style.props.forEach((p) => { acc[p] = style; }); return acc; }, {});"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_object_assign_merge() {
        // Merges whole objects — `fromEntries` cannot express a merge.
        assert!(
            run_on("const m = styles.reduce((acc, s) => Object.assign(acc, s.propTypes), {});")
                .is_empty()
        );
    }

    #[test]
    fn allows_concise_spread_of_foreign_object() {
        // Spreads a non-accumulator object — a merge, not an entry write.
        assert!(
            run_on("const m = styles.reduce((acc, s) => ({ ...acc, ...s.propTypes }), {});")
                .is_empty()
        );
    }

    #[test]
    fn allows_conditional_key_write() {
        // Branching body is not the canonical single-entry shape.
        assert!(
            run_on(
                "const o = xs.reduce((acc, x) => { if (x.ok) { acc[x.k] = x.v; } return acc; }, {});"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_multiple_key_writes() {
        // More than one assignment — not a single entry per element.
        assert!(
            run_on(
                "const o = xs.reduce((acc, x) => { acc[x.k] = x.v; acc.count = 1; return acc; }, {});"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_destructured_accumulator() {
        // A destructured accumulator is not a plain entry accumulator.
        assert!(
            run_on("const o = xs.reduce(({ ...acc }, x) => ({ ...acc, [x.k]: x.v }), {});")
                .is_empty()
        );
    }

    #[test]
    fn allows_reduce_with_non_object_init() {
        assert!(run_on("const sum = nums.reduce((acc, n) => acc + n, 0);").is_empty());
    }

    #[test]
    fn allows_object_from_entries() {
        assert!(
            run_on("const obj = Object.fromEntries(pairs.map(([k, v]) => [k, v]));").is_empty()
        );
    }

    #[test]
    fn allows_reduce_with_array_init() {
        assert!(run_on("const arr = items.reduce((acc, x) => [...acc, x], []);").is_empty());
    }
}
