//! no-misleading-array-reverse OXC backend.
//!
//! Only fires when the receiver is demonstrably an array (an array literal, a
//! binding typed `T[]`/`Array<T>`, or an array-producing expression): a
//! `.reverse()` / `.sort()` / `.fill()` whose receiver cannot be proven an array
//! is a method-name collision on a non-array object (e.g. a canvas
//! `shape.fill(color)` color-setter), not `Array.prototype`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, expression_is_array};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, IdentifierReference, VariableDeclarationKind};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// In-place array mutators that return `this` (the same reference). Assigning or
/// returning their result is misleading — it looks like a copy but aliases the
/// original. `splice` is deliberately excluded: it returns a brand-new array of
/// the removed elements, never `this`, so its result is never a hidden alias.
const MUTATING_METHODS: &[&str] = &["reverse", "sort", "fill"];

/// Non-mutating array methods that always return a fresh array. Chaining a
/// mutating method onto one of these is safe — the caller holds the only
/// reference to the new array, so nothing shared is silently mutated.
const FRESH_ARRAY_METHODS: &[&str] =
    &["filter", "map", "slice", "concat", "flat", "flatMap", "split"];

/// Whether the receiver is a freshly-constructed array with no prior alias, so
/// mutating it in place is not observable through any other reference.
fn is_fresh_array(expr: &Expression) -> bool {
    match expr {
        // An array literal `[a, b]` is constructed fresh at this expression and
        // has no other reference, so mutating it in place is unobservable.
        Expression::ArrayExpression(_) => true,
        // `new Array(n)` / `new Uint8Array(n)` (or any TypedArray ctor) build a
        // brand-new array-like value with no prior alias.
        Expression::NewExpression(new_expr) => {
            matches!(&new_expr.callee, Expression::Identifier(id)
                if crate::oxc_helpers::is_fresh_array_ctor_name(&id.name))
        }
        Expression::CallExpression(inner) => {
            let Expression::StaticMemberExpression(member) = &inner.callee else {
                return false;
            };
            // `Array.from(...)` / `Array.of(...)` return a brand-new array.
            if matches!(member.property.name.as_str(), "from" | "of")
                && matches!(&member.object, Expression::Identifier(id) if id.name == "Array")
            {
                return true;
            }
            // `Object.keys/values/entries/getOwnPropertyNames(...)` each return a
            // brand-new array per spec, so the caller holds the only reference.
            if matches!(
                member.property.name.as_str(),
                "keys" | "values" | "entries" | "getOwnPropertyNames"
            ) && matches!(&member.object, Expression::Identifier(id) if id.name == "Object")
            {
                return true;
            }
            // An in-place mutator (`sort`/`reverse`/`fill`) returns its receiver
            // by identity, so freshness propagates through the chain:
            // `[...arr].sort()` is as fresh as `[...arr]`. Recurse to keep the
            // chain rooted at a genuine fresh producer.
            if MUTATING_METHODS.contains(&member.property.name.as_str()) {
                return is_fresh_array(&member.object);
            }
            // Chaining onto a fresh array, e.g. `arr.filter(p).sort(cmp)`.
            FRESH_ARRAY_METHODS.contains(&member.property.name.as_str())
        }
        _ => false,
    }
}

/// Whether `id` resolves to a `const` binding whose initializer is itself a
/// fresh array (`[...arr]`, `value.slice()`, `Array.from(...)`, ...). Such a
/// receiver is the sole reference to a brand-new array, so mutating it in place
/// is unobservable — the same reasoning as a literal fresh-array receiver.
fn receiver_is_fresh_const(
    id: &IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Some(ref_id) = id.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    for kind in
        std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        match kind {
            // `const` is the only kind that is provably never reassigned; a `let`
            // binding could be pointed at a shared array later, so stay conservative.
            AstKind::VariableDeclaration(decl) => {
                return decl.kind == VariableDeclarationKind::Const;
            }
            AstKind::VariableDeclarator(decl) => {
                let Some(init) = &decl.init else {
                    return false;
                };
                if !is_fresh_array(init) {
                    return false;
                }
            }
            _ => {}
        }
    }
    false
}

/// Check if a call expression is a mutating array method call (not on a spread
/// copy nor a fresh array returned by a non-mutating method).
fn is_mutating_call(expr: &Expression, semantic: &oxc_semantic::Semantic) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    if !MUTATING_METHODS.contains(&member.property.name.as_str()) {
        return false;
    }
    // Only a genuine array receiver returns `this` from these mutators; a
    // method-name collision on a non-array object (`shape.fill(color)`,
    // `Immutable.sort(x)`) is not `Array.prototype` and is never misleading.
    if !expression_is_array(&member.object, semantic) {
        return false;
    }
    // Not misleading when the receiver is a fresh array — either literally
    // (`[...arr].sort()`) or an identifier resolving to a fresh-array `const`
    // (`const a = [...arr]; a.sort()`).
    if is_fresh_array(&member.object) {
        return false;
    }
    if let Expression::Identifier(obj) = &member.object
        && receiver_is_fresh_const(obj, semantic)
    {
        return false;
    }
    true
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclaration, AstType::ReturnStatement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".reverse(", ".sort(", ".fill("])
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
                    let Some(init) = &declarator.init else {
                        continue;
                    };
                    if is_mutating_call(init, semantic) {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, init.span().start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "Assigning the result of a mutating array method is misleading — it returns the same reference, not a copy.".into(),
                            severity: Severity::Error,
                            span: None,
                        });
                    }
                }
            }
            AstKind::ReturnStatement(ret) => {
                if let Some(arg) = &ret.argument
                    && is_mutating_call(arg, semantic) {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, arg.span().start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "Returning the result of a mutating array method is misleading — it returns the same reference, not a copy.".into(),
                            severity: Severity::Error,
                            span: None,
                        });
                    }
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
    fn flags_const_reverse() {
        assert_eq!(
            run("function f(arr: number[]) { const reversed = arr.reverse(); }").len(),
            1
        );
    }

    #[test]
    fn flags_const_sort() {
        // `arr.sort(cmp)` mutates the shared `arr` directly — still misleading.
        assert_eq!(
            run("function f(arr: number[]) { const x = arr.sort((a, b) => a - b); }").len(),
            1
        );
    }

    #[test]
    fn flags_return_sort() {
        assert_eq!(
            run("function f(arr: number[]) { return arr.sort(); }").len(),
            1
        );
    }

    #[test]
    fn allows_spread_copy() {
        assert!(run("const reversed = [...arr].reverse();").is_empty());
    }

    // === issue #2382: mutating method chained on a fresh-array-returning call ===

    #[test]
    fn allows_filter_then_sort() {
        assert!(run("const x = arr.filter((p) => p.active).sort((a, b) => a.n - b.n);").is_empty());
    }

    #[test]
    fn allows_map_then_reverse() {
        assert!(run("const r = arr.map((f) => f.value).reverse();").is_empty());
    }

    #[test]
    fn allows_slice_then_sort() {
        assert!(run("const s = arr.slice(1).sort((a, b) => a - b);").is_empty());
    }

    #[test]
    fn allows_filter_then_sort_in_return() {
        assert!(run("function f() { return arr.filter((p) => p.active).sort(cmp); }").is_empty());
    }

    // === issue #3305: mutating method on a freshly-constructed array ===

    #[test]
    fn allows_new_array_reverse() {
        assert!(run("const chunks = new Array(n).reverse();").is_empty());
    }

    #[test]
    fn allows_new_array_fill() {
        assert!(run("const chunks = new Array(sizeInMB).fill('x'.repeat(chunkSize));").is_empty());
    }

    #[test]
    fn allows_array_from_sort() {
        assert!(run("const x = Array.from(iter).sort((a, b) => a - b);").is_empty());
    }

    #[test]
    fn flags_preexisting_array_sort() {
        // GUARD: a pre-existing typed array receiver is still mutated in place.
        assert_eq!(
            run("function f(arr: number[]) { const sorted = arr.sort(); }").len(),
            1
        );
    }

    // === issue #5320: TypedArray constructors are fresh, like `new Array` ===

    #[test]
    fn allows_new_uint8array_fill() {
        assert!(run("const buffer = new Uint8Array(9).fill(toCharCode(' '));").is_empty());
    }

    #[test]
    fn allows_new_float32array_fill() {
        assert!(run("const buffer = new Float32Array(n).fill(1);").is_empty());
    }

    #[test]
    fn flags_aliased_array_reverse() {
        // GUARD: an aliased (non-fresh) receiver is still misleading.
        assert_eq!(run("function f(arr: number[]) { const r = arr.reverse(); }").len(), 1);
    }

    // === issue #3950: uppercase-first receiver is a namespace/class, not an array ===

    #[test]
    fn allows_pascalcase_class_reverse() {
        assert!(run("function c() { return Foo.reverse(x); }").is_empty());
    }

    #[test]
    fn allows_pascalcase_namespace_sort() {
        assert!(run("const x = Immutable.sort(x);").is_empty());
    }

    // === issue #4883: `.fill()`/`.reverse()`/`.sort()` on a non-array object ===

    #[test]
    fn allows_fill_on_canvas_shape_param() {
        // `shape.fill()` is a Konva canvas color-getter, not `Array.prototype`.
        assert!(
            run("function _fillColor(shape: Shape) { const fill = shape.fill(); }").is_empty()
        );
    }

    #[test]
    fn allows_fill_on_unresolved_receiver() {
        assert!(run("const fill = shape.fill();").is_empty());
    }

    // === issue #7287: any element-list array literal is a fresh receiver ===

    #[test]
    fn allows_array_literal_reverse() {
        // An inline element-list literal is a brand-new allocation with no other
        // alias, so reversing it in place is unobservable — not misleading.
        assert!(run("const r = [1, 2, 3].reverse();").is_empty());
    }

    #[test]
    fn allows_inline_array_literal_sort() {
        // `[this.foreignKey, this.otherKey].sort()` — the literal is fresh, no
        // prior binding aliases it, so the in-place sort is unobservable.
        assert!(
            run("class C { m() { const keys = [this.foreignKey, this.otherKey].sort(); } }")
                .is_empty()
        );
    }

    #[test]
    fn allows_single_element_literal_reverse() {
        assert!(run("const y = [x].reverse();").is_empty());
    }

    // === issue #3794: `splice` returns a new array of removed elements, never `this` ===

    #[test]
    fn allows_splice_on_shared_array() {
        // `arr.splice(...)` returns the removed elements, not `arr` — never the
        // "thought it was a copy but it's the same reference" bug, even on a
        // shared receiver.
        assert!(run("function f() { return arr.splice(0, 1); }").is_empty());
    }

    #[test]
    fn allows_destructured_splice_on_shared_array() {
        assert!(run("const [x] = data.splice(i, 1);").is_empty());
    }

    #[test]
    fn allows_uppercase_namespace_splice() {
        assert!(run("function a() { return MAP.splice(body); }").is_empty());
    }

    #[test]
    fn flags_typed_array_receiver_reverse() {
        // GUARD: a receiver typed as an array is an array instance — still misleading.
        assert_eq!(
            run("function f(items: string[]) { return items.reverse(); }").len(),
            1
        );
    }

    // === issue #3826: String#split() returns a freshly-allocated array ===

    #[test]
    fn allows_split_then_sort() {
        assert!(run("function f(text) { return text.split('\\n').sort(); }").is_empty());
    }

    // === issue #3746: Object.keys/values/entries/getOwnPropertyNames return fresh arrays ===

    #[test]
    fn allows_object_keys_sort() {
        assert!(run("const sortedKeys = Object.keys(oauthData).sort();").is_empty());
    }

    #[test]
    fn allows_object_values_sort() {
        assert!(run("const v = Object.values(o).sort();").is_empty());
    }

    #[test]
    fn allows_object_entries_sort() {
        assert!(run("const e = Object.entries(o).sort();").is_empty());
    }

    #[test]
    fn allows_object_get_own_property_names_reverse() {
        assert!(run("const n = Object.getOwnPropertyNames(o).reverse();").is_empty());
    }

    #[test]
    fn flags_non_object_keys_sort() {
        // GUARD: a non-`Object` receiver — `keys` is not a fresh-array method,
        // so freshness is unprovable and the mutation is still misleading.
        assert_eq!(run("const x = foo.keys().sort();").len(), 1);
    }

    // === issue #3794: receiver resolves to a local fresh-array `const` ===

    #[test]
    fn allows_spread_const_sort() {
        // The receiver is a fresh copy held in a `const`, so sorting it in place
        // is unobservable through the original `orgArray`.
        assert!(
            run("function f(orgArray) { const array = [...orgArray]; return array.sort((a, b) => 0); }")
                .is_empty()
        );
    }

    #[test]
    fn allows_slice_const_splice() {
        assert!(
            run("function f(value) { const result = value.slice(); const [item] = result.splice(0, 1); return item; }")
                .is_empty()
        );
    }

    #[test]
    fn allows_spread_const_reverse() {
        assert!(run("function f(a) { const b = [...a]; return b.reverse(); }").is_empty());
    }

    #[test]
    fn flags_let_spread_sort() {
        // GUARD: a `let` binding could be reassigned to a shared array later, so
        // freshness is not provable — stay conservative and flag.
        assert_eq!(
            run("function f(a) { let b = [...a]; return b.sort(); }").len(),
            1
        );
    }

    #[test]
    fn flags_const_non_fresh_init_sort() {
        // GUARD: a typed-array binding that is not a fresh copy (a shared array
        // passed in and rebound) is still mutated in place — still misleading.
        assert_eq!(
            run("function f(shared: number[]) { const b: number[] = shared; return b.sort(); }").len(),
            1
        );
    }

    // === issue #5211: empty array literal built via `push` is a fresh local array ===

    #[test]
    fn allows_empty_const_built_with_push_then_sort() {
        // The receiver is a `const` empty array literal populated in place — the
        // sole reference to a fresh allocation — so sorting it is unobservable.
        assert!(
            run("function f(items) { const matches: string[] = []; items.forEach((i) => matches.push(i)); return matches.sort((a, b) => b.length - a.length); }")
                .is_empty()
        );
    }

    #[test]
    fn allows_empty_array_literal_reverse() {
        // A direct empty-literal receiver is a fresh allocation.
        assert!(run("const r = [].reverse();").is_empty());
    }

    #[test]
    fn flags_parameter_array_reverse() {
        // GUARD: a function-parameter array is a shared reference owned by the
        // caller — reversing it in place is still misleading.
        assert_eq!(
            run("function f(xs: number[]) { const r = xs.reverse(); return r; }").len(),
            1
        );
    }

    #[test]
    fn flags_let_empty_array_sort() {
        // GUARD: an empty array literal bound to a `let` could be reassigned to a
        // shared array later, so ownership is not provable — stay conservative.
        assert_eq!(
            run("function f() { let b: number[] = []; return b.sort(); }").len(),
            1
        );
    }

    #[test]
    fn allows_hole_array_literal_reverse() {
        // Any array literal — even a sparse one with a hole (`[,]`) — is a fresh
        // allocation with no other alias, so reversing it in place is unobservable.
        assert!(run("const r = [,].reverse();").is_empty());
    }

    // === issue #7246: freshness propagates through chained mutating methods ===

    #[test]
    fn allows_spread_sort_then_reverse_chain() {
        // `[...arr].sort()` returns the fresh spread copy by identity, so the
        // chained `.reverse()` still aliases only that unshared array.
        assert!(run("const sortedPaths = [...UNSET_BATCH].sort().reverse();").is_empty());
    }

    #[test]
    fn allows_filter_sort_then_reverse_chain() {
        // Rooted at a fresh producer (`filter`) — freshness carries through the
        // chained mutators.
        assert!(run("const y = arr.filter((f) => f.active).sort().reverse();").is_empty());
    }

    #[test]
    fn flags_shared_sort_then_reverse_chain() {
        // GUARD: the chain root is a plain (shared) identifier, not a fresh
        // producer, so the aliasing mutation is still misleading.
        assert_eq!(run("const z = arr.sort().reverse();").len(), 1);
    }
}
