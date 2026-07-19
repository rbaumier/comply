//! no-instanceof-builtins OXC backend — flag `x instanceof Array` and other builtins.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, TSType, TSTypeName};
use std::sync::Arc;

pub struct Check;

/// Built-in constructors the rule keeps flagging.
///
/// Each listed builtin has a cross-realm-safe alternative the remediation
/// can point at (e.g. `Array.isArray(x)` for `Array`), so flagging
/// `instanceof` is actionable.
///
/// `Error` and its subclasses (EvalError, RangeError, …) are *not*
/// listed here. In server-side single-realm Node/Bun apps with no
/// `vm.runInContext`, no `Worker` boundaries and no iframes, the
/// cross-realm concern that motivates avoiding `x instanceof Error`
/// does not apply — and `instanceof Error` is the canonical, well-typed
/// way to narrow an `unknown` thrown value. Forcing every boundary
/// mapper to rewrite the same pattern through a custom helper produces
/// a flood of false positives.
///
/// `Promise` is also *not* listed: there is no `Promise.isPromise()`
/// built-in, duck-typing `.then` accepts any thenable (semantically
/// different), and converting to `async` is not always possible for
/// mixed sync/async APIs. With no cross-realm-safe alternative, the
/// warning would be unactionable.
const BUILTINS: &[&str] = &[
    "Array",
    "ArrayBuffer",
    "RegExp",
    "Map",
    "Set",
    "WeakMap",
    "WeakSet",
];

/// Returns `true` when `ty` is a type-predicate (`val is X`) whose asserted type
/// is a bare reference to `builtin`. Type arguments are ignored, so
/// `val is WeakMap<K, V>` matches `builtin == "WeakMap"`.
fn predicate_asserts_builtin(ty: &TSType, builtin: &str) -> bool {
    let TSType::TSTypePredicate(pred) = ty else { return false };
    let Some(asserted) = pred.type_annotation.as_ref() else { return false };
    let TSType::TSTypeReference(type_ref) = &asserted.type_annotation else { return false };
    let TSTypeName::IdentifierReference(id) = &type_ref.type_name else { return false };
    id.name.as_str() == builtin
}

/// Returns `true` when the `instanceof builtin` check at `node_id` is the body of
/// the nearest enclosing function whose declared return type is a type-predicate
/// asserting the same `builtin` (`(val): val is RegExp => val instanceof RegExp`).
/// Such a function is the canonical type guard for that builtin: its signature
/// already commits to exactly this check, so the `instanceof` is the intended
/// implementation, not a cross-realm footgun. The walk stops at the first function
/// boundary, so an outer guard never exempts an inner check and a predicate naming
/// a different builtin never exempts a mismatched `instanceof`.
fn in_matching_type_predicate_guard(
    node_id: oxc_semantic::NodeId,
    builtin: &str,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    for kind in semantic.nodes().ancestor_kinds(node_id) {
        let return_type = match kind {
            AstKind::ArrowFunctionExpression(arrow) => arrow.return_type.as_ref(),
            AstKind::Function(func) => func.return_type.as_ref(),
            _ => continue,
        };
        return return_type
            .is_some_and(|ann| predicate_asserts_builtin(&ann.type_annotation, builtin));
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::BinaryExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["instanceof"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::BinaryExpression(bin) = node.kind() else { return };
        if bin.operator != oxc_ast::ast::BinaryOperator::Instanceof {
            return;
        }

        let Expression::Identifier(id) = &bin.right else { return };
        let name = id.name.as_str();
        if !BUILTINS.contains(&name) {
            return;
        }

        // The body of a TS type-predicate guard (`(val): val is Map => val
        // instanceof Map`) is the canonical, declared implementation of that
        // check — flagging it is noise, not an actionable cross-realm warning.
        if in_matching_type_predicate_guard(node.id(), name, semantic) {
            return;
        }

        let suggestion = if name == "Array" {
            "Use `Array.isArray(x)` instead.".to_string()
        } else {
            format!("Avoid `instanceof {name}` — it fails across realms.")
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, bin.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: suggestion,
            severity: Severity::Error,
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
    fn flags_instanceof_array() {
        let src = "const r = x instanceof Array;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_instanceof_map() {
        let src = "const r = x instanceof Map;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_instanceof_error() {
        // Regression for rbaumier/comply#28 — `instanceof Error` is the
        // canonical narrowing for `unknown` thrown values in single-realm
        // Node/Bun. No realistic TS-wide alternative exists.
        let src = r#"
            function fromCaught(value: unknown): Error | null {
                return value instanceof Error ? value : null;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_instanceof_error_subclasses() {
        for cls in ["TypeError", "RangeError", "SyntaxError"] {
            let src = format!("const r = x instanceof {cls};");
            assert!(run(&src).is_empty(), "{cls} should be allowed");
        }
    }

    #[test]
    fn ignores_instanceof_promise() {
        // Regression for rbaumier/comply#1672 — no `Promise.isPromise()`
        // built-in exists, so the warning would be unactionable.
        let src = r#"
            function unwrap(result: unknown) {
                if (result instanceof Promise) {
                    return result;
                }
                return result;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_array_after_promise_removed() {
        // Negative-space guard for #1672: removing `Promise` must not stop
        // `Array` (which has `Array.isArray`) from firing.
        let src = "const r = x instanceof Array;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_instanceof_in_type_predicate_guard_issue_6699() {
        // Regression for rbaumier/comply#6699 (unjs/unenv reimplementing
        // `node:util/types`): the body of a `val is X` type-predicate function
        // is the canonical implementation of that guard, not a footgun.
        let src = "const isRegExp = (val): val is RegExp => val instanceof RegExp;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_instanceof_map_in_type_predicate_guard() {
        let src =
            "const isMap = (val): val is Map<unknown, unknown> => val instanceof Map;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_instanceof_weakmap_predicate_with_type_args() {
        // `val is WeakMap<K, V>` matches `instanceof WeakMap` on the bare name.
        let src = "const isWeakMap = <K extends WeakKey, V>(val: unknown): val is WeakMap<K, V> => val instanceof WeakMap;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_instanceof_in_type_predicate_function_declaration() {
        let src = r#"
            function isSet(val: unknown): val is Set<unknown> {
                return val instanceof Set;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_bare_instanceof_map_outside_guard() {
        // Negative space for #6699: a bare check with no type-predicate return
        // type must still fire.
        let src = "const r = x instanceof Map;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_when_predicate_names_different_builtin() {
        // Negative space for #6699: the predicate asserts `Set` but the body
        // checks `Map` — not the same builtin, so the guard must not exempt it.
        let src = "const f = (val): val is Set<unknown> => val instanceof Map;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_instanceof_in_nested_function_inside_matching_guard() {
        // Negative space for #6699: the walk stops at the FIRST enclosing
        // function. The inner arrow has no type-predicate return type, so its
        // `instanceof Map` must fire even though the OUTER guard asserts the
        // same builtin (`val is Map`).
        let src = r#"
            const outer = (val): val is Map<unknown, unknown> => {
                const inner = () => val instanceof Map;
                return inner();
            };
        "#;
        assert_eq!(run(src).len(), 1);
    }
}
