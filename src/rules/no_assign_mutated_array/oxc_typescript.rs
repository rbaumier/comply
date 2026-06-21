//! no-assign-mutated-array OxcCheck backend — flag assignments whose RHS
//! is a mutating array method call (sort, reverse, fill).
//!
//! Only fires when the receiver is demonstrably an array (an array literal, a
//! binding typed `T[]`/`Array<T>`, or an array-producing expression): a `.fill()`
//! / `.sort()` / `.reverse()` whose receiver cannot be proven an array is a
//! method-name collision on a non-array object (e.g. a canvas `shape.fill(color)`
//! color-setter), not `Array.prototype`.
//!
//! Within genuine arrays, receivers known to be a fresh array (spread copy,
//! `new Array`, `Array.from`/`of`, `Object.keys`/`values`/`entries`/
//! `getOwnPropertyNames`, and fresh-returning methods like `slice`/`filter`/
//! `map`) are exempt: mutating them in place is not observable through any other
//! reference.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, expression_is_array};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const MUTATING_METHODS: &[&str] = &["sort", "reverse", "fill"];

/// Check if a call is a mutating array method and return the method name.
fn mutating_method_name<'a>(
    expr: &'a Expression<'a>,
    semantic: &oxc_semantic::Semantic,
    source: &str,
) -> Option<&'a str> {
    let call = unwrap_expr(expr);
    let Expression::CallExpression(call) = call else { return None };
    let Expression::StaticMemberExpression(member) = &call.callee else { return None };
    let name = member.property.name.as_str();
    if !MUTATING_METHODS.contains(&name) {
        return None;
    }

    // Only a genuine array receiver mutates in place; a method-name collision on
    // a non-array object (`shape.fill(color)`) is not `Array.prototype`.
    if !expression_is_array(&member.object, semantic) {
        return None;
    }

    // Allow when the receiver is a freshly-created array — mutating it in place
    // is unobservable through any other reference.
    if is_fresh_array(&member.object, source) {
        return None;
    }

    Some(name)
}

/// Walk through parenthesized / type assertion wrappers.
fn unwrap_expr<'a, 'b>(expr: &'b Expression<'a>) -> &'b Expression<'a> {
    match expr {
        Expression::ParenthesizedExpression(p) => unwrap_expr(&p.expression),
        Expression::TSAsExpression(t) => unwrap_expr(&t.expression),
        Expression::TSSatisfiesExpression(t) => unwrap_expr(&t.expression),
        Expression::TSNonNullExpression(t) => unwrap_expr(&t.expression),
        Expression::TSTypeAssertion(t) => unwrap_expr(&t.expression),
        _ => expr,
    }
}

fn is_fresh_array(expr: &Expression, source: &str) -> bool {
    match expr {
        Expression::ArrayExpression(_) => {
            // Spread copy: `[...arr]`
            let text = &source[expr.span().start as usize..expr.span().end as usize];
            text.contains("...")
        }
        // `new Array(n)` constructs a brand-new array with no prior alias.
        Expression::NewExpression(new_expr) => {
            matches!(&new_expr.callee, Expression::Identifier(id) if id.name == "Array")
        }
        Expression::CallExpression(call) => {
            let Expression::StaticMemberExpression(member) = &call.callee else {
                return false;
            };
            let method = member.property.name.as_str();
            // `Array.from(...)` / `Array.of(...)` also return a brand-new array.
            if matches!(method, "from" | "of")
                && matches!(&member.object, Expression::Identifier(id) if id.name == "Array")
            {
                return true;
            }
            // `Object.keys/values/entries/getOwnPropertyNames(obj)` return a NEW
            // array, not a reference to `obj` — sorting/reversing them in place is
            // safe. The `Object` receiver is load-bearing: a `.keys()` on any other
            // receiver (e.g. `myMap.keys()`) is not a fresh-array producer.
            if matches!(method, "keys" | "values" | "entries" | "getOwnPropertyNames")
                && matches!(&member.object, Expression::Identifier(id) if id.name == "Object")
            {
                return true;
            }
            matches!(
                method,
                "slice" | "filter" | "map" | "concat" | "flat" | "flatMap"
                    | "toSorted" | "toReversed" | "toSpliced" | "with"
            )
        }
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclaration, AstType::AssignmentExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".sort(", ".reverse(", ".fill("])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::VariableDeclaration(decl) => {
                for declarator in &decl.declarations {
                    let Some(init) = &declarator.init else { continue };
                    let Some(method) = mutating_method_name(init, semantic, ctx.source) else { continue };
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, init.span().start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "Assigning result of `.{method}()` — mutating method returns the same array. \
                             Use `toSorted()`, `toReversed()`, or spread before mutating: `[...arr].{method}(...)`."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            AstKind::AssignmentExpression(assign) => {
                let Some(method) = mutating_method_name(&assign.right, semantic, ctx.source) else { return };
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, assign.right.span().start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Assigning result of `.{method}()` — mutating method returns the same array. \
                         Use `toSorted()`, `toReversed()`, or spread before mutating: `[...arr].{method}(...)`."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            _ => {}
        }
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
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod oxc_tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_const_sort() {
        assert_eq!(run("function f(arr: number[]) { const x = arr.sort(); }").len(), 1);
    }

    #[test]
    fn allows_spread_then_sort() {
        assert!(run("const x = [...arr].sort();").is_empty());
    }

    // === issue #3305: mutating method on a freshly-constructed array ===

    #[test]
    fn allows_new_array_fill() {
        assert!(run("const chunks = new Array(n).fill('x');").is_empty());
    }

    #[test]
    fn allows_new_array_fill_repeat() {
        assert!(run("const chunks = new Array(sizeInMB).fill('x'.repeat(chunkSize));").is_empty());
    }

    #[test]
    fn allows_array_from_sort() {
        assert!(run("const x = Array.from(iter).sort();").is_empty());
    }

    #[test]
    fn flags_preexisting_array_fill() {
        // GUARD: a pre-existing typed array receiver is still mutated in place.
        assert_eq!(run("function f(arr: number[]) { const x = arr.fill(0); }").len(), 1);
    }

    // === issue #4883: `.fill()`/`.reverse()`/`.sort()` on a non-array object ===

    #[test]
    fn allows_fill_on_canvas_shape_param() {
        // `shape.fill(color)` is a Konva canvas color-getter/setter, not
        // `Array.prototype.fill` — the receiver is typed as a non-array class.
        assert!(
            run("function _fillColor(shape: Shape) { const fill = shape.fill(); }").is_empty()
        );
    }

    #[test]
    fn allows_fill_on_unresolved_receiver() {
        // An unprovable receiver (no type, no array initializer) is not flagged:
        // the method-name collision is too weak a signal on its own.
        assert!(run("const fill = shape.fill();").is_empty());
    }

    #[test]
    fn flags_array_literal_fill() {
        // GUARD: an array literal receiver is unambiguously an array.
        assert_eq!(run("const b = [1, 2, 3].fill(0);").len(), 1);
    }

    #[test]
    fn flags_typed_array_reverse() {
        assert_eq!(
            run("function f(arr: number[]) { const r = arr.reverse(); }").len(),
            1
        );
    }

    // === issue #4527: Object.keys/values/entries/getOwnPropertyNames return fresh arrays ===

    #[test]
    fn allows_object_keys_sort() {
        assert!(
            run("const sortedTokens = Object.keys(valueCounts).sort((a, b) => valueCounts[b] - valueCounts[a]);")
                .is_empty()
        );
    }

    #[test]
    fn allows_object_values_sort() {
        assert!(run("const v = Object.values(obj).sort();").is_empty());
    }

    #[test]
    fn allows_object_entries_reverse() {
        assert!(run("const e = Object.entries(obj).reverse();").is_empty());
    }

    #[test]
    fn allows_object_get_own_property_names_sort() {
        assert!(run("const n = Object.getOwnPropertyNames(obj).sort();").is_empty());
    }

    #[test]
    fn flags_non_object_keys_sort() {
        // GUARD: a non-`Object` receiver — `keys` is not a fresh-array method,
        // so freshness is unprovable and the mutation is still flagged.
        assert_eq!(run("const k = obj.keys().sort();").len(), 1);
    }
}
