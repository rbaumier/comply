use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, FunctionType, LogicalExpression, LogicalOperator};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            if let AstKind::LogicalExpression(expr) = node.kind() {
                check_logical(expr, semantic, ctx, &mut diagnostics);
            }
        }
        diagnostics
    }
}

/// True when `id` resolves to a non-optional formal parameter of a named
/// function declaration or a class method — a real input boundary.
///
/// Resolution is per-binding (via the reference's symbol declaration), so a
/// same-named parameter in another scope cannot poison the verdict. Two gates
/// must hold on the resolved binding:
///   1. The declaration is a non-optional `FormalParameter`. An optional
///      parameter (`param?: T`) admits `undefined` as valid input, so defaulting
///      it is idiomatic.
///   2. The enclosing function is a `FunctionDeclaration` or a class method —
///      not an inline arrow/function-expression callback (`map`/`watch`/event
///      handlers), whose parameters are supplied by the runtime, not the caller.
///
/// When the identifier does not resolve to a binding, returns `false`
/// (conservative; an exported arrow public API is likewise not flagged).
fn is_boundary_param(
    id: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let scoping = semantic.scoping();
    let Some(sym_id) = id
        .reference_id
        .get()
        .and_then(|ref_id| scoping.get_reference(ref_id).symbol_id())
    else {
        return false;
    };

    let nodes = semantic.nodes();
    let decl_id = scoping.symbol_declaration(sym_id);

    // Walk the declaration node and its ancestors once. The binding must be a
    // non-optional `FormalParameter`, and the first enclosing function must be a
    // real input boundary: a `FunctionDeclaration`, or a `Function` whose parent
    // is a `MethodDefinition` (a class method). An arrow or function-expression
    // callback (`map`/`watch`/event handlers) is excluded.
    let mut seen_param = false;
    for node in
        std::iter::once(nodes.get_node(decl_id)).chain(nodes.ancestors(decl_id))
    {
        match node.kind() {
            AstKind::FormalParameter(param) => {
                if param.optional {
                    return false;
                }
                seen_param = true;
            }
            AstKind::Function(func) => {
                return seen_param
                    && (func.r#type == FunctionType::FunctionDeclaration
                        || matches!(
                            nodes.parent_kind(node.id()),
                            AstKind::MethodDefinition(_)
                        ));
            }
            AstKind::ArrowFunctionExpression(_) => return false,
            _ => {}
        }
    }
    false
}

fn check_logical(
    expr: &LogicalExpression,
    semantic: &oxc_semantic::Semantic<'_>,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let op = expr.operator;
    if !matches!(op, LogicalOperator::Coalesce | LogicalOperator::Or) {
        return;
    }
    let Expression::Identifier(id) = &expr.left else {
        return;
    };
    let name = id.name.as_str();
    if !is_boundary_param(id, semantic) {
        return;
    }
    // A typed identifier fallback (e.g. `param ?? otherParam`) is intentional
    // domain logic — skip.
    if let Expression::Identifier(right_id) = &expr.right {
        if right_id.name.as_str() != "undefined" {
            return;
        }
    }
    // `param || (a && b)` is a boolean OR combining two conditions, not a
    // fallback default for `param`. When the right side of `||` is itself a
    // boolean computation, no replacement value is being assigned — skip.
    // `??` is always nullish defaulting regardless of the right side's shape.
    if op == LogicalOperator::Or && is_boolean_computation(crate::oxc_helpers::peel_parens(&expr.right)) {
        return;
    }
    let op_text = op.as_str();
    let (line, column) = byte_offset_to_line_col(ctx.source, expr.span.start as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: format!(
            "Using '{op_text}' to default a function parameter '{name}' \
             silently paves over invalid input. Validate at the \
             boundary and return a Result error instead."
        ),
        severity: super::META.severity,
        span: None,
    });
}

/// True when `expr` produces a boolean by computation rather than being a
/// fallback value. Used to distinguish `param || (a && b)` (boolean OR) from
/// `param || defaultValue` (silent default).
fn is_boolean_computation(expr: &Expression) -> bool {
    use oxc_ast::ast::{BinaryOperator, UnaryOperator};
    match expr {
        Expression::LogicalExpression(_) | Expression::BooleanLiteral(_) => true,
        Expression::UnaryExpression(unary) => unary.operator == UnaryOperator::LogicalNot,
        Expression::BinaryExpression(bin) => matches!(
            bin.operator,
            BinaryOperator::Equality
                | BinaryOperator::StrictEquality
                | BinaryOperator::Inequality
                | BinaryOperator::StrictInequality
                | BinaryOperator::LessThan
                | BinaryOperator::LessEqualThan
                | BinaryOperator::GreaterThan
                | BinaryOperator::GreaterEqualThan
                | BinaryOperator::In
                | BinaryOperator::Instanceof
        ),
        _ => false,
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

    #[test]
    fn flags_nullish_coalesce_on_param() {
        assert_eq!(
            run_on("function f(x: number) { const v = x ?? 0; return v; }").len(),
            1
        );
    }

    #[test]
    fn flags_logical_or_on_param() {
        assert_eq!(
            run_on("function f(items: number[]) { const v = items || []; return v; }").len(),
            1
        );
    }

    #[test]
    fn allows_default_on_local_variable() {
        assert!(run_on("function f() { const local: number | null = null; const v = local ?? 0; return v; }").is_empty());
    }

    #[test]
    fn allows_nullish_on_property_access() {
        assert!(run_on("function f(opts: { x?: number }) { return opts.x ?? 0; }").is_empty());
    }

    #[test]
    fn allows_typed_identifier_fallback() {
        // `dateEntree ?? createdAt`: both are typed parameters; this is intentional domain logic.
        assert!(run_on(
            "function deriveEntryYear(dateEntree: Date | null, createdAt: Date): number { return (dateEntree ?? createdAt).getUTCFullYear(); }"
        ).is_empty());
    }

    #[test]
    fn allows_nullish_default_on_optional_object_param() {
        // `options?: {...}` explicitly admits `undefined`; defaulting it is idiomatic.
        assert!(run_on(
            "function safeClean(worker: F, options?: { actionIfDirty: \"warn\" | \"throw\" }) { const { actionIfDirty } = options ?? {}; return actionIfDirty; }"
        ).is_empty());
    }

    #[test]
    fn allows_nullish_default_on_optional_string_param() {
        // `sourcePath?: string` is optional; `?? path.join(...)` is correct.
        assert!(run_on(
            "function generateSamples(sourcePath?: string) { const finalSourcePath = sourcePath ?? path.join(base, DEV_SAMPLES_BASE); return finalSourcePath; }"
        ).is_empty());
    }

    #[test]
    fn allows_logical_or_default_on_optional_param() {
        assert!(run_on(
            "function f(count?: number) { const v = count || 0; return v; }"
        ).is_empty());
    }

    #[test]
    fn flags_nullish_default_on_non_optional_nullable_param() {
        // `value: number | undefined` (no `?`): nullability is domain-driven, still a smell.
        assert_eq!(
            run_on("function f(value: number | undefined) { const v = value ?? 0; return v; }").len(),
            1
        );
    }

    #[test]
    fn flags_nullish_default_on_non_optional_null_param() {
        // `value: number | null` (no `?`): still flagged.
        assert_eq!(
            run_on("function f(value: number | null) { const v = value ?? 0; return v; }").len(),
            1
        );
    }

    #[test]
    fn allows_boolean_or_with_logical_rhs() {
        // Regression #1762: `activeBar || (activeLegend && activeLegend !== name)`
        // is a boolean OR combining two conditions, not a fallback default.
        assert!(run_on(
            "const renderShape = (props: any, activeBar: any, activeLegend: string) => { const o = activeBar || (activeLegend && activeLegend !== name) ? 0.3 : 1; return o; };"
        ).is_empty());
    }

    #[test]
    fn allows_boolean_or_with_comparison_rhs() {
        // `param || (a === b)`: comparison produces a boolean, not a default value.
        assert!(run_on(
            "function f(active: boolean, x: number, y: number) { const v = active || (x === y); return v; }"
        ).is_empty());
    }

    #[test]
    fn allows_boolean_or_with_negation_rhs() {
        // `param || !other`: logical-not produces a boolean, not a default value.
        assert!(run_on(
            "function f(active: boolean, hidden: boolean) { const v = active || !hidden; return v; }"
        ).is_empty());
    }

    #[test]
    fn still_flags_logical_or_with_value_rhs() {
        // `param || []`: the right side is a fallback value, still flagged.
        assert_eq!(
            run_on("function f(items: number[]) { const v = items || []; return v; }").len(),
            1
        );
    }

    #[test]
    fn flags_nullish_coalesce_on_method_param() {
        // A class method is a real input boundary: a non-optional parameter
        // defaulted with `??` is still flagged.
        assert_eq!(
            run_on("class C { m(x: number) { return x ?? 0; } }").len(),
            1
        );
    }

    #[test]
    fn allows_cross_scope_optional_param() {
        // Issue #3712 case 1: two non-optional `value` params in sibling functions
        // must not poison an optional `value?` in a third. Per-binding resolution
        // means the optional binding is exempt; `a`/`b` don't default anything.
        assert!(run_on(
            "function a(value: string) { return value; }\nfunction b(value: number) { return value; }\nasync function copy(value?: string) { const r = value ?? \"x\"; return r; }"
        ).is_empty());
    }

    #[test]
    fn allows_flatmap_callback_param() {
        // Issue #3712 case 2: `i` is a `flatMap` callback parameter supplied by the
        // runtime, not a function-declaration/method input boundary.
        assert!(run_on(
            "function useX() { return modes.flatMap(i => (i || \"\").split(\"/\")); }"
        ).is_empty());
    }

    #[test]
    fn allows_event_handler_arrow_param() {
        // Issue #3712 case 2: `event` is an arrow parameter (const-assigned event
        // handler), not a boundary function — not flagged.
        assert!(run_on(
            "const handler = (event) => { event = event || globalThis.event; return event; };"
        ).is_empty());
    }

    #[test]
    fn allows_watch_callback_param() {
        // Issue #3712 case 2: `newValue` is a `watch` callback parameter supplied
        // by the reactivity runtime, not a boundary function input.
        assert!(run_on(
            "function useTitle() { watch(title, (newValue) => { document.title = newValue ?? \"\"; }); }"
        ).is_empty());
    }
}
