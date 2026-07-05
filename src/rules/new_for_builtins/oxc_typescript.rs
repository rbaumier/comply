//! new-for-builtins OXC backend — enforce `new` for builtins, disallow for Symbol/BigInt.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression};
use oxc_span::GetSpan;
use std::sync::Arc;

/// Builtins that MUST be called with `new`.
const ENFORCE_NEW: &[&str] = &[
    "Object",
    "Array",
    "ArrayBuffer",
    "DataView",
    "Date",
    "Error",
    "Function",
    "Map",
    "WeakMap",
    "Set",
    "WeakSet",
    "Promise",
    "RegExp",
    "SharedArrayBuffer",
    "Proxy",
    "WeakRef",
    "FinalizationRegistry",
];

/// Builtins that MUST NOT be called with `new`.
const DISALLOW_NEW: &[&str] = &["Symbol", "BigInt"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression, AstType::NewExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            // `Map()` without `new` — should be `new Map()`.
            AstKind::CallExpression(call) => {
                let Expression::Identifier(ident) = &call.callee else {
                    return;
                };
                let name = ident.name.as_str();
                if !ENFORCE_NEW.contains(&name) {
                    return;
                }
                if is_name_locally_bound(semantic, ident) {
                    return;
                }
                // `x === Object(x)` / `x !== Object(x)` uses `Object(...)` as the
                // spec is-object coercion operator, not as a constructor.
                if name == "Object" && is_object_identity_idiom(call, node, semantic, ctx.source) {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!("Use `new {name}()` instead of `{name}()`."),
                    severity: Severity::Error,
                    span: None,
                });
            }
            // `new Symbol()` — should be `Symbol()`.
            AstKind::NewExpression(new_expr) => {
                let Expression::Identifier(ident) = &new_expr.callee else {
                    return;
                };
                let name = ident.name.as_str();
                if !DISALLOW_NEW.contains(&name) {
                    return;
                }
                if is_name_locally_bound(semantic, ident) {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Use `{name}()` instead of `new {name}()`. `{name}` is not a constructor."
                    ),
                    severity: Severity::Error,
                    span: None,
                });
            }
            _ => {}
        }
    }
}

/// Check whether the identifier has a local binding (parameter, variable, or import).
fn is_name_locally_bound(
    semantic: &oxc_semantic::Semantic,
    ident: &oxc_ast::ast::IdentifierReference,
) -> bool {
    let scoping = semantic.scoping();
    let name = ident.name.as_str();
    // Check if any symbol with this name exists in any scope.
    for sym_id in scoping.symbol_ids() {
        if scoping.symbol_name(sym_id) == name {
            return true;
        }
    }
    false
}

/// True when this `Object(arg)` call is one operand of a `===`/`!==` whose other
/// operand is the same expression as `arg` (`x === Object(x)` / `x !== Object(x)`,
/// either direction). That is the spec-based "is `x` an object?" identity test —
/// `Object(v)` returns objects unchanged and wraps primitives — so the call is a
/// coercion, not a constructor call, and `new Object()` would be nonsensical.
/// Only a single, non-spread argument qualifies. Operand identity is compared by
/// source-text slice, which covers identifiers and member expressions.
fn is_object_identity_idiom(
    call: &oxc_ast::ast::CallExpression,
    node: &oxc_semantic::AstNode<'_>,
    semantic: &oxc_semantic::Semantic,
    source: &str,
) -> bool {
    if call.arguments.len() != 1 {
        return false;
    }
    let Some(arg) = call.arguments.first().and_then(|a| a.as_expression()) else {
        return false;
    };
    let AstKind::BinaryExpression(bin) = semantic.nodes().parent_node(node.id()).kind() else {
        return false;
    };
    if !matches!(
        bin.operator,
        BinaryOperator::StrictEquality | BinaryOperator::StrictInequality
    ) {
        return false;
    }
    // The call is one operand; compare the argument with the other operand.
    let other = if bin.left.span() == call.span { &bin.right } else { &bin.left };
    let arg_span = arg.span();
    let other_span = other.span();
    source
        .get(arg_span.start as usize..arg_span.end as usize)
        .zip(source.get(other_span.start as usize..other_span.end as usize))
        .is_some_and(|(a, b)| a == b)
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
    fn allows_not_equal_object_identity_idiom() {
        // #7271: `e !== Object(e)` is the is-object coercion test, not construction.
        assert!(run_on("if (e !== Object(e)) { e = { target: null }; }").is_empty());
    }

    #[test]
    fn allows_equal_object_identity_idiom_other_direction() {
        // #7271: `Object(model) === model` — same idiom, call on the left.
        assert!(run_on("if (Object(model) === model) { doThing(); }").is_empty());
    }

    #[test]
    fn allows_object_identity_idiom_member_operand() {
        assert!(run_on("if (foo.bar === Object(foo.bar)) {}").is_empty());
    }

    #[test]
    fn flags_empty_object_construction() {
        let d = run_on("const o = Object();");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "new-for-builtins");
    }

    #[test]
    fn flags_object_equality_with_differing_operands() {
        let d = run_on("if (x === Object(y)) {}");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_bare_object_call() {
        let d = run_on("const o = Object({ a: 1 });");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_other_builtins_unchanged() {
        assert_eq!(run_on("const a = Array();").len(), 1);
        assert_eq!(run_on("const e = Error('x');").len(), 1);
        assert_eq!(run_on("const m = Map();").len(), 1);
    }

    #[test]
    fn flags_array_identity_shape_still_construction() {
        // The guard is Object-specific: `Array(x) === x` is still construction.
        let d = run_on("if (Array(x) === x) {}");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn new_object_not_flagged() {
        assert!(run_on("const o = new Object();").is_empty());
    }
}
