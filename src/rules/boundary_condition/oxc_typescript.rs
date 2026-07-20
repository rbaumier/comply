//! boundary-condition OXC backend.
//!
//! Flags `arr[0]` or `arr[arr.length - 1]` reads without a length guard
//! or nullish fallback. Optional-chained computed access (`arr?.[0]`) is
//! exempt: it is a deliberate optional read that short-circuits to
//! `undefined` when the base is nullish. The same intent is exempted when the
//! access result is immediately consumed by an optional member, computed, or
//! call access (`arr[0]?.prop`, `arr[0]?.[i]`, `arr[0]?.()`): the `?.`
//! acknowledges that `arr[0]` may be `undefined`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{
    binding_declared_ts_type, byte_offset_to_line_col, resolves_to_import_from, ts_type_member_type,
};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ComputedMemberExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ComputedMemberExpression(member) = node.kind() else {
            return;
        };
        // `arr?.[0]` is a deliberate optional access (short-circuits to `undefined`
        // when the base is nullish) — the same intent signal as `.at(0)` or a
        // `?? fallback`, so it is not an accidental unchecked read.
        if member.optional {
            return;
        }

        // `arr[0]?.prop` / `arr[0]?.[i]` / `arr[0]?.()` — the access result is
        // immediately guarded by optional chaining, so the developer has already
        // acknowledged that `arr[0]` may be `undefined`. Flagging the inner read
        // would be redundant.
        if result_consumed_by_optional_access(node, semantic) {
            return;
        }

        // `typeof arr[0]` — the access is the operand of a `typeof` operator. An
        // out-of-bounds index yields `undefined`, and `typeof undefined` is the
        // string `"undefined"` (the operator never throws), so an empty array
        // simply makes the surrounding type-guard comparison evaluate false. The
        // possibly-`undefined` result is harmless by construction — this is the
        // idiomatic `Array.isArray(x) && typeof x[0] === "number"` narrowing.
        if is_typeof_operand(node, semantic) {
            return;
        }

        // `arr[0] === 'h'` / `path[0] !== '/'` / `arguments[0] !== kConstruct` —
        // the access is the direct operand of an equality/inequality comparison
        // against a non-nullish value. An out-of-bounds index yields `undefined`,
        // and comparing `undefined` to a concrete value never throws and produces
        // the correct "no match" result (`undefined === 'h'` is `false`,
        // `undefined !== kConstruct` is `true`), so the comparison acts as an
        // implicit guard — the same rationale as the `typeof` exemption above. A
        // `=== undefined` / `=== null` comparison stays flagged: there the
        // emptiness IS the thing being checked, so it is a value read.
        if is_equality_comparison_operand(node, semantic) {
            return;
        }

        // `arr[0]!` / `arr[arr.length - 1]!` — the access is the operand of a
        // TypeScript non-null assertion. The `!` is the developer's explicit
        // statement that this element is present, dismissing exactly the boundary
        // condition this rule warns about, so the read is not an accidental
        // unchecked access. Teams that object to `!` are served by the dedicated
        // `ts-no-non-null-assertion` rule; flagging here would double-penalize.
        if is_non_null_asserted(node, semantic) {
            return;
        }

        // jest/vitest mock-introspection arrays — `<spy>.mock.calls[0]`,
        // `.mock.results[0]`, `.mock.instances[0]` (and a further index into a
        // call entry, `.mock.calls[0][1]`). These arrays are framework-managed
        // test structures; indexing them is the idiomatic way to read a recorded
        // call/result, not an unguarded out-of-bounds read on user data.
        if object_is_mock_introspection_array(&member.object) {
            return;
        }
        let source = ctx.source;

        // Only flag when object is a plain identifier or member expression chain
        let obj_text = expr_text(&member.object, source);
        match &member.object {
            Expression::Identifier(_) => {}
            Expression::StaticMemberExpression(_) | Expression::ComputedMemberExpression(_) => {}
            _ => return,
        }

        let is_first = is_zero_index(&member.expression, source);
        let is_last = !is_first && is_last_index(&member.expression, obj_text, source);
        if !is_first && !is_last {
            return;
        }

        // Skip assignment targets
        if is_assignment_target(node, semantic) {
            return;
        }

        // Skip if wrapped in `?? fallback` or `|| fallback`
        if has_nullish_or_logical_fallback(node, semantic) {
            return;
        }

        // Skip if dominated by an `if` / ternary / `&&` / enclosing `while`/`for`
        // whose condition guards this array — either a `.length` check or, for a
        // first-element read, a truthy `arr[0]` / `arr?.[0]` check on the same array.
        if has_length_guard_ancestor(node, semantic, obj_text, is_first, source) {
            return;
        }

        // Skip if inside a `switch (<obj_text>.length)` case that proves the array
        // is non-empty — a `case N:` with `N >= 1`, or a `default:` when the
        // switch lists `case 0:` (so length 0 is handled elsewhere).
        if has_switch_length_guard_ancestor(node, semantic, obj_text, source) {
            return;
        }

        // Skip when this access IS the discriminant of a `switch` — `switch (arr[0])`.
        // The discriminant is only read to be matched against each `case` label; an
        // out-of-bounds index yields `undefined`, which simply matches no `case` and
        // falls through (to `default:` if present, otherwise past the statement). The
        // dispatch cannot crash on `undefined`, so the boundary read in this position
        // is harmless. Distinct from the length-guard check above, which exempts an
        // `arr[0]` inside a `case` body of `switch (arr.length)`.
        if is_switch_discriminant(node, semantic) {
            return;
        }

        // Skip if a preceding sibling guards with early exit or expect().toHaveLength()
        if has_preceding_guard(node, semantic, obj_text, source) {
            return;
        }

        // Skip if a preceding unconditional `arr.push(...)` guarantees non-empty.
        // `push` always adds an element, so any subsequent `arr[0]` /
        // `arr[arr.length - 1]` read on the same binding is in-bounds. The push
        // may sit in an ancestor scope (e.g. module-level setup) that runs before
        // a nested callback's access.
        if has_preceding_push(node, semantic, obj_text, source) {
            return;
        }

        // Skip if a preceding "ensure non-empty" guard makes the array non-empty:
        // an `if` whose test detects the same base being nullish/empty
        // (`!arr`, `arr === null/undefined`, `arr.length === 0`, or a `||` of
        // these) and whose consequent assigns a non-empty array literal to that
        // same base — `if (!arr || arr.length === 0) { arr = [d]; }`.
        // After the block the base has at least one element in both branches
        // (guard false ⇒ already non-empty; guard true ⇒ assigned a non-empty
        // literal), so the subsequent first/last read is in-bounds.
        if has_preceding_ensure_nonempty_guard(node, obj_text, source, semantic) {
            return;
        }

        // `arr[0]` where `arr` is a same-scope `const` bound to a non-empty array
        // literal is provably in-bounds — the literal's element count is known.
        if is_first
            && let Expression::Identifier(obj_ident) = &member.object
            && resolves_to_nonempty_array_literal(node, obj_ident.name.as_str(), semantic)
        {
            return;
        }

        // `arr[0]` where `arr` is a same-scope `const` bound to a fixed-size array
        // construction — `new Uint32Array(N)` (any TypedArray) or `new Array(N)`
        // with a numeric-literal length `N >= 1`, or `new Uint32Array([...])` /
        // `new Array([...])` with a non-empty static element-list literal. The
        // constructed length is statically known to be at least one, so the
        // first-element read is in-bounds (e.g. the Web Crypto nonce idiom
        // `const a = new Uint32Array(1); a[0]`).
        if is_first
            && let Expression::Identifier(obj_ident) = &member.object
            && resolves_to_nonempty_fixed_array_construction(
                node,
                obj_ident.name.as_str(),
                semantic,
            )
        {
            return;
        }

        // `obj[key][0]` where `obj` is a same-scope `const` bound to an object
        // literal whose every property value is a non-empty array literal.
        // Whichever array the dynamic `key` selects is statically non-empty, so the
        // first-element read is in-bounds — the rule's "empty array yields
        // `undefined`" concern cannot apply, since no value in the object is an
        // empty array. Covers the relative-time formatter idiom
        // `const units = { days: ["day", "days"], … }; units[unit][0]`.
        if is_first
            && let Expression::ComputedMemberExpression(inner) = &member.object
            && let Expression::Identifier(obj_ident) = &inner.object
            && resolves_to_const_object_with_nonempty_arrays(
                node,
                obj_ident.name.as_str(),
                semantic,
            )
        {
            return;
        }

        // `parts[0]` / `parts[parts.length - 1]` where `parts` is a same-scope
        // `const` bound to a `String.prototype.split` call (`str.split(sep)`).
        // `split` is specified to always return an array with at least one element
        // (even `''.split(',')` yields `['']`), so both the first and last reads
        // are in-bounds with no length guard. Covers the file-extension /
        // path-splitting idiom `const parts = name.split('.'); parts[parts.length - 1]`.
        if (is_first || is_last)
            && let Expression::Identifier(obj_ident) = &member.object
            && resolves_to_split_call(node, obj_ident.name.as_str(), semantic)
        {
            return;
        }

        // `p[0]` where `p`'s binding has a literal tuple type annotation
        // (`p: [number, number]`, `readonly [A, B]`) with at least one element.
        // A fixed-length tuple guarantees the first element exists, so the read
        // is in-bounds with no runtime guard. Resolved syntactically from the
        // annotation on the receiver's parameter/variable declaration; an aliased
        // tuple (`p: LineSegment<T>`) can't be resolved without type info and
        // stays flagged.
        if is_first
            && let Expression::Identifier(obj_ident) = &member.object
            && resolves_to_nonempty_tuple_type(obj_ident, semantic)
        {
            return;
        }

        // `obj.field[0]` where `field` is declared as a non-empty tuple member on
        // `obj`'s type — e.g. `prop.embedded[0]` with `interface Prop { embedded?:
        // [string, string] }`. The indexed receiver is a `StaticMemberExpression`,
        // not an identifier, so [`resolves_to_nonempty_tuple_type`] (which requires
        // an identifier receiver) never runs. Resolve `obj`'s declared type, look up
        // `field`, and skip when that member is a fixed-length tuple — the
        // member-access counterpart of the identifier-receiver exemption above.
        if is_first && member_access_receiver_is_nonempty_tuple_member(&member.object, semantic) {
            return;
        }

        // `r[0]` where `r` is an UNANNOTATED `const` whose initializer is a call to
        // a same-file function with a non-empty tuple return-type annotation
        // (`const skipInvalidParam = (…): [number, boolean] => …; const r =
        // skipInvalidParam(…); r[0]`). TypeScript infers `r` as that tuple, so the
        // first-element read is in-bounds. The callee is resolved to its declaration
        // via the symbol table (same file only); a `[A, B] | undefined` return type
        // qualifies when every non-nullish member is a non-empty tuple — a
        // possibly-`undefined` receiver is a nullish-access concern outside this
        // rule's empty-array scope. An imported, unresolved, array-returning, or
        // unannotated callee stays flagged.
        if is_first
            && let Expression::Identifier(obj_ident) = &member.object
            && resolves_to_nonempty_tuple_from_call_return(obj_ident, semantic)
        {
            return;
        }

        // `a[0]` where `a` is an UNANNOTATED callback parameter of an element-typed
        // iteration method (`.sort`/`.map`/`.forEach`/`.filter`/`.find`), or
        // `section[0]` where `section` is an UNANNOTATED `for...of` binding — and
        // the receiver/iterable array declares a non-empty tuple element type
        // (e.g. `Record<string, [string, string[]][]>`). TypeScript infers the
        // binding as that tuple element, so `[0]` is always in-bounds even without
        // an explicit annotation on the binding. The element type is derived purely
        // syntactically from the source array's declared type; any unresolved hop
        // falls back to flagging.
        if is_first
            && let Expression::Identifier(obj_ident) = &member.object
            && (sort_callback_param_tuple_element(obj_ident, semantic)
                || for_of_tuple_element(node, obj_ident.name.as_str(), semantic))
        {
            return;
        }

        // `val[0]` where `val` is the callback parameter of a Vue
        // `watch([e0, …, eN-1], cb)` whose SOURCE (first argument) is a non-empty
        // array literal of `N` elements. Vue types that parameter as a fixed-length
        // `N`-tuple matching the sources, not a dynamic `T[]`, so index 0 is always
        // in-bounds — the array can never be empty. Recognized structurally from the
        // `watch` call + its array-literal first argument + the enclosing callback's
        // parameter; a non-array-literal source (`watch(singleRef, cb)`) or an
        // empty-array source (`watch([], cb)`) is not a fixed-length tuple and stays
        // flagged.
        if is_first
            && let Expression::Identifier(obj_ident) = &member.object
            && is_watch_array_literal_source_callback_param(node, obj_ident.name.as_str(), semantic)
        {
            return;
        }

        // `v[0]` / `v[v.length - 1]` where `v`'s binding is annotated with a
        // gl-matrix fixed-size vector/matrix type (`vec2`/`vec3`/`vec4`,
        // `mat2`/`mat3`/`mat4`/`mat2d`, `quat`/`quat2`) imported from `gl-matrix`.
        // Those aliases denote fixed-length tuples (`vec2` = `[number, number]`,
        // `mat4` = a 16-element tuple) that are always at least two elements long,
        // so both the first- and last-element reads are in-bounds. The aliases are
        // `TSTypeReference`s, which the literal-tuple guard above cannot resolve,
        // so they are recognized by name — but only when the type name actually
        // resolves to a `gl-matrix` import, so a same-named local type can't
        // trigger the exemption.
        if (is_first || is_last)
            && let Expression::Identifier(obj_ident) = &member.object
            && resolves_to_glmatrix_fixed_type(obj_ident, semantic)
        {
            return;
        }

        // `match[0]` after a null guard, where `match` is a `RegExp.prototype.exec`
        // or `String.prototype.match` result. A non-null exec/match result is a
        // `RegExpExecArray`/`RegExpMatchArray` whose index 0 (the full match) is
        // always present — never an empty array — so the first-element read is
        // in-bounds once the `if (!match) return` / `=== null` guard has passed.
        if is_first
            && let Expression::Identifier(obj_ident) = &member.object
            && resolves_to_regex_match(node, obj_ident.name.as_str(), semantic)
            && has_preceding_nullish_exit_guard(node, obj_ident.name.as_str(), semantic)
        {
            return;
        }

        // `m[0]` inside the truthy branch of a same-variable truthiness guard on
        // `m` — `m ? m[0] : d`, `m && m[0]`, or `if (m) { m[0] }` — where `m` is
        // bound to a `RegExp.prototype.exec` / `String.prototype.match` call. A
        // non-null exec/match result is a `RegExpExecArray`/`RegExpMatchArray`
        // whose index 0 (the full match) always exists, and the truthiness test
        // discards the `null` case, so the first-element read is in-bounds — the
        // ternary/`&&` equivalent of the `if (!m) return; m[0]` null-exit guard
        // above. The exec/match provenance is essential: a bare truthy-guarded
        // array index stays flagged because an empty array (`[]`) is truthy, so
        // truthiness alone does not prove non-emptiness for an arbitrary array.
        if is_first
            && let Expression::Identifier(obj_ident) = &member.object
            && resolves_to_regex_match(node, obj_ident.name.as_str(), semantic)
            && reference_in_truthy_narrowed_branch(
                node.id(),
                node.kind().span(),
                obj_ident.name.as_str(),
                semantic.nodes(),
            )
        {
            return;
        }

        // `m[0]` inside the body of an enclosing `while (<test>)` whose `<test>`
        // proves `m` is non-null on every iteration — `m != null`, `m !== null`,
        // or a bare truthy `m` — where `m` is bound to a `RegExp.prototype.exec` /
        // `String.prototype.match` call. This is the canonical exec/match
        // consumption loop `let m = re.exec(s); while (m != null) { …m[0]…; m = re.exec(s); }`:
        // the loop condition IS the null narrowing (no separate `if (!m) …` guard),
        // and a non-null exec/match result is a `RegExpExecArray`/`RegExpMatchArray`
        // whose index 0 (the full match) always exists, so the first-element read is
        // in-bounds. The exec/match provenance is essential — a bare
        // `while (arr != null) { arr[0] }` on an arbitrary array stays flagged,
        // since a non-null empty array (`[]`) still has no index 0.
        if is_first
            && let Expression::Identifier(obj_ident) = &member.object
            && resolves_to_regex_match(node, obj_ident.name.as_str(), semantic)
            && is_in_while_non_null_loop_on(node, obj_ident.name.as_str(), semantic)
        {
            return;
        }

        // `match[0]` where `match` is the element bound by
        // `for (const match of <expr>.matchAll(...))`. Each element yielded by
        // `String.prototype.matchAll` is a `RegExpMatchArray` whose index 0 (the
        // full match) is always present, and the loop body runs only for a
        // successful match — so the first-element read is in-bounds with no null
        // guard needed (unlike a nullable `.exec()` / `.match()` result).
        if is_first
            && let Expression::Identifier(obj_ident) = &member.object
            && is_matchall_for_of_element(node, obj_ident.name.as_str(), semantic)
        {
            return;
        }

        // `e[0]` where `e` is bound to an element of an entries iterator whose
        // elements are `[K, V]` two-tuples: the loop variable of
        // `for (const e of <src>)`, or a callback parameter of a `.map`/`.forEach`/…
        // callback (its single element) or a `.sort`/`.toSorted` comparator (both
        // `(a, b)` parameters), invoked on `<src>`. `<src>` is `Object.entries(x)`,
        // a provable `Map`/`Set` instance's `.entries()`, or either of those wrapped
        // in `Array.from(...)` / an array-spread literal `[...]`. `Object.entries`
        // returns `Array<[string, T]>`, and `Map<K, V>`/`Set<T>` `.entries()` yields
        // `[K, V]`/`[T, T]`, so every element is a two-element tuple whose index 0
        // (the key) is always present — the first-element read is in-bounds with no
        // length guard. Scoped to provable tuple sources: `Object.keys` yields
        // `string` elements (where `[0]` is a possibly-out-of-bounds character index),
        // `Object.values` yields scalar `T`, and an untyped `foo.entries()` receiver
        // is unresolved — all stay flagged.
        if is_first
            && let Expression::Identifier(obj_ident) = &member.object
            && (is_entries_for_of_element(node, obj_ident.name.as_str(), semantic)
                || is_entries_callback_param(node, obj_ident.name.as_str(), semantic))
        {
            return;
        }

        // Cypress idiom: `$el[0]` inside a `.then(($el) => ...)` callback unwraps the
        // underlying DOM node from the jQuery wrapper. Cypress invokes the callback
        // only when the queried element exists (it fails the test otherwise), so the
        // index is always present.
        if let Expression::Identifier(obj_ident) = &member.object
            && obj_ident.name.starts_with('$')
            && is_then_callback_param(node, obj_ident.name.as_str(), semantic)
        {
            return;
        }

        // `const x = arr[0]` / `const x = arr[arr.length - 1]` where the binding
        // `x` is null/undefined-guarded before any unguarded use. An out-of-bounds
        // read yields `undefined`, which the guard already handles, so the access
        // is defensively written, not an accidental unchecked read. Covers two
        // idioms: an early-exit guard following the binding (`if (!x) return`) and
        // every use of `x` being individually guarded (`x?.`, `x ?? d`, or inside
        // an `if (x && …)` truthy narrowing). See [`result_binding_is_null_guarded`].
        if (is_first || is_last) && result_binding_is_null_guarded(node, semantic) {
            return;
        }

        // `word[0]` / `word[word.length - 1]` where `word` is a `string`-typed
        // binding guarded by a preceding `if (!word) return/throw`. A string is
        // falsy exactly when empty, so the truthiness early-exit proves the string
        // is non-empty at the access — both the first and last reads are in-bounds.
        // Scoped to a `string` annotation: an array's truthiness says nothing about
        // its length (`[]` is truthy), so the same guard would not bound an array.
        if (is_first || is_last)
            && let Expression::Identifier(obj_ident) = &member.object
            && binding_has_string_type(obj_ident, semantic)
            && has_preceding_nullish_exit_guard(node, obj_ident.name.as_str(), semantic)
        {
            return;
        }

        // `str[0]` / `str[str.length - 1]` inside the truthy branch of a
        // same-variable truthiness guard on `str` — `str ? …str[0]… : …`,
        // `str && …str[0]…`, or `if (str) { …str[0]… }`. A string is falsy
        // exactly when empty, so a truthy `str` is non-empty and the boundary
        // read is in-bounds. Restricted to strings: an array is truthy even when
        // empty (`[]`), so a truthy-guarded array index stays flagged. String
        // evidence is a `string` annotation on the binding, or a string method
        // (`.toUpperCase()` / `.slice()` / …) called on the same variable inside
        // the guarded branch (the `str ? str[0].toUpperCase() + str.slice(1) : ""`
        // idiom, where `str` is a generic `S extends string` with no plain
        // `string` annotation).
        if (is_first || is_last)
            && let Expression::Identifier(obj_ident) = &member.object
            && is_in_same_var_truthy_string_guard(node, obj_ident, semantic)
        {
            return;
        }

        let which = if is_first { "first" } else { "last" };
        let at_arg = if is_first { "0" } else { "-1" };
        // Report at the opening `[` of this access, not at `member.span().start`.
        // A `ComputedMemberExpression`'s span starts at its object, so every link
        // of a chain like `a[0][0][0]` would otherwise share one position and
        // collapse into duplicate diagnostics. The bracket offset is distinct per
        // access and points at the actual index site.
        let bracket_offset = open_bracket_offset(member, source);
        let (line, column) = byte_offset_to_line_col(source, bracket_offset);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "boundary-condition".into(),
            message: format!(
                "Unchecked access to the {which} element — on an empty array this is `undefined`. \
                 Guard with `if ({obj_text}.length)`, use `{obj_text}.at({at_arg})`, or add a `?? fallback`."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

fn expr_text<'a>(expr: &'a Expression, source: &'a str) -> &'a str {
    let start = expr.span().start as usize;
    let end = expr.span().end as usize;
    &source[start..end]
}

/// Names of the jest/vitest mock-introspection arrays hung off `<spy>.mock`.
const MOCK_INTROSPECTION_ARRAYS: [&str; 3] = ["calls", "results", "instances"];

/// Returns true when `object` is (or is a further index into) a jest/vitest
/// mock-introspection array — a member chain ending in `.mock.calls`,
/// `.mock.results`, or `.mock.instances`. Indexing these framework-managed
/// arrays (`spy.mock.calls[0]`, and a nested `spy.mock.calls[0][1]` into a call
/// entry) is an idiomatic test read, not an unguarded out-of-bounds access.
///
/// Recognized structurally on the AST so it never matches a project-specific
/// identifier: any number of trailing computed accesses are peeled off first
/// (covers `.mock.calls[0]` being indexed again), then the underlying static
/// member chain must read `<expr>.mock.<calls|results|instances>`.
fn object_is_mock_introspection_array(object: &Expression) -> bool {
    let mut current = object;
    // Peel trailing computed accesses (`...[0]`, `...[0][1]`) to reach the
    // static `.mock.<array>` chain underneath.
    while let Expression::ComputedMemberExpression(computed) = current {
        current = &computed.object;
    }
    let Expression::StaticMemberExpression(array_member) = current else {
        return false;
    };
    if !MOCK_INTROSPECTION_ARRAYS.contains(&array_member.property.name.as_str()) {
        return false;
    }
    matches!(
        &array_member.object,
        Expression::StaticMemberExpression(mock_member)
            if mock_member.property.name.as_str() == "mock"
    )
}

/// Byte offset of the opening `[` of a computed access. The bracket sits after
/// the object (skipping any whitespace and an optional `?.`); falls back to the
/// object's end if no `[` is found, which never happens for valid input.
fn open_bracket_offset(member: &ComputedMemberExpression, source: &str) -> usize {
    let object_end = member.object.span().end as usize;
    source[object_end..member.span().end as usize]
        .find('[')
        .map_or(object_end, |rel| object_end + rel)
}

fn is_zero_index(expr: &Expression, source: &str) -> bool {
    if let Expression::NumericLiteral(lit) = expr {
        let text = &source[lit.span.start as usize..lit.span.end as usize];
        return text == "0";
    }
    false
}

/// Check if index has shape `<object_text>.length - 1`.
fn is_last_index(expr: &Expression, object_text: &str, source: &str) -> bool {
    let Expression::BinaryExpression(bin) = expr else {
        return false;
    };
    if !matches!(bin.operator, BinaryOperator::Subtraction) {
        return false;
    }
    // Right must be `1`
    let Expression::NumericLiteral(right) = &bin.right else {
        return false;
    };
    let right_text = &source[right.span.start as usize..right.span.end as usize];
    if right_text != "1" {
        return false;
    }
    // Left must be `<object>.length`
    let Expression::StaticMemberExpression(left_member) = &bin.left else {
        return false;
    };
    if left_member.property.name.as_str() != "length" {
        return false;
    }
    let left_obj_text = expr_text(&left_member.object, source);
    left_obj_text == object_text
}

/// Returns true when the index-access `node` is the base of an optional
/// member, computed, or call access — `arr[0]?.prop`, `arr[0]?.[i]`, or
/// `arr[0]?.()`. The `?.` on the consumer explicitly handles `arr[0]` being
/// `undefined`, so the inner read is not an accidental unchecked access.
///
/// Only the access that uses `node` as its base counts: the parent must be an
/// optional access whose own base span equals `node`'s span. An optional access
/// elsewhere in an enclosing expression (e.g. `node` as a call argument) does
/// not vouch the read safe.
fn result_consumed_by_optional_access(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let parent_id = nodes.parent_id(node.id());
    if parent_id == node.id() {
        return false;
    }
    let node_span = node.kind().span();
    match nodes.get_node(parent_id).kind() {
        AstKind::StaticMemberExpression(member) => {
            member.optional && member.object.span() == node_span
        }
        AstKind::ComputedMemberExpression(member) => {
            member.optional && member.object.span() == node_span
        }
        AstKind::CallExpression(call) => call.optional && call.callee.span() == node_span,
        _ => false,
    }
}

/// Returns true when the index-access `node` is the operand of a `typeof`
/// operator (`typeof arr[0]`, `typeof obj[key]`, `typeof (arr[0])`). `typeof`
/// returns the string `"undefined"` for an out-of-bounds (`undefined`) read and
/// never throws, so the possibly-empty array does not produce a boundary
/// violation — the surrounding type-guard comparison just evaluates false.
///
/// Climbs only through `ParenthesizedExpression` wrappers (so a parenthesized
/// operand still counts), then requires the immediate enclosing node to be a
/// `UnaryExpression` whose operator is `typeof` and whose argument is exactly
/// this access. The argument span is matched against the climbed node's span, so
/// the access must BE the operand — `typeof x === arr[0]` (where `arr[0]` is a
/// comparison operand, not the `typeof` operand) and `typeof arr[0].length`
/// (where the operand is `arr[0].length`, a value read of `arr[0]`) stay flagged.
fn is_typeof_operand(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = node.id();
    let mut current_span = node.kind().span();
    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return false;
        }
        let parent = nodes.get_node(parent_id);
        match parent.kind() {
            AstKind::ParenthesizedExpression(paren) => {
                current_span = paren.span;
                current_id = parent_id;
            }
            AstKind::UnaryExpression(unary) => {
                return unary.operator == UnaryOperator::Typeof
                    && unary.argument.span() == current_span;
            }
            _ => return false,
        }
    }
}

/// Returns true when the index-access `node` is the operand of a TypeScript
/// non-null assertion (`arr[0]!`, `(arr[0])!`, `arr[arr.length - 1]!`). The `!`
/// is the developer's explicit assertion that the element is present, so the read
/// is not an accidental unchecked access — the same dismissal signal as `.at(0)`
/// or a `?? fallback`.
///
/// Climbs only through `ParenthesizedExpression` wrappers (so a parenthesized
/// operand still counts), then requires the immediate enclosing node to be a
/// `TSNonNullExpression` whose asserted expression is exactly this access. The
/// asserted-expression span is matched against the climbed node's span, so the
/// access must BE the `!` operand — `arr[0].foo!` (where the `!` applies to the
/// outer `.foo` member, a value read of `arr[0]`) stays flagged.
fn is_non_null_asserted(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = node.id();
    let mut current_span = node.kind().span();
    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return false;
        }
        let parent = nodes.get_node(parent_id);
        match parent.kind() {
            AstKind::ParenthesizedExpression(paren) => {
                current_span = paren.span;
                current_id = parent_id;
            }
            AstKind::TSNonNullExpression(non_null) => {
                return non_null.expression.span() == current_span;
            }
            _ => return false,
        }
    }
}

/// Returns true when the index-access `node` is the DIRECT left or right operand
/// of an equality/inequality comparison (`===`, `!==`, `==`, `!=`) whose OTHER
/// operand is neither the identifier `undefined` nor a `NullLiteral` —
/// `arr[0] === 'h'`, `path[0] !== '/'`, `(value[0]) === 'h'`. An out-of-bounds
/// index yields `undefined`, and comparing `undefined` against a non-nullish
/// value never throws and produces the correct "no match" result, so the
/// possibly-empty array is harmless — the same rationale as [`is_typeof_operand`].
///
/// Climbs only through `ParenthesizedExpression` wrappers (so a parenthesized
/// operand still counts), then requires the immediate enclosing node to be a
/// `BinaryExpression` with an equality operator. "Direct operand" is enforced by
/// the parent walk: `arr[0].foo === x` has `arr[0]` as the object of a member
/// access (its parent is a member expression, not the comparison), so it stays
/// flagged — reading `undefined.foo` would throw. An `undefined` / `null` other
/// operand is rejected: `arr[0] === undefined` is a deliberate emptiness check,
/// not a comparison against a concrete value.
fn is_equality_comparison_operand(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = node.id();
    let mut current_span = node.kind().span();
    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return false;
        }
        let parent = nodes.get_node(parent_id);
        match parent.kind() {
            AstKind::ParenthesizedExpression(paren) => {
                current_span = paren.span;
                current_id = parent_id;
            }
            AstKind::BinaryExpression(bin) => {
                if !matches!(
                    bin.operator,
                    BinaryOperator::StrictEquality
                        | BinaryOperator::StrictInequality
                        | BinaryOperator::Equality
                        | BinaryOperator::Inequality
                ) {
                    return false;
                }
                // The climbed node is a direct child of the comparison, so it is
                // exactly one of the operands; the other is what it is compared to.
                let other = if bin.left.span() == current_span {
                    &bin.right
                } else {
                    &bin.left
                };
                return match other {
                    Expression::NullLiteral(_) => false,
                    Expression::Identifier(id) if id.name.as_str() == "undefined" => false,
                    _ => true,
                };
            }
            _ => return false,
        }
    }
}

fn is_assignment_target(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let parent_id = nodes.parent_id(node.id());
    if parent_id == node.id() {
        return false;
    }
    let parent = nodes.get_node(parent_id);
    // The ComputedMemberExpression is wrapped in a MemberExpression parent
    // in AstKind, so check its parent for assignments
    match parent.kind() {
        AstKind::AssignmentExpression(assign) => {
            // Check the node span overlaps the left side
            let left_start = assign.left.span().start;
            let left_end = assign.left.span().end;
            let node_span = node.kind().span();
            node_span.start >= left_start && node_span.end <= left_end
        }
        _ => false,
    }
}

fn has_nullish_or_logical_fallback(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = node.id();
    for _ in 0..6 {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return false;
        }
        let parent = nodes.get_node(parent_id);
        match parent.kind() {
            AstKind::ParenthesizedExpression(_) | AstKind::TSNonNullExpression(_) => {
                current_id = parent_id;
                continue;
            }
            AstKind::LogicalExpression(logical) => {
                if matches!(
                    logical.operator,
                    LogicalOperator::Coalesce | LogicalOperator::Or
                ) {
                    // Must be the left operand
                    let left_end = logical.left.span().end;
                    let node_span = node.kind().span();
                    if node_span.end <= left_end {
                        return true;
                    }
                }
                return false;
            }
            _ => return false,
        }
    }
    false
}

/// Returns true when an ancestor `if` or ternary condition proves this access is
/// in-bounds. Recognized guards:
///   1. in an enclosing `if` condition, any `.length` check, or a `<obj_text>`
///      non-emptiness test recognized by [`test_proves_nonempty`] — notably a
///      truthy `<obj_text>.some(...)` / `<obj_text>?.some(...)` on the same array
///      (`Array.prototype.some` is `false` for `[]`). Both cover first and last
///      reads;
///   2. for a first-element read (`is_first`), a truthy `arr[0]` / `arr?.[0]`
///      check on the same array (`obj_text`) — the truthiness equivalent of
///      `if (arr.length)`. This also exempts the guard condition's own `[0]`
///      access, which sits inside its enclosing `if.test`.
///   3. in a ternary (`cond ? <consequent> : <alternate>`), an access in the
///      truthy `consequent` branch — which runs only when `cond` held — guarded
///      either by a `<obj_text>.length` check in the condition, or (for a
///      first-element read) by a truthy `arr[0]` / `arr?.[0]` test on the same
///      array. The truthy `arr[0]` test also exempts its OWN `[0]` access in the
///      condition: it is the ternary equivalent of `if (arr[0])`. The `.length`
///      check is scoped to `obj_text` because an unrelated `.length` mention in
///      the condition would not bound this array. `Array.isArray(obj_text)`
///      alone is NOT a guard: it proves array-ness, not non-emptiness, and the
///      empty array still yields `undefined` at index 0. The `alternate` (falsy)
///      branch stays flagged — it runs when the test is falsy, so the element
///      may be absent.
///   4. in an `&&` chain, two short-circuit guards (the right operand runs only
///      when the left is truthy):
///        a. a `<obj_text>.length` check in the LEFT operand exempts an access in
///           the RIGHT operand (`arr.length && arr[0]`), the expression form of
///           `if (arr.length)`. It does not exempt an access in the LEFT operand
///           itself (that runs before the guard).
///        b. for a first-element read, a truthy `<obj_text>[0]` test in the LEFT
///           operand exempts a `[0]` read in EITHER operand: the LEFT read is the
///           truthiness test itself (`arr[0] && …`, evaluated for truthiness only,
///           short-circuiting harmlessly on an empty array) and the RIGHT read
///           runs only after it (`arr[0] && use(arr[0])`) — the `&&` form of
///           `if (arr[0])`.
///      Scoped to the SAME array: a different array in the left
///      (`foo.length && bar[0]`, `other[0] && use(arr[0])`) stays flagged. Only
///      `&&` qualifies; `||` / `??` short-circuit on a falsy left and do not
///      prove the element present.
///   5. in the body of an enclosing `while (<test>)` / `for (…; <test>; …)`,
///      an access whose `<test>` proves `<obj_text>` is non-empty
///      (`<obj_text>.length`, `.length > 0`, `>= 1`, `!== 0`, `=== N` for `N >= 1`,
///      or the mirrored `0 < <obj_text>.length` — see [`test_proves_nonempty`]),
///      or (for a first-element read) a truthy `<obj_text>[0]` test. The loop
///      condition is re-evaluated before each iteration, so it dominates every
///      synchronous point in the body. Scoped to the SAME receiver array on the `.length`
///      side; a non-empty check on a different binding does not exempt. A
///      `do { … } while (<test>)` is NOT recognized — its body runs once before
///      the test, so the test does not dominate the first iteration.
fn has_length_guard_ancestor(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    obj_text: &str,
    is_first: bool,
    source: &str,
) -> bool {
    let nodes = semantic.nodes();
    let node_span = node.kind().span();
    let mut current_id = node.id();
    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return false;
        }
        let parent = nodes.get_node(parent_id);
        match parent.kind() {
            AstKind::IfStatement(if_stmt) => {
                let cond_text = &source
                    [if_stmt.test.span().start as usize..if_stmt.test.span().end as usize];
                if normalize_optional_chaining(cond_text).contains(".length") {
                    return true;
                }
                // A dominating `if (arr.some(...))` proves `arr` is non-empty in the
                // branch (`Array.prototype.some` is `false` for `[]`), so both `arr[0]`
                // and `arr[arr.length - 1]` are in-bounds — the same non-emptiness
                // oracle the enclosing `while`/`for` arms consult.
                if test_proves_nonempty(&if_stmt.test, obj_text, source) {
                    return true;
                }
                if is_first && condition_guards_index0(&if_stmt.test, obj_text, source, false) {
                    return true;
                }
            }
            AstKind::ConditionalExpression(cond) => {
                let in_consequent = cond.consequent.span().start <= node_span.start
                    && node_span.end <= cond.consequent.span().end;
                let in_test = cond.test.span().start <= node_span.start
                    && node_span.end <= cond.test.span().end;
                let in_alternate = cond.alternate.span().start <= node_span.start
                    && node_span.end <= cond.alternate.span().end;
                if in_consequent || in_test {
                    let cond_text = &source
                        [cond.test.span().start as usize..cond.test.span().end as usize];
                    // `.length` guard applies to the truthy consequent. Optional
                    // chaining is normalized on both sides so `arr?.length` matches.
                    // A `<obj_text>.length === 0` (or `< 1` / `<= 0`) test is the
                    // exception: its truthy branch is the EMPTY case, so the consequent
                    // access stays flagged.
                    if in_consequent
                        && normalize_optional_chaining(cond_text).contains(&format!(
                            "{}.length",
                            normalize_optional_chaining(obj_text)
                        ))
                        && !is_length_zero_check(&cond.test, obj_text, source)
                    {
                        return true;
                    }
                    // A truthy `arr[0]` / `arr?.[0]` test narrows the consequent AND
                    // exempts the test's own `[0]` access — the ternary equivalent of
                    // `if (arr[0])`. The alternate branch stays flagged.
                    if is_first && condition_guards_index0(&cond.test, obj_text, source, false) {
                        return true;
                    }
                }
                // `cond ? <consequent> : <alternate>` — the alternate runs only when
                // `cond` is FALSY. When `cond` is `<obj_text>.length === 0` (or `< 1`
                // / `<= 0`), possibly one OR-disjunct among others, its falsity forces
                // every disjunct false, so `<obj_text>.length >= 1` and the access is
                // in-bounds. The dual of the consequent `.length` guard.
                if in_alternate
                    && alternate_guarded_by_empty_length(&cond.test, obj_text, source)
                {
                    return true;
                }
            }
            AstKind::LogicalExpression(logical) => {
                // `arr.length && …arr[0]…` — the `&&` short-circuit evaluates the
                // right operand only when the left operand is truthy, so a
                // `{obj_text}.length` check on the SAME array in the left operand
                // proves the array is non-empty wherever this access sits in the
                // right operand. The expression equivalent of `if (arr.length) { arr[0] }`.
                // Scoped, like the `if`/ternary arms, to a `.length` on the same
                // object: `foo.length && bar[0]` (a different array) stays flagged.
                // This `.length` guard reaches only the RIGHT operand; a truthy
                // `arr[0]` LEFT guard is handled separately below.
                if matches!(logical.operator, LogicalOperator::And) {
                    let in_right = logical.right.span().start <= node_span.start
                        && node_span.end <= logical.right.span().end;
                    let in_left = logical.left.span().start <= node_span.start
                        && node_span.end <= logical.left.span().end;
                    if in_right {
                        let left_text = &source[logical.left.span().start as usize
                            ..logical.left.span().end as usize];
                        if normalize_optional_chaining(left_text).contains(&format!(
                            "{}.length",
                            normalize_optional_chaining(obj_text)
                        )) {
                            return true;
                        }
                    }
                    // A truthy `arr[0]` test in the LEFT operand is the `&&` form of
                    // `if (arr[0])`. The LEFT operand is evaluated for truthiness only,
                    // so a first-element read there short-circuits harmlessly to
                    // `undefined` on an empty array (`arr[0] && use`); and the RIGHT
                    // operand runs only after that truthy test, so a first-element read
                    // there is in-bounds (`arr[0] && use(arr[0])`). Both reuse the
                    // ternary-consequent predicate on `logical.left`: a different array
                    // in the left (`other[0] && use(arr[0])`) does not match, so it
                    // stays flagged. Only `&&` qualifies — `||` / `??` short-circuit on
                    // a FALSY left and do not prove the element present.
                    if is_first
                        && (in_left || in_right)
                        && condition_guards_index0(&logical.left, obj_text, source, false)
                    {
                        return true;
                    }
                }
            }
            AstKind::WhileStatement(while_stmt) => {
                if span_contains(while_stmt.body.span(), node_span) {
                    if test_proves_nonempty(&while_stmt.test, obj_text, source) {
                        return true;
                    }
                    if is_first
                        && condition_guards_index0(&while_stmt.test, obj_text, source, false)
                    {
                        return true;
                    }
                }
            }
            AstKind::ForStatement(for_stmt) => {
                if span_contains(for_stmt.body.span(), node_span)
                    && let Some(test) = &for_stmt.test
                {
                    if test_proves_nonempty(test, obj_text, source) {
                        return true;
                    }
                    if is_first && condition_guards_index0(test, obj_text, source, false) {
                        return true;
                    }
                }
            }
            _ => {}
        }
        current_id = parent_id;
    }
}

/// Returns true when `test` — the condition of an enclosing `while`/`for` loop or
/// a dominating `if` whose branch dominates the access — proves `<obj_text>.length
/// >= 1` (the array is non-empty in the guarded region). Recognized on the SAME
/// receiver array:
///   - a bare truthy `<obj_text>.length` (zero is falsy, so a truthy length is `>= 1`);
///   - `<obj_text>.length > 0` / `>= 1` / `!== 0` / `=== N` (`N >= 1`) and the
///     mirrored `0 < <obj_text>.length` — delegated to
///     [`length_comparison_proves_nonempty`];
///   - a truthy `<obj_text>.some(pred)` / `<obj_text>?.some(pred)` call —
///     `Array.prototype.some` returns `false` for an empty array, so a truthy
///     result proves at least one element exists (see [`array_some_call_matches`]);
///   - any conjunct of an `&&` chain that proves one of the above — every conjunct
///     holds inside the guarded region, so it suffices for one to bound the length.
/// `||` is NOT traversed: a disjunct need not hold when the whole test is true.
fn test_proves_nonempty(test: &Expression, obj_text: &str, source: &str) -> bool {
    match test {
        Expression::ParenthesizedExpression(paren) => {
            test_proves_nonempty(&paren.expression, obj_text, source)
        }
        Expression::LogicalExpression(logical)
            if logical.operator == LogicalOperator::And =>
        {
            test_proves_nonempty(&logical.left, obj_text, source)
                || test_proves_nonempty(&logical.right, obj_text, source)
        }
        Expression::StaticMemberExpression(_) => is_length_of(test, obj_text, source),
        Expression::CallExpression(call) => array_some_call_matches(call, obj_text, source),
        Expression::ChainExpression(chain) => matches!(
            &chain.expression,
            ChainElement::CallExpression(call)
                if array_some_call_matches(call, obj_text, source)
        ),
        _ => length_comparison_proves_nonempty(test, obj_text, source),
    }
}

/// Returns true when `call` is `<obj_text>.some(...)` / `<obj_text>?.some(...)` on
/// the SAME receiver — its callee is a `.some` member access whose receiver text
/// (optional chaining normalized) equals `<obj_text>`. `Array.prototype.some`
/// returns `true` only for a non-empty array (`[].some(_)` is `false`), so a truthy
/// `arr.some(...)` test proves `arr.length >= 1`, the same non-emptiness signal as a
/// truthy `arr.length`. A `.some(...)` on a different array does not match.
fn array_some_call_matches(call: &CallExpression, obj_text: &str, source: &str) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    member.property.name.as_str() == "some"
        && normalize_optional_chaining(expr_text(&member.object, source))
            == normalize_optional_chaining(obj_text)
}

/// Returns true when `expr` (an `if`/ternary condition, in its positive form when
/// `negated` is false) proves a zero-index access `obj_text[0]` / `obj_text?.[0]`
/// is in-bounds. `obj_text` is matched after stripping optional-chaining `?.` to
/// `.` on both sides. Recognized signals:
///   - a truthy `obj_text[0]` test — the truthiness equivalent of `if (arr.length)`;
///   - a predicate call that receives `obj_text[0]` as an argument
///     (`isPlainObject(arr[0])`, `Array.isArray(arr[0])`): the positive branch runs
///     only when the first element existed and satisfied the predicate. The callee
///     is not matched by name — the structural signal is `arr[0]` appearing as a
///     call argument.
///   - an equality (`===` / `==`) or relational (`< > <= >=`) comparison one of whose
///     operands references `obj_text[0]` — `typeof arr[0] === 'string'`,
///     `arr[0] === x`, `arr[0] > 5`. A truthy result of such a comparison implies the
///     element is present: an absent element is `undefined`, which fails every
///     relational test and never equals a concrete value. Inequality (`!==` / `!=`) is
///     excluded — `undefined !== x` is `true`, so the branch runs for an absent
///     element — as is equality against `undefined` / `null`, which is an emptiness
///     check (mirrors [`is_equality_comparison_operand`]).
/// Recurses through the operators that preserve or carry the guard: `&&`, `||`, `!`,
/// equality/relational comparisons (into both operands), and parentheses. `negated`
/// tracks logical-NOT polarity so a predicate call under
/// `!` is NOT a guard: `if (!isPlainObject(arr[0])) { …arr[0]… }` narrows the branch
/// to the element being absent or failing the predicate, so the body access stays
/// flagged.
fn condition_guards_index0(
    expr: &Expression,
    obj_text: &str,
    source: &str,
    negated: bool,
) -> bool {
    match expr {
        Expression::ComputedMemberExpression(member) => {
            if is_zero_index(&member.expression, source)
                && normalize_optional_chaining(expr_text(&member.object, source))
                    == normalize_optional_chaining(obj_text)
            {
                return true;
            }
            condition_guards_index0(&member.object, obj_text, source, negated)
        }
        Expression::StaticMemberExpression(member) => {
            condition_guards_index0(&member.object, obj_text, source, negated)
        }
        Expression::ChainExpression(chain) => match &chain.expression {
            ChainElement::ComputedMemberExpression(member) => {
                if is_zero_index(&member.expression, source)
                    && normalize_optional_chaining(expr_text(&member.object, source))
                        == normalize_optional_chaining(obj_text)
                {
                    return true;
                }
                condition_guards_index0(&member.object, obj_text, source, negated)
            }
            ChainElement::StaticMemberExpression(member) => {
                condition_guards_index0(&member.object, obj_text, source, negated)
            }
            _ => false,
        },
        Expression::CallExpression(call) => {
            // A predicate call that receives `arr[0]` as an argument tests the first
            // element before the positive branch runs. Only a non-negated call is a
            // guard; under `!` the branch runs when the element is absent or fails the
            // predicate, so its body access stays flagged.
            if negated {
                return false;
            }
            call.arguments.iter().any(|arg| {
                arg.as_expression().is_some_and(|arg_expr| {
                    condition_guards_index0(arg_expr, obj_text, source, negated)
                })
            })
        }
        Expression::LogicalExpression(logical) => {
            condition_guards_index0(&logical.left, obj_text, source, negated)
                || condition_guards_index0(&logical.right, obj_text, source, negated)
        }
        Expression::BinaryExpression(binary) => {
            // A comparison guards the branch only when a truthy result implies the
            // `arr[0]` operand is present. Relational operators qualify: `undefined`
            // coerces to `NaN`, so `arr[0] > 5` (etc.) is false for an absent element.
            // Equality (`===` / `==`) qualifies unless the element is compared against
            // `undefined` / `null` — that is an emptiness check, not a presence guard.
            // Inequality (`!==` / `!=`) never qualifies: `undefined !== x` is `true`, so
            // the branch runs for an absent element. The `arr[0]` access is recognized
            // by the member arms above, whichever side of the comparison it sits on.
            match binary.operator {
                BinaryOperator::LessThan
                | BinaryOperator::LessEqualThan
                | BinaryOperator::GreaterThan
                | BinaryOperator::GreaterEqualThan => {
                    condition_guards_index0(&binary.left, obj_text, source, negated)
                        || condition_guards_index0(&binary.right, obj_text, source, negated)
                }
                BinaryOperator::StrictEquality | BinaryOperator::Equality => {
                    (condition_guards_index0(&binary.left, obj_text, source, negated)
                        && !is_nullish_literal(&binary.right))
                        || (condition_guards_index0(&binary.right, obj_text, source, negated)
                            && !is_nullish_literal(&binary.left))
                }
                _ => false,
            }
        }
        Expression::UnaryExpression(unary) => {
            let negated = if unary.operator == UnaryOperator::LogicalNot {
                !negated
            } else {
                negated
            };
            condition_guards_index0(&unary.argument, obj_text, source, negated)
        }
        Expression::ParenthesizedExpression(paren) => {
            condition_guards_index0(&paren.expression, obj_text, source, negated)
        }
        _ => false,
    }
}

/// Returns true when `expr` is the `undefined` identifier or a `null` literal.
/// Comparing `arr[0]` against either is a deliberate emptiness check, not a
/// presence guard, so it must not vouch a body access safe.
fn is_nullish_literal(expr: &Expression) -> bool {
    matches!(expr, Expression::NullLiteral(_))
        || matches!(expr, Expression::Identifier(id) if id.name.as_str() == "undefined")
}

/// Returns true when `expr` (a ternary condition) contains a `<obj_text>.length`
/// emptiness check (see [`is_length_zero_check`]) reachable from the root through
/// `||` (`LogicalOr`) connectives only. A ternary's alternate runs exactly when
/// the condition is falsy; with only `||` between the root and the check, that
/// falsity forces the check false, so `<obj_text>.length >= 1` and the access is
/// in-bounds. `&&` is NOT traversed: a check under `&&` need not be false when the
/// whole condition is false, so it does not prove non-emptiness.
fn alternate_guarded_by_empty_length(expr: &Expression, obj_text: &str, source: &str) -> bool {
    match expr {
        Expression::ParenthesizedExpression(paren) => {
            alternate_guarded_by_empty_length(&paren.expression, obj_text, source)
        }
        Expression::LogicalExpression(logical)
            if logical.operator == LogicalOperator::Or =>
        {
            alternate_guarded_by_empty_length(&logical.left, obj_text, source)
                || alternate_guarded_by_empty_length(&logical.right, obj_text, source)
        }
        _ => is_length_zero_check(expr, obj_text, source),
    }
}

/// Returns true when `expr` asserts `<obj_text>.length` is zero — the array is
/// empty: `<obj_text>.length === 0`, `== 0`, `< 1`, or `<= 0`.
fn is_length_zero_check(expr: &Expression, obj_text: &str, source: &str) -> bool {
    let Expression::BinaryExpression(binary) = expr else {
        return false;
    };
    if !is_length_of(&binary.left, obj_text, source) {
        return false;
    }
    match binary.operator {
        BinaryOperator::StrictEquality | BinaryOperator::Equality | BinaryOperator::LessEqualThan => {
            is_numeric_literal(&binary.right, 0, source)
        }
        BinaryOperator::LessThan => is_numeric_literal(&binary.right, 1, source),
        _ => false,
    }
}

/// Strips optional-chaining tokens so `data?.choices` and `data.choices` compare
/// equal. The condition writes the access with `?.`, the flagged in-block read
/// without it; both denote the same array.
fn normalize_optional_chaining(text: &str) -> String {
    text.replace("?.", ".")
}

/// Returns true when this access IS the discriminant expression of an enclosing
/// `switch` — `switch (arr[0]) { ... }`. The discriminant is read solely to be
/// compared against the `case` labels, so an out-of-bounds `undefined` matches no
/// case and falls through harmlessly; the switch dispatch never throws on it.
/// Parentheses and non-null assertions between the access and the switch are
/// transparent (`switch ((arr[0])!)`). Scoped to the discriminant position: an
/// `arr[0]` inside a `case` consequent is a different role and stays flagged.
fn is_switch_discriminant(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let node_span = node.kind().span();
    let mut current_id = node.id();
    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return false;
        }
        let parent = nodes.get_node(parent_id);
        match parent.kind() {
            AstKind::ParenthesizedExpression(_) | AstKind::TSNonNullExpression(_) => {
                current_id = parent_id;
            }
            AstKind::SwitchStatement(switch) => {
                let disc = switch.discriminant.span();
                return disc.start <= node_span.start && node_span.end <= disc.end;
            }
            _ => return false,
        }
    }
}

/// Returns true when an ancestor `switch (<obj_text>.length)` proves this access
/// is in-bounds. The discriminant must be the same array's `.length`, and the
/// enclosing case must guarantee a non-empty length: a `case N:` whose test is
/// a numeric literal `N >= 1`, or the `default:` arm when the switch also lists
/// an explicit `case 0:` (length 0 is handled there, so `default` implies
/// `length >= 1`). Both first (`arr[0]`) and last (`arr[arr.length - 1]`) reads
/// need only `length >= 1`, so the same predicate covers them.
fn has_switch_length_guard_ancestor(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    obj_text: &str,
    source: &str,
) -> bool {
    let nodes = semantic.nodes();
    let node_span = node.kind().span();
    for ancestor in nodes.ancestors(node.id()) {
        let AstKind::SwitchStatement(switch) = ancestor.kind() else {
            continue;
        };
        if !is_length_of(&switch.discriminant, obj_text, source) {
            continue;
        }
        let has_zero_case = switch
            .cases
            .iter()
            .any(|case| case.test.as_ref().is_some_and(|t| is_numeric_literal(t, 0, source)));
        for case in &switch.cases {
            if !case_contains_span(case, node_span) {
                continue;
            }
            return match &case.test {
                Some(test) => is_positive_numeric_literal(test, source),
                None => has_zero_case,
            };
        }
    }
    false
}

/// Returns true when `expr` is `<obj_text>.length`.
fn is_length_of(expr: &Expression, obj_text: &str, source: &str) -> bool {
    let Expression::StaticMemberExpression(member) = expr else {
        return false;
    };
    member.property.name.as_str() == "length" && expr_text(&member.object, source) == obj_text
}

/// Returns true when `expr` is the numeric literal `value`.
fn is_numeric_literal(expr: &Expression, value: u32, source: &str) -> bool {
    matches!(expr, Expression::NumericLiteral(lit)
        if source[lit.span.start as usize..lit.span.end as usize]
            .parse::<u32>()
            .is_ok_and(|n| n == value))
}

/// Returns true when `expr` is a numeric literal `>= 1`.
fn is_positive_numeric_literal(expr: &Expression, source: &str) -> bool {
    matches!(expr, Expression::NumericLiteral(lit)
        if source[lit.span.start as usize..lit.span.end as usize]
            .parse::<u32>()
            .is_ok_and(|n| n >= 1))
}

/// Returns true when `span` falls within any of `case`'s consequent statements.
fn case_contains_span(case: &SwitchCase, span: oxc_span::Span) -> bool {
    case.consequent
        .iter()
        .any(|stmt| stmt.span().start <= span.start && span.end <= stmt.span().end)
}

/// Returns true if `stmt` or a top-level statement within it is an early exit —
/// `return`, `throw`, `continue`, `break`, or a bare `.exit()` call such as
/// `process.exit(1)`. `continue`/`break` abandon the rest of the current
/// loop/switch execution just as `return`/`throw` abandon the function, so a
/// read reached only after such a statement fires is unreachable when the guard
/// takes it — the same non-emptiness guarantee behind a subsequent `arr[0]`.
fn body_has_early_exit(stmt: &Statement) -> bool {
    match stmt {
        Statement::ReturnStatement(_)
        | Statement::ThrowStatement(_)
        | Statement::ContinueStatement(_)
        | Statement::BreakStatement(_) => true,
        Statement::ExpressionStatement(expr_stmt) => {
            if let Expression::CallExpression(call) = &expr_stmt.expression {
                if let Expression::StaticMemberExpression(member) = &call.callee {
                    return member.property.name.as_str() == "exit";
                }
            }
            false
        }
        Statement::BlockStatement(block) => block.body.iter().any(body_has_early_exit),
        _ => false,
    }
}

/// Equality matchers that, applied to `expect(<arr>.length)`, assert the length
/// equals their argument. The array is proven non-empty only when that argument
/// is `>= 1` — `expect(arr.length).toEqual(0)` asserts the array is EMPTY, so it
/// must not vouch a subsequent `arr[0]` read safe.
const LENGTH_EQUALITY_MATCHERS: [&str; 3] = ["toBe", "toEqual", "toStrictEqual"];

/// `expect(<arr>.length).<matcher>(N)` matchers that assert a lower bound on the
/// length, paired with the smallest `N` that still proves `length >= 1`:
///   - `toBeGreaterThan(N)` means `length > N`, non-empty for `N >= 0`.
///   - `toBeGreaterThanOrEqual(N)` means `length >= N`, non-empty for `N >= 1`.
const LENGTH_LOWER_BOUND_MATCHERS: [(&str, u32); 2] =
    [("toBeGreaterThan", 0), ("toBeGreaterThanOrEqual", 1)];

/// Scans `stmts` for the statement containing `node_span_start`, then checks
/// all preceding siblings for one of these guard patterns:
///   1. `if (...length...) { return/throw/process.exit }` (early-exit guard)
///   2. `expect(<obj_text>).toHaveLength(N)` (Vitest/Jest assertion guard)
///   3. `expect(<obj_text>.length).<matcher>(N)` with `N` proving `length >= 1`
///      (see [`length_expect_proves_nonempty`])
///   4. chai length assertion on the same array (see [`stmt_has_chai_length_assertion`])
///   5. Node/Deno throwing assertion proving non-emptiness on the same array
///      (see [`stmt_is_assert_nonempty_length`])
fn scan_preceding_stmts(
    stmts: &[Statement],
    node_span_start: u32,
    obj_text: &str,
    source: &str,
) -> bool {
    let our_idx = stmts
        .iter()
        .position(|s| s.span().start <= node_span_start && node_span_start < s.span().end);
    let Some(our_idx) = our_idx else { return false };

    let have_length_needle = format!("expect({obj_text}).toHaveLength(");
    let length_expect_prefix = format!("expect({obj_text}.length).");
    for stmt in &stmts[..our_idx] {
        if let Statement::IfStatement(if_stmt) = stmt {
            let cond_start = if_stmt.test.span().start as usize;
            let cond_end = if_stmt.test.span().end as usize;
            let cond_text = &source[cond_start..cond_end];
            if cond_text.contains(".length")
                && (body_has_early_exit(&if_stmt.consequent)
                    || if_stmt.alternate.as_ref().map_or(false, body_has_early_exit))
            {
                return true;
            }
        }
        let stmt_span = stmt.span();
        let stmt_text = &source[stmt_span.start as usize..stmt_span.end as usize];
        if stmt_text.contains(have_length_needle.as_str()) {
            return true;
        }
        if let Some(after_prefix) = find_after(stmt_text, &length_expect_prefix) {
            if length_expect_proves_nonempty(after_prefix) {
                return true;
            }
        }
        if stmt_has_chai_length_assertion(stmt_text, obj_text) {
            return true;
        }
        if stmt_is_assert_nonempty_length(stmt, obj_text, source) {
            return true;
        }
    }
    false
}

/// Given `after_prefix` — the text immediately following `expect(<arr>.length).`
/// — returns true when it is a matcher call that proves `length >= 1`. The
/// matcher's leading integer argument is checked against the threshold for its
/// family: an equality matcher ([`LENGTH_EQUALITY_MATCHERS`]) needs `N >= 1`, a
/// lower-bound matcher ([`LENGTH_LOWER_BOUND_MATCHERS`]) needs `N >= its_min`.
/// A non-integer or absent argument (`toEqual(expected)`, `toBeGreaterThan(x)`)
/// can't be proven non-empty, so it does not qualify.
fn length_expect_proves_nonempty(after_prefix: &str) -> bool {
    for matcher in LENGTH_EQUALITY_MATCHERS {
        if let Some(arg) = matcher_int_arg(after_prefix, matcher) {
            return arg >= 1;
        }
    }
    for (matcher, min) in LENGTH_LOWER_BOUND_MATCHERS {
        if let Some(arg) = matcher_int_arg(after_prefix, matcher) {
            return arg >= min;
        }
    }
    false
}

/// When `after_prefix` is `<matcher>(<int>...)`, returns the leading unsigned
/// integer argument. Returns `None` when the matcher name doesn't match or the
/// argument is not an integer literal (so a non-literal expression argument
/// stays unproven rather than silently treated as zero).
fn matcher_int_arg(after_prefix: &str, matcher: &str) -> Option<u32> {
    let call = format!("{matcher}(");
    let rest = after_prefix.strip_prefix(&call)?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse::<u32>().ok()
}

/// Throwing-assertion callees that take a single boolean condition argument:
/// Node's `assert(cond)` and `assert.ok(cond)`. Both throw an `AssertionError`
/// unless the condition is truthy, so a length comparison passed to them
/// establishes the array length the same way an `if`-guard does.
fn is_assert_condition_callee(callee: &Expression) -> bool {
    match callee {
        Expression::Identifier(id) => id.name.as_str() == "assert",
        Expression::StaticMemberExpression(member) => {
            member.property.name.as_str() == "ok"
                && matches!(&member.object, Expression::Identifier(id) if id.name.as_str() == "assert")
        }
        _ => false,
    }
}

/// Throwing-assertion callees that compare two values for equality:
/// `assert.equal(a, b)` and `assert.strictEqual(a, b)`. They throw unless the
/// two arguments are equal, so `assert.equal(arr.length, N)` with `N >= 1`
/// proves the array is non-empty.
fn is_assert_equal_callee(callee: &Expression) -> bool {
    matches!(callee, Expression::StaticMemberExpression(member)
        if matches!(member.property.name.as_str(), "equal" | "strictEqual")
            && matches!(&member.object, Expression::Identifier(id) if id.name.as_str() == "assert"))
}

/// Returns true when `stmt` is a throwing assertion that proves `<obj_text>` is
/// non-empty (`length >= 1`), making a subsequent first/last read in-bounds.
/// Recognized forms:
///   - `assert(<obj>.length <cmp> N)` / `assert.ok(<obj>.length <cmp> N)` — the
///     condition argument is a length comparison that bounds the length away
///     from 0 (see [`length_comparison_proves_nonempty`]).
///   - `assert.equal(<obj>.length, N)` / `assert.strictEqual(<obj>.length, N)`
///     with `N >= 1`.
///
/// Scoped to the SAME receiver array; an assertion on a different array, a
/// non-length condition, or one that proves `length === 0` does not qualify.
fn stmt_is_assert_nonempty_length(stmt: &Statement, obj_text: &str, source: &str) -> bool {
    let Statement::ExpressionStatement(expr_stmt) = stmt else {
        return false;
    };
    let Expression::CallExpression(call) = &expr_stmt.expression else {
        return false;
    };
    if is_assert_condition_callee(&call.callee) {
        let Some(first_arg) = call.arguments.first().and_then(|a| a.as_expression()) else {
            return false;
        };
        return length_comparison_proves_nonempty(first_arg, obj_text, source);
    }
    if is_assert_equal_callee(&call.callee) {
        let (Some(actual), Some(expected)) = (
            call.arguments.first().and_then(|a| a.as_expression()),
            call.arguments.get(1).and_then(|a| a.as_expression()),
        ) else {
            return false;
        };
        return is_length_of(actual, obj_text, source)
            && is_positive_numeric_literal(expected, source);
    }
    false
}

/// Returns true when `expr` is a comparison that proves `<obj_text>.length >= 1`.
/// The `.length` member must be on the SAME receiver array. Recognized (with the
/// `.length` side on either operand):
///   - `length === N` / `length == N` with `N >= 1`
///   - `length !== 0` / `length != 0`
///   - `length >= N` with `N >= 1`
///   - `length > N` with `N >= 0`
///
/// `length === 0` (or any bound that admits 0) does NOT qualify — it proves the
/// array may be empty, so the first/last read stays flagged.
fn length_comparison_proves_nonempty(expr: &Expression, obj_text: &str, source: &str) -> bool {
    let Expression::BinaryExpression(bin) = expr else {
        return false;
    };
    let left_is_len = is_length_of(&bin.left, obj_text, source);
    let right_is_len = is_length_of(&bin.right, obj_text, source);
    if !left_is_len && !right_is_len {
        return false;
    }
    // Normalize so `value` is the literal compared against `<obj>.length`, and
    // `op` reads as `length <op> value`.
    let (value_expr, op) = if left_is_len {
        (&bin.right, bin.operator)
    } else {
        (&bin.left, flip_comparison(bin.operator))
    };
    let Expression::NumericLiteral(lit) = value_expr else {
        return false;
    };
    let Ok(n) = source[lit.span.start as usize..lit.span.end as usize].parse::<u32>() else {
        return false;
    };
    match op {
        BinaryOperator::StrictEquality | BinaryOperator::Equality => n >= 1,
        // `length !== 0` / `length != 0` proves non-empty; a `!= N` for any other
        // `N` does not (length could still be 0).
        BinaryOperator::StrictInequality | BinaryOperator::Inequality => n == 0,
        BinaryOperator::GreaterEqualThan => n >= 1,
        BinaryOperator::GreaterThan => true, // length > 0 (or any N) proves >= 1
        _ => false,
    }
}

/// Mirrors a comparison operator across its operands so `N <op> length` can be
/// read as `length <flipped> N`. Only the comparisons used by
/// [`length_comparison_proves_nonempty`] are mapped; others pass through and are
/// rejected by the caller.
fn flip_comparison(op: BinaryOperator) -> BinaryOperator {
    match op {
        BinaryOperator::LessThan => BinaryOperator::GreaterThan,
        BinaryOperator::LessEqualThan => BinaryOperator::GreaterEqualThan,
        other => other,
    }
}

/// Returns true when `stmt_text` is a chai BDD length assertion on `obj_text`
/// that proves the array is non-empty — making a subsequent `obj_text[0]` /
/// `obj_text[obj_text.length - 1]` read in-bounds. Recognized forms:
///   - `<obj>.length.should.<...>` — the `should` chain hung off `.length`
///     (e.g. `.should.be.equal(N)`, `.should.be.greaterThan(0)`,
///     `.should.be.at.least(1)`).
///   - `<obj>.should.have.length(` / `<obj>.should.have.lengthOf(` — the
///     alternative chai syntax that asserts the array's length directly.
///
/// Scoped to a length assertion on the SAME receiver array: a bare `.should`
/// on `obj_text` (not on its `.length`, and not a `.have.length` assertion)
/// does not vouch the read safe.
fn stmt_has_chai_length_assertion(stmt_text: &str, obj_text: &str) -> bool {
    stmt_text.contains(&format!("{obj_text}.length.should."))
        || stmt_text.contains(&format!("{obj_text}.should.have.length("))
        || stmt_text.contains(&format!("{obj_text}.should.have.lengthOf("))
}

/// Returns the substring of `haystack` immediately following the first
/// occurrence of `needle`, or `None` if `needle` is absent.
fn find_after<'a>(haystack: &'a str, needle: &str) -> Option<&'a str> {
    haystack
        .find(needle)
        .map(|idx| &haystack[idx + needle.len()..])
}

/// Returns true when a preceding sibling statement in the same block guards
/// the array access via an early-exit pattern or a Vitest/Jest length assertion.
/// Does not cross function boundaries.
fn has_preceding_guard(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    obj_text: &str,
    source: &str,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = node.id();
    let node_span_start = node.kind().span().start;

    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return false;
        }
        let parent = nodes.get_node(parent_id);
        match parent.kind() {
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => return false,
            AstKind::BlockStatement(block) => {
                return scan_preceding_stmts(&block.body, node_span_start, obj_text, source);
            }
            AstKind::FunctionBody(body) => {
                return scan_preceding_stmts(
                    &body.statements,
                    node_span_start,
                    obj_text,
                    source,
                );
            }
            AstKind::Program(prog) => {
                return scan_preceding_stmts(&prog.body, node_span_start, obj_text, source);
            }
            _ => {}
        }
        current_id = parent_id;
    }
}

/// Returns true when an unconditional `<obj_text>.push(...)` statement precedes
/// the access in its scope or in any enclosing scope. Walks ancestors
/// outward: at each block/function/program scope, anchors on the statement that
/// contains the access (or the path down to it) and scans its preceding siblings
/// for a `push` on the same binding. Only direct sibling expression statements
/// count, so a `push` nested inside an `if`/loop — which may not run — does not
/// vouch the access safe. A push in an outer scope is honored because it always
/// executes before any nested callback defined after it.
fn has_preceding_push(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    obj_text: &str,
    source: &str,
) -> bool {
    let nodes = semantic.nodes();
    let node_span_start = node.kind().span().start;
    for ancestor in nodes.ancestors(node.id()) {
        let stmts: &[Statement] = match ancestor.kind() {
            AstKind::Program(prog) => &prog.body,
            AstKind::FunctionBody(body) => &body.statements,
            AstKind::BlockStatement(block) => &block.body,
            _ => continue,
        };
        if scan_preceding_pushes(stmts, node_span_start, obj_text, source) {
            return true;
        }
    }
    false
}

/// Anchors on the statement in `stmts` containing `node_span_start`, then returns
/// true if any preceding sibling is an unconditional `<obj_text>.push(...)`.
fn scan_preceding_pushes(
    stmts: &[Statement],
    node_span_start: u32,
    obj_text: &str,
    source: &str,
) -> bool {
    let Some(our_idx) = stmts
        .iter()
        .position(|s| s.span().start <= node_span_start && node_span_start < s.span().end)
    else {
        return false;
    };
    stmts[..our_idx]
        .iter()
        .any(|stmt| stmt_is_push_on(stmt, obj_text, source))
}

/// Returns true when `stmt` is an expression statement `<obj_text>.push(...)`.
fn stmt_is_push_on(stmt: &Statement, obj_text: &str, source: &str) -> bool {
    let Statement::ExpressionStatement(expr_stmt) = stmt else {
        return false;
    };
    let Expression::CallExpression(call) = &expr_stmt.expression else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    member.property.name.as_str() == "push" && expr_text(&member.object, source) == obj_text
}

/// Returns true when `name` resolves to a same-scope `const` whose initializer
/// is a non-empty array literal — making `name[0]` provably in-bounds. Walks
/// ancestors innermost-first, so the closest binding wins (a shadowing inner
/// `const` is honored over an outer one). Runtime-transparent wrappers around
/// the initializer (`[''] as T`, `<T>['x']`, `[...] satisfies T`, `[...]!`,
/// parentheses) are peeled first, so an asserted literal still qualifies. Only
/// an array literal qualifies: a call initializer (`getColors()`) or a `let` is
/// unknown and stays flagged. A spread element makes the length non-static, so
/// an array literal containing one does not qualify either.
fn resolves_to_nonempty_array_literal(
    node: &oxc_semantic::AstNode,
    name: &str,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(node.id()) {
        let stmts: &[Statement] = match ancestor.kind() {
            AstKind::Program(prog) => &prog.body,
            AstKind::FunctionBody(body) => &body.statements,
            AstKind::BlockStatement(block) => &block.body,
            _ => continue,
        };
        for stmt in stmts {
            let Statement::VariableDeclaration(decl) = stmt else {
                continue;
            };
            if decl.kind != VariableDeclarationKind::Const {
                continue;
            }
            for declarator in &decl.declarations {
                let BindingPattern::BindingIdentifier(id) = &declarator.id else {
                    continue;
                };
                if id.name.as_str() != name {
                    continue;
                }
                // Closest binding wins: the first declarator matching `name`
                // decides, even if its initializer is not a qualifying literal.
                // Peel transparent wrappers so `['x'] as T` / `<T>['x']` /
                // `['x'] satisfies T` / `['x']!` / `(['x'])` reach the literal.
                let init = declarator.init.as_ref().map(peel_transparent_wrappers);
                return matches!(
                    init,
                    Some(Expression::ArrayExpression(arr)) if is_static_nonempty_array(arr)
                );
            }
        }
    }
    false
}

/// Returns true when the array literal has at least one statically-present
/// element and no spread (a spread's length is unknown, so it disqualifies).
fn is_static_nonempty_array(arr: &ArrayExpression) -> bool {
    if arr.elements.is_empty() {
        return false;
    }
    !arr.elements
        .iter()
        .any(|el| matches!(el, ArrayExpressionElement::SpreadElement(_)))
}

/// Returns true when `name` resolves to a same-scope `const` whose initializer is
/// an object literal with at least one property and EVERY property value a
/// statically non-empty array literal — making `name[key][0]` provably in-bounds
/// for any key the object actually holds. Mirrors
/// [`resolves_to_nonempty_array_literal`]: walks ancestor scopes innermost-first so
/// the closest binding wins, and only a direct `const` qualifies (a `let` may be
/// reassigned). The proof is purely structural — whichever value `name[key]`
/// selects is one of the literal's array values, each of known length `>= 1`, so
/// the first-element read can never hit an empty array. A spread property, a
/// getter/setter, a non-array value, an empty array, an array containing a spread,
/// or an empty object disqualifies: any of those leaves a value whose length is not
/// provably `>= 1`.
fn resolves_to_const_object_with_nonempty_arrays(
    node: &oxc_semantic::AstNode,
    name: &str,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(node.id()) {
        let stmts: &[Statement] = match ancestor.kind() {
            AstKind::Program(prog) => &prog.body,
            AstKind::FunctionBody(body) => &body.statements,
            AstKind::BlockStatement(block) => &block.body,
            _ => continue,
        };
        for stmt in stmts {
            let Statement::VariableDeclaration(decl) = stmt else {
                continue;
            };
            if decl.kind != VariableDeclarationKind::Const {
                continue;
            }
            for declarator in &decl.declarations {
                let BindingPattern::BindingIdentifier(id) = &declarator.id else {
                    continue;
                };
                if id.name.as_str() != name {
                    continue;
                }
                // Closest binding wins: the first declarator matching `name`
                // decides, even if its initializer is not a qualifying literal.
                return matches!(
                    &declarator.init,
                    Some(Expression::ObjectExpression(obj))
                        if object_values_all_nonempty_arrays(obj)
                );
            }
        }
    }
    false
}

/// Returns true when `obj` has at least one property and every property is a plain
/// (non-spread) entry whose value is a statically non-empty array literal. A spread
/// property (`{ ...rest }`) or a getter/setter — whose value is not an array
/// literal — fails the match, leaving a value of unknown length, so the object does
/// not qualify.
fn object_values_all_nonempty_arrays(obj: &ObjectExpression) -> bool {
    if obj.properties.is_empty() {
        return false;
    }
    obj.properties.iter().all(|prop| {
        matches!(prop, ObjectPropertyKind::ObjectProperty(p)
            if matches!(&p.value, Expression::ArrayExpression(arr) if is_static_nonempty_array(arr)))
    })
}

/// The fixed-size array constructors whose first argument fixes the length:
/// the TypedArray family plus `Array`. `new <ctor>(N)` allocates exactly `N`
/// slots, and `new <ctor>([...])` builds one slot per element.
const FIXED_SIZE_ARRAY_CTORS: [&str; 12] = [
    "Int8Array",
    "Uint8Array",
    "Uint8ClampedArray",
    "Int16Array",
    "Uint16Array",
    "Int32Array",
    "Uint32Array",
    "Float32Array",
    "Float64Array",
    "BigInt64Array",
    "BigUint64Array",
    "Array",
];

/// Returns true when `name` resolves to a same-scope `const` whose initializer
/// is a fixed-size array construction with a statically-known length `>= 1` —
/// making `name[0]` provably in-bounds. Mirrors
/// [`resolves_to_nonempty_array_literal`]: walks ancestor scopes innermost-first
/// so the closest binding wins, and only a direct `const` qualifies (a `let` may
/// be reassigned to a shorter array). A call initializer or non-qualifying
/// `new` expression stays flagged.
fn resolves_to_nonempty_fixed_array_construction(
    node: &oxc_semantic::AstNode,
    name: &str,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(node.id()) {
        let stmts: &[Statement] = match ancestor.kind() {
            AstKind::Program(prog) => &prog.body,
            AstKind::FunctionBody(body) => &body.statements,
            AstKind::BlockStatement(block) => &block.body,
            _ => continue,
        };
        for stmt in stmts {
            let Statement::VariableDeclaration(decl) = stmt else {
                continue;
            };
            if decl.kind != VariableDeclarationKind::Const {
                continue;
            }
            for declarator in &decl.declarations {
                let BindingPattern::BindingIdentifier(id) = &declarator.id else {
                    continue;
                };
                if id.name.as_str() != name {
                    continue;
                }
                // Closest binding wins: the first declarator matching `name`
                // decides, even if its initializer is not a qualifying `new`.
                return matches!(
                    &declarator.init,
                    Some(Expression::NewExpression(new_expr))
                        if is_nonempty_fixed_array_construction(new_expr)
                );
            }
        }
    }
    false
}

/// Returns true when `new_expr` constructs a fixed-size array (a TypedArray or
/// `Array`) of statically-known length `>= 1`: either `new <ctor>(N)` with a
/// numeric-literal `N >= 1`, or `new <ctor>([...])` with a non-empty static
/// element-list literal. A dynamic length (`new Uint32Array(n)`) or a spread in
/// the element list leaves the length unknown, so it does not qualify.
fn is_nonempty_fixed_array_construction(new_expr: &NewExpression) -> bool {
    let Expression::Identifier(callee) = &new_expr.callee else {
        return false;
    };
    if !FIXED_SIZE_ARRAY_CTORS.contains(&callee.name.as_str()) {
        return false;
    }
    let Some(first_arg) = new_expr.arguments.first().and_then(|a| a.as_expression()) else {
        return false;
    };
    match first_arg {
        Expression::NumericLiteral(lit) => lit.value >= 1.0 && lit.value.fract() == 0.0,
        Expression::ArrayExpression(arr) => is_static_nonempty_array(arr),
        _ => false,
    }
}

/// Returns true when `ident`'s binding has a type denoting a non-empty tuple —
/// making `ident[0]` provably in-bounds. A directly-annotated binding reads the
/// `type_annotation` on the enclosing `FormalParameter` (`p: [A, B]`) or
/// `VariableDeclarator` (`const p: [A, B]`). A binding destructured from a typed
/// object pattern reads the matching MEMBER's type instead — from an inline type
/// literal (`{ nouns }: { nouns?: [string, string] }`) or a same-file
/// `interface`/`type` member (`{ nouns }: PaginationControlProps<T>` with
/// `interface PaginationControlProps<T> { nouns?: [string, string] }`), resolved by
/// the destructuring KEY so a renamed prop (`{ nouns: n }`) is handled; a generic
/// receiver is resolved by name since the member's tuple type is independent of its
/// type arguments (see [`binding_declared_ts_type`]). The resolved type qualifies
/// when it is a literal tuple (`[A, B]`, with a `readonly [...]` wrapper unwrapped)
/// or a bare named alias that resolves to one (`type Semver = [number, number,
/// number]`; see [`ts_type_resolves_to_nonempty_tuple`]). A generic reference
/// standing alone (`LineSegment<T>`) stays unresolved — its tuple shape needs type
/// information this native backend doesn't have.
fn resolves_to_nonempty_tuple_type<'a>(
    ident: &IdentifierReference,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    if binding_declared_ts_type(ident, semantic)
        .is_some_and(|ty| ts_type_resolves_to_nonempty_tuple(ty, semantic))
    {
        return true;
    }
    let Some(ref_id) = ident.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let nodes = semantic.nodes();
    let decl_node_id = scoping.symbol_declaration(sym_id);
    for kind in std::iter::once(nodes.kind(decl_node_id))
        .chain(nodes.ancestor_kinds(decl_node_id))
    {
        let annotation = match kind {
            AstKind::FormalParameter(param) => &param.type_annotation,
            AstKind::VariableDeclarator(decl) => &decl.type_annotation,
            // Leaving the binding's own declaration without finding an annotation.
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) | AstKind::Program(_) => {
                return false;
            }
            _ => continue,
        };
        return annotation
            .as_ref()
            .is_some_and(|ann| ts_type_resolves_to_nonempty_tuple(&ann.type_annotation, semantic));
    }
    false
}

/// Returns true when the indexed receiver is a member access `obj.field` whose
/// `field` is declared as a non-empty tuple on `obj`'s type — making
/// `obj.field[0]` provably in-bounds. `obj` must be a simple identifier whose
/// declared type resolves (same file) to an `interface`/`type`/inline type
/// literal carrying a `field: [A, B]` member. The member-access counterpart of
/// [`resolves_to_nonempty_tuple_type`], which only covers an identifier receiver;
/// the member's tuple type is read via [`ts_type_member_type`] and validated with
/// [`ts_type_resolves_to_nonempty_tuple`] so index-vs-arity behavior matches the
/// identifier case. A plain-array member (`field?: string[]`), an empty tuple
/// (`field: []`), an absent member, or a cross-file/unresolved container type all
/// stay flagged.
fn member_access_receiver_is_nonempty_tuple_member<'a>(
    object: &Expression<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let Expression::StaticMemberExpression(access) = object else {
        return false;
    };
    let Expression::Identifier(obj_ident) = &access.object else {
        return false;
    };
    let Some(container) = binding_declared_ts_type(obj_ident, semantic) else {
        return false;
    };
    ts_type_member_type(container, access.property.name.as_str(), semantic)
        .is_some_and(|ty| ts_type_resolves_to_nonempty_tuple(ty, semantic))
}

/// Returns true when `ident` is an UNANNOTATED `const` binding whose initializer
/// is a call to a same-file function with a non-empty tuple return-type
/// annotation — so the binding is provably a non-empty tuple and `ident[0]` is
/// in-bounds. The initializer is peeled of transparent wrappers to reach the
/// `CallExpression`; the callee is resolved to its declaration via the symbol
/// table (same file only) and its explicit return type read from a `function`
/// declaration or a `const`-bound arrow/function expression. A `[A, B] | undefined`
/// return type qualifies when every non-nullish member is a non-empty tuple; a
/// possibly-`undefined` receiver is a nullish-access concern outside this rule's
/// empty-array scope. An imported,
/// unresolved, array-returning, or unannotated callee stays flagged. An ANNOTATED
/// binding is left to [`resolves_to_nonempty_tuple_type`]: an explicit annotation
/// overrides the call's inferred type, so its own type drives the decision.
fn resolves_to_nonempty_tuple_from_call_return<'a>(
    ident: &IdentifierReference,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    if binding_declared_type(ident, semantic).is_some() {
        return false;
    }
    let Some(init) = binding_const_init(ident, semantic) else {
        return false;
    };
    let Expression::CallExpression(call) = peel_transparent_wrappers(init) else {
        return false;
    };
    let Expression::Identifier(callee) = &call.callee else {
        return false;
    };
    callee_return_type_annotation(callee, semantic).is_some_and(|ann| {
        ts_return_type_resolves_to_nonempty_tuple(&ann.type_annotation, semantic)
    })
}

/// Resolves the callee `IdentifierReference` of a call to its declaration in the
/// same file via the symbol table and returns that function's explicit
/// return-type annotation. The declaration is either a `function` declaration
/// (`function f(): [A, B]`) or a `const`-bound arrow/function expression
/// (`const f = (): [A, B] => …`). Returns `None` when the callee does not resolve
/// to a same-file function with an explicit return type (an imported callee has no
/// in-file declaration; a parameter or an unannotated function has no return type).
fn callee_return_type_annotation<'a>(
    callee: &IdentifierReference,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<&'a TSTypeAnnotation<'a>> {
    let ref_id = callee.reference_id.get()?;
    let scoping = semantic.scoping();
    let sym_id = scoping.get_reference(ref_id).symbol_id()?;
    let nodes = semantic.nodes();
    let decl_node_id = scoping.symbol_declaration(sym_id);
    for kind in std::iter::once(nodes.kind(decl_node_id))
        .chain(nodes.ancestor_kinds(decl_node_id))
    {
        match kind {
            AstKind::Function(func) => return func.return_type.as_deref(),
            AstKind::VariableDeclarator(decl) => {
                return match decl.init.as_ref().map(peel_transparent_wrappers) {
                    Some(Expression::ArrowFunctionExpression(arrow)) => {
                        arrow.return_type.as_deref()
                    }
                    Some(Expression::FunctionExpression(func)) => func.return_type.as_deref(),
                    _ => None,
                };
            }
            AstKind::FormalParameter(_) | AstKind::Program(_) => return None,
            _ => continue,
        }
    }
    None
}

/// Returns true when a function's return type `ty` proves a non-empty tuple
/// result: either `ty` itself resolves to a non-empty tuple, or `ty` is a union
/// every one of whose NON-NULLISH members does (`[A, B] | undefined`). A
/// union with a non-tuple non-nullish member, or with no non-nullish member at
/// all, does not qualify. Reuses [`ts_type_resolves_to_nonempty_tuple`] so the
/// index-vs-arity behavior matches the annotated-tuple case.
fn ts_return_type_resolves_to_nonempty_tuple<'a>(
    ty: &'a TSType<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let TSType::TSUnionType(union) = ty else {
        return ts_type_resolves_to_nonempty_tuple(ty, semantic);
    };
    let mut has_non_nullish = false;
    for member in &union.types {
        if is_nullish_keyword_type(member) {
            continue;
        }
        has_non_nullish = true;
        if !ts_type_resolves_to_nonempty_tuple(member, semantic) {
            return false;
        }
    }
    has_non_nullish
}

/// Returns true when `ty` is the `undefined` or `null` keyword type — a nullish
/// union member ignored when deciding whether the non-nullish members are tuples.
fn is_nullish_keyword_type(ty: &TSType) -> bool {
    matches!(ty, TSType::TSUndefinedKeyword(_) | TSType::TSNullKeyword(_))
}

/// Returns true when `ty` is a literal tuple type with at least one element,
/// unwrapping a leading `readonly` operator (`readonly [A, B]`). An empty tuple
/// `[]` has no element at index 0, so it does not qualify.
fn ts_type_is_nonempty_tuple(ty: &TSType) -> bool {
    match ty {
        TSType::TSTupleType(tuple) => !tuple.element_types.is_empty(),
        TSType::TSTypeOperatorType(op)
            if op.operator == TSTypeOperatorOperator::Readonly =>
        {
            ts_type_is_nonempty_tuple(&op.type_annotation)
        }
        _ => false,
    }
}

/// Maximum type-alias hops to follow when resolving a named tuple alias. Bounds
/// a long alias chain (`type A = B; type B = C; ...`); combined with the
/// visited-name set in [`ts_type_resolves_to_nonempty_tuple`], it also stops a
/// self-referential alias from looping forever.
const MAX_TUPLE_ALIAS_HOPS: usize = 8;

/// Returns true when `ty` denotes a non-empty tuple: either directly (a literal
/// `[A, B]`, `readonly` unwrapped — see [`ts_type_is_nonempty_tuple`]) or by
/// resolving a bare named alias to its declared type
/// (`type Semver = [number, number, number]`). Only a bare `TSTypeReference` with
/// no type arguments is followed; a generic reference (`LineSegment<T>`) needs
/// type information this native backend lacks and stays unresolved. Alias chains
/// (`type A = B; type B = [..]`) are followed up to [`MAX_TUPLE_ALIAS_HOPS`] hops,
/// tracking visited names so a self-referential alias cannot loop forever. An
/// alias whose declaration is not in the file stays unresolved (returns false),
/// so the read is still flagged.
fn ts_type_resolves_to_nonempty_tuple<'a>(
    ty: &'a TSType<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    if ts_type_is_nonempty_tuple(ty) {
        return true;
    }
    let Some(name) = bare_type_reference_name(ty) else {
        return false;
    };
    let mut current = name;
    let mut visited: Vec<&str> = Vec::new();
    for _ in 0..MAX_TUPLE_ALIAS_HOPS {
        if visited.contains(&current) {
            return false;
        }
        visited.push(current);
        let Some(declared) = find_type_alias_declared_type(current, semantic) else {
            return false;
        };
        if ts_type_is_nonempty_tuple(declared) {
            return true;
        }
        let Some(next) = bare_type_reference_name(declared) else {
            return false;
        };
        current = next;
    }
    false
}

/// If `ty` is a bare `TSTypeReference` (a plain type name with NO type
/// arguments), return the referenced identifier name; otherwise `None`. A
/// generic reference (`Foo<T>`) or a qualified name (`A.B`) returns `None`.
fn bare_type_reference_name<'a>(ty: &'a TSType<'a>) -> Option<&'a str> {
    let TSType::TSTypeReference(reference) = ty else {
        return None;
    };
    if reference.type_arguments.is_some() {
        return None;
    }
    let TSTypeName::IdentifierReference(name) = &reference.type_name else {
        return None;
    };
    Some(name.name.as_str())
}

/// Returns the declared type of the file's `type <name> = ...` alias, or `None`
/// if no such alias is declared. Mirrors the `semantic.nodes()` scan that
/// `collect_keyof_typeof_aliases` uses in the `ts-no-enum-object-literal-pattern`
/// rule.
fn find_type_alias_declared_type<'a>(
    name: &str,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<&'a TSType<'a>> {
    for node in semantic.nodes().iter() {
        if let AstKind::TSTypeAliasDeclaration(decl) = node.kind()
            && decl.id.name.as_str() == name
        {
            return Some(&decl.type_annotation);
        }
    }
    None
}

/// Array iteration methods whose callback receives an array ELEMENT as its (first)
/// parameter. When the receiver array's element type is a non-empty tuple, the
/// unannotated parameter is that tuple, so its `[0]` is in-bounds. `reduce` is
/// excluded: its first callback parameter is the accumulator, not an element.
const ELEMENT_CALLBACK_METHODS: [&str; 5] = ["sort", "map", "forEach", "filter", "find"];

/// Array methods that return an array with the SAME element type as their
/// receiver, so element-type derivation can see through them onto the receiver.
const ELEMENT_PRESERVING_METHODS: [&str; 4] = ["sort", "filter", "slice", "reverse"];

/// Returns true when `ident` is an UNANNOTATED callback parameter of an
/// element-typed iteration method ([`ELEMENT_CALLBACK_METHODS`]) whose receiver
/// array has a non-empty tuple element type — so the parameter is bound to that
/// tuple and `ident[0]` is in-bounds. Walks the parameter's binding to its
/// enclosing function, then to the parent `CallExpression`, confirms the callee is
/// `<recv>.<method>`, and derives `<recv>`'s element type via
/// [`array_element_type_is_nonempty_tuple`]. An ANNOTATED parameter is already
/// covered by [`resolves_to_nonempty_tuple_type`], so it is skipped here.
fn sort_callback_param_tuple_element<'a>(
    ident: &IdentifierReference,
    semantic: &oxc_semantic::Semantic<'a>,
) -> bool {
    let Some(ref_id) = ident.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let nodes = semantic.nodes();
    let decl_node_id = scoping.symbol_declaration(sym_id);

    // The binding must be a formal parameter WITHOUT a type annotation.
    let mut is_unannotated_param = false;
    for kind in std::iter::once(nodes.kind(decl_node_id))
        .chain(nodes.ancestor_kinds(decl_node_id))
    {
        match kind {
            AstKind::FormalParameter(param) => {
                if param.type_annotation.is_some() {
                    return false;
                }
                is_unannotated_param = true;
                break;
            }
            // The binding is not a parameter — out of scope for this case.
            AstKind::VariableDeclarator(_)
            | AstKind::Function(_)
            | AstKind::ArrowFunctionExpression(_)
            | AstKind::Program(_) => return false,
            _ => continue,
        }
    }
    if !is_unannotated_param {
        return false;
    }

    // The function owning the parameter, then its parent call. The callee must be
    // `<recv>.<method>` for an element-typed iteration method whose receiver has a
    // non-empty tuple element type.
    let mut ancestors = nodes.ancestors(decl_node_id);
    let Some(fn_node) = ancestors.by_ref().find(|n| {
        matches!(
            n.kind(),
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
        )
    }) else {
        return false;
    };
    let AstKind::CallExpression(call) = nodes.parent_kind(fn_node.id()) else {
        return false;
    };
    let Expression::StaticMemberExpression(callee) = &call.callee else {
        return false;
    };
    if !ELEMENT_CALLBACK_METHODS.contains(&callee.property.name.as_str()) {
        return false;
    }
    array_element_type_is_nonempty_tuple(&callee.object, semantic)
}

/// Returns true when `name` is the element binding of an enclosing
/// `for (const name of <iterable>)` whose iterable array has a non-empty tuple
/// element type. Mirrors [`is_matchall_for_of_element`]: walks ancestors
/// innermost-first so the closest binding `for...of` wins. A `for...of` binding
/// never carries a type annotation, so the element type is derived syntactically
/// from the iterable via [`array_element_type_is_nonempty_tuple`].
fn for_of_tuple_element<'a>(
    node: &oxc_semantic::AstNode<'a>,
    name: &str,
    semantic: &oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(node.id()) {
        let AstKind::ForOfStatement(for_of) = ancestor.kind() else {
            continue;
        };
        if !for_of_binds_name(&for_of.left, name) {
            continue;
        }
        return array_element_type_is_nonempty_tuple(&for_of.right, semantic);
    }
    false
}

/// Returns true when `name` is the first parameter of the callback passed as the
/// second argument to a Vue `watch([e0, …, eN-1], cb)` call whose SOURCE (first
/// argument) is a non-empty array literal. Vue types that parameter as a
/// fixed-length tuple matching the array-literal sources, so its index 0 is always
/// in-bounds — the array can never be empty. Purely structural: keyed on the
/// `watch` callee, an array-literal first argument, and the enclosing callback
/// binding `name` as its first parameter — no name/value allowlist. A
/// non-array-literal source (`watch(singleRef, cb)`) or an empty-array source
/// (`watch([], cb)`) is not a fixed-length tuple and returns false, so the access
/// stays flagged. Walks ancestors innermost-first so the closest enclosing callback
/// decides. Mirrors [`is_entries_callback_param`] / [`is_then_callback_param`].
fn is_watch_array_literal_source_callback_param(
    node: &oxc_semantic::AstNode,
    name: &str,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(node.id()) {
        let params = match ancestor.kind() {
            AstKind::ArrowFunctionExpression(arrow) => &arrow.params,
            AstKind::Function(func) => &func.params,
            _ => continue,
        };
        // `name` must be this callback's first parameter — the tuple slot Vue binds
        // to the watched sources. If the closest enclosing callback binds `name`
        // elsewhere (or not at all), it is not the watch source binder.
        if !first_param_is_name(params, name) {
            return false;
        }
        let AstKind::CallExpression(call) = nodes.parent_kind(ancestor.id()) else {
            return false;
        };
        // Bare global `watch(<sources>, <this callback>)`.
        if !matches!(&call.callee, Expression::Identifier(id) if id.name.as_str() == "watch") {
            return false;
        }
        // The first argument must be a non-empty array-literal source, and THIS
        // callback must be the second argument.
        let Some(Expression::ArrayExpression(sources)) =
            call.arguments.first().and_then(|arg| arg.as_expression())
        else {
            return false;
        };
        let callback_is_second_arg = call
            .arguments
            .get(1)
            .and_then(|arg| arg.as_expression())
            .is_some_and(|cb| cb.span() == ancestor.kind().span());
        return callback_is_second_arg && is_static_nonempty_array(sources);
    }
    false
}

/// Returns true when the array that `expr` evaluates to has a non-empty tuple
/// element type, derived purely syntactically. Conservative: any hop that cannot
/// be resolved from declared types returns false, so the access keeps flagging.
fn array_element_type_is_nonempty_tuple<'a>(
    expr: &'a Expression<'a>,
    semantic: &oxc_semantic::Semantic<'a>,
) -> bool {
    expr_declared_type(expr, semantic)
        .and_then(array_type_element)
        .is_some_and(ts_type_is_nonempty_tuple)
}

/// Derives the declared TYPE of expression `expr`, chaining through: `x!` / `(x)`
/// unwrapping; an `x as T` cast (yielding `T`); element-preserving array methods
/// (`.sort`/`.filter`/`.slice`/`.reverse`) onto their receiver (same array type);
/// a bare identifier resolved via [`binding_type`]; and a computed member on a
/// `Record<_, V>`-typed object (yielding `V`). Returns `None` when a hop is not
/// syntactically resolvable.
fn expr_declared_type<'a>(
    expr: &'a Expression<'a>,
    semantic: &oxc_semantic::Semantic<'a>,
) -> Option<&'a TSType<'a>> {
    match expr {
        Expression::TSNonNullExpression(inner) => expr_declared_type(&inner.expression, semantic),
        Expression::ParenthesizedExpression(inner) => {
            expr_declared_type(&inner.expression, semantic)
        }
        Expression::TSAsExpression(as_expr) => Some(&as_expr.type_annotation),
        Expression::CallExpression(call) => {
            let Expression::StaticMemberExpression(member) = &call.callee else {
                return None;
            };
            if !ELEMENT_PRESERVING_METHODS.contains(&member.property.name.as_str()) {
                return None;
            }
            expr_declared_type(&member.object, semantic)
        }
        Expression::Identifier(ident) => binding_type(ident, semantic),
        Expression::ComputedMemberExpression(member) => {
            expr_declared_type(&member.object, semantic).and_then(record_value_type)
        }
        _ => None,
    }
}

/// Resolves the declared TYPE of identifier `ident`'s binding: its `: T`
/// annotation, or — failing that — the type derived from its `const` initializer
/// (e.g. `const s = expr as Record<_, _>` yields the cast type, `const xs =
/// arr.sort(...)` yields `arr`'s type). Returns `None` when neither is resolvable.
fn binding_type<'a>(
    ident: &IdentifierReference,
    semantic: &oxc_semantic::Semantic<'a>,
) -> Option<&'a TSType<'a>> {
    if let Some(ty) = binding_declared_type(ident, semantic) {
        return Some(ty);
    }
    binding_const_init(ident, semantic).and_then(|init| expr_declared_type(init, semantic))
}

/// Extracts the element type of an array type: `T[]` → `T`, and `Array<T>` /
/// `ReadonlyArray<T>` → `T`. Any other type has no array element.
fn array_type_element<'a>(ty: &'a TSType<'a>) -> Option<&'a TSType<'a>> {
    match ty {
        TSType::TSArrayType(arr) => Some(&arr.element_type),
        TSType::TSTypeReference(reference) => {
            let TSTypeName::IdentifierReference(name) = &reference.type_name else {
                return None;
            };
            if name.name.as_str() != "Array" && name.name.as_str() != "ReadonlyArray" {
                return None;
            }
            reference.type_arguments.as_ref()?.params.first()
        }
        _ => None,
    }
}

/// Extracts the value type of a record/index-signature type: `Record<K, V>` → `V`,
/// or `{ [k: K]: V }` → `V`. Any other type has no record value.
fn record_value_type<'a>(ty: &'a TSType<'a>) -> Option<&'a TSType<'a>> {
    match ty {
        TSType::TSTypeReference(reference) => {
            let TSTypeName::IdentifierReference(name) = &reference.type_name else {
                return None;
            };
            if name.name.as_str() != "Record" {
                return None;
            }
            reference.type_arguments.as_ref()?.params.get(1)
        }
        TSType::TSTypeLiteral(lit) => lit.members.iter().find_map(|member| match member {
            TSSignature::TSIndexSignature(sig) => Some(&sig.type_annotation.type_annotation),
            _ => None,
        }),
        _ => None,
    }
}

/// Resolves `ident` to its binding and returns the declared `TSType` from the
/// enclosing `FormalParameter` (`x: T`) or `VariableDeclarator` (`const x: T`).
/// Returns `None` when the binding has no syntactic type annotation. Mirrors the
/// resolution in [`resolves_to_nonempty_tuple_type`].
fn binding_declared_type<'a>(
    ident: &IdentifierReference,
    semantic: &oxc_semantic::Semantic<'a>,
) -> Option<&'a TSType<'a>> {
    let ref_id = ident.reference_id.get()?;
    let scoping = semantic.scoping();
    let sym_id = scoping.get_reference(ref_id).symbol_id()?;
    let nodes = semantic.nodes();
    let decl_node_id = scoping.symbol_declaration(sym_id);
    for kind in std::iter::once(nodes.kind(decl_node_id))
        .chain(nodes.ancestor_kinds(decl_node_id))
    {
        let annotation = match kind {
            AstKind::FormalParameter(param) => &param.type_annotation,
            AstKind::VariableDeclarator(decl) => &decl.type_annotation,
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) | AstKind::Program(_) => {
                return None;
            }
            _ => continue,
        };
        return annotation.as_ref().map(|ann| &ann.type_annotation);
    }
    None
}

/// Resolves `ident` to a same-binding `const` declaration and returns its
/// initializer expression, letting derivation see through an unannotated
/// `const xs = arr.sort(...)`. Restricted to `const`: a `let`/`var` may be
/// reassigned to a shorter or differently-typed array. Returns `None` when the
/// binding is not a `const` variable.
fn binding_const_init<'a>(
    ident: &IdentifierReference,
    semantic: &oxc_semantic::Semantic<'a>,
) -> Option<&'a Expression<'a>> {
    let ref_id = ident.reference_id.get()?;
    let scoping = semantic.scoping();
    let sym_id = scoping.get_reference(ref_id).symbol_id()?;
    let nodes = semantic.nodes();
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let mut init: Option<&'a Expression<'a>> = None;
    for kind in std::iter::once(nodes.kind(decl_node_id))
        .chain(nodes.ancestor_kinds(decl_node_id))
    {
        match kind {
            AstKind::VariableDeclarator(decl) => {
                init = decl.init.as_ref();
            }
            AstKind::VariableDeclaration(decl) => {
                return if decl.kind == VariableDeclarationKind::Const {
                    init
                } else {
                    None
                };
            }
            AstKind::FormalParameter(_)
            | AstKind::Function(_)
            | AstKind::ArrowFunctionExpression(_)
            | AstKind::Program(_) => return None,
            _ => continue,
        }
    }
    None
}

/// gl-matrix fixed-size vector/matrix/quaternion type names. Each is a
/// fixed-length tuple of at least two `number`s (`vec2` = `[number, number]`,
/// `mat4` = a 16-element tuple), so any in-range index — including `[0]` and
/// `[length - 1]` — is always present.
const GLMATRIX_FIXED_TYPES: [&str; 9] =
    ["vec2", "vec3", "vec4", "mat2", "mat2d", "mat3", "mat4", "quat", "quat2"];

/// Returns true when `ident`'s binding is annotated with a gl-matrix fixed-size
/// type ([`GLMATRIX_FIXED_TYPES`]) that is imported from `gl-matrix` — making any
/// in-range index read in-bounds. Mirrors [`resolves_to_nonempty_tuple_type`]:
/// resolves the receiver to its declaration and reads the `type_annotation` on
/// the enclosing `FormalParameter` (`v: vec2`) or `VariableDeclarator`
/// (`const v: mat4`). The annotation must be a bare `TSTypeReference` whose name
/// is a gl-matrix type AND resolves to a `gl-matrix` import, so a same-named
/// local type cannot trigger the exemption.
fn resolves_to_glmatrix_fixed_type(
    ident: &IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Some(ref_id) = ident.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let nodes = semantic.nodes();
    let decl_node_id = scoping.symbol_declaration(sym_id);
    for kind in std::iter::once(nodes.kind(decl_node_id))
        .chain(nodes.ancestor_kinds(decl_node_id))
    {
        let annotation = match kind {
            AstKind::FormalParameter(param) => &param.type_annotation,
            AstKind::VariableDeclarator(decl) => &decl.type_annotation,
            // Leaving the binding's own declaration without finding an annotation.
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) | AstKind::Program(_) => {
                return false;
            }
            _ => continue,
        };
        return annotation
            .as_ref()
            .is_some_and(|ann| ts_type_is_glmatrix_fixed(&ann.type_annotation, semantic));
    }
    false
}

/// Returns true when `ty` is a `TSTypeReference` to a gl-matrix fixed-size type
/// name ([`GLMATRIX_FIXED_TYPES`]) whose name resolves to a `gl-matrix` import.
/// Anchoring to the import keeps a same-named local type or alias from matching.
fn ts_type_is_glmatrix_fixed(ty: &TSType, semantic: &oxc_semantic::Semantic) -> bool {
    let TSType::TSTypeReference(reference) = ty else {
        return false;
    };
    let TSTypeName::IdentifierReference(name) = &reference.type_name else {
        return false;
    };
    GLMATRIX_FIXED_TYPES.contains(&name.name.as_str())
        && resolves_to_import_from(name, semantic, &["gl-matrix"])
}

/// Returns true when `ident`'s binding has a type annotation denoting a `string`:
/// the bare `string` keyword or a union that includes it (`string | undefined`).
/// Mirrors [`resolves_to_nonempty_tuple_type`]: resolves the reference to its
/// declaration and reads the `type_annotation` on the enclosing `FormalParameter`
/// (`word: string`) or `VariableDeclarator` (`const word: string`). A string is
/// falsy exactly when empty, so this is the receiver-type signal that lets a
/// truthiness early-exit (`if (!word) return`) prove non-emptiness — array types,
/// whose `[]` is truthy, never qualify.
fn binding_has_string_type(
    ident: &IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Some(ref_id) = ident.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let nodes = semantic.nodes();
    let decl_node_id = scoping.symbol_declaration(sym_id);
    for kind in std::iter::once(nodes.kind(decl_node_id))
        .chain(nodes.ancestor_kinds(decl_node_id))
    {
        let annotation = match kind {
            AstKind::FormalParameter(param) => &param.type_annotation,
            AstKind::VariableDeclarator(decl) => &decl.type_annotation,
            // Leaving the binding's own declaration without finding an annotation.
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) | AstKind::Program(_) => {
                return false;
            }
            _ => continue,
        };
        return annotation
            .as_ref()
            .is_some_and(|ann| ts_type_is_string(&ann.type_annotation));
    }
    false
}

/// Returns true when `ty` is the `string` keyword, or a union any of whose members
/// is `string` (`string | undefined`, `string | null`). A non-string member such
/// as a nullish type is harmless here: the truthiness guard discards every nullish
/// value, leaving only the (non-empty) string branch at the access.
fn ts_type_is_string(ty: &TSType) -> bool {
    match ty {
        TSType::TSStringKeyword(_) => true,
        TSType::TSUnionType(union) => union.types.iter().any(ts_type_is_string),
        _ => false,
    }
}

/// Returns true when `name` resolves to a same-scope `const`/`let` whose
/// initializer is a `RegExp.prototype.exec` or `String.prototype.match` call
/// (`re.exec(s)` / `s.match(re)`). The closest binding wins. A non-null result
/// of either is a match array whose index 0 (the full match) always exists.
fn resolves_to_regex_match(
    node: &oxc_semantic::AstNode,
    name: &str,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(node.id()) {
        let stmts: &[Statement] = match ancestor.kind() {
            AstKind::Program(prog) => &prog.body,
            AstKind::FunctionBody(body) => &body.statements,
            AstKind::BlockStatement(block) => &block.body,
            _ => continue,
        };
        for stmt in stmts {
            let Statement::VariableDeclaration(decl) = stmt else {
                continue;
            };
            for declarator in &decl.declarations {
                let BindingPattern::BindingIdentifier(id) = &declarator.id else {
                    continue;
                };
                if id.name.as_str() != name {
                    continue;
                }
                // Closest binding wins: the first declarator matching `name`
                // decides, even if its initializer is not an exec/match call.
                return matches!(&declarator.init, Some(init) if is_regex_exec_or_match_call(init));
            }
        }
    }
    false
}

/// Returns true when `expr` is `<recv>.exec(...)` or `<recv>.match(...)` — the
/// two calls that yield a `RegExpExecArray`/`RegExpMatchArray | null`.
fn is_regex_exec_or_match_call(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = peel_transparent_wrappers(expr) else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    matches!(member.property.name.as_str(), "exec" | "match")
}

/// Returns true when `name` resolves to a same-scope `const` whose initializer is
/// a `String.prototype.split` call (`str.split(sep)`) — making `name[0]` and
/// `name[name.length - 1]` provably in-bounds. Mirrors
/// [`resolves_to_regex_match`]: walks ancestor scopes innermost-first so the
/// closest binding wins, and only a direct `const` qualifies (a `let` may be
/// reassigned to a shorter or empty array). `split` always returns an array with
/// at least one element, so a non-empty length is guaranteed.
fn resolves_to_split_call(
    node: &oxc_semantic::AstNode,
    name: &str,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(node.id()) {
        let stmts: &[Statement] = match ancestor.kind() {
            AstKind::Program(prog) => &prog.body,
            AstKind::FunctionBody(body) => &body.statements,
            AstKind::BlockStatement(block) => &block.body,
            _ => continue,
        };
        for stmt in stmts {
            let Statement::VariableDeclaration(decl) = stmt else {
                continue;
            };
            if decl.kind != VariableDeclarationKind::Const {
                continue;
            }
            for declarator in &decl.declarations {
                let BindingPattern::BindingIdentifier(id) = &declarator.id else {
                    continue;
                };
                if id.name.as_str() != name {
                    continue;
                }
                // Closest binding wins: the first declarator matching `name`
                // decides, even if its initializer is not a `split` call.
                return matches!(&declarator.init, Some(init) if is_split_call(init));
            }
        }
    }
    false
}

/// Strips runtime-transparent wrapper expressions — type assertions (`expr as T`,
/// `<T>expr`), `satisfies` narrowing (`expr satisfies T`), the non-null assertion
/// (`expr!`), and parentheses — returning the underlying expression. These wrappers are
/// compile-time-only, so `foo.split("/") as [string, ...string[]]` is the `split`
/// call at runtime. The call-shape predicates below peel through this first so a
/// wrapped `.split()`/`.exec()`/`.matchAll()` is still recognized.
fn peel_transparent_wrappers<'a, 'b>(expr: &'b Expression<'a>) -> &'b Expression<'a> {
    match expr {
        Expression::ParenthesizedExpression(p) => peel_transparent_wrappers(&p.expression),
        Expression::TSAsExpression(t) => peel_transparent_wrappers(&t.expression),
        Expression::TSSatisfiesExpression(t) => peel_transparent_wrappers(&t.expression),
        Expression::TSNonNullExpression(t) => peel_transparent_wrappers(&t.expression),
        Expression::TSTypeAssertion(t) => peel_transparent_wrappers(&t.expression),
        _ => expr,
    }
}

/// Returns true when `expr` is a `<recv>.split(...)` call. `<recv>` may itself be
/// a member chain (e.g. `this.name.split(...)`), so only the called property name
/// is checked. Any argument shape qualifies — `split()` with no argument returns
/// `[wholeString]`, still non-empty.
fn is_split_call(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = peel_transparent_wrappers(expr) else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    member.property.name.as_str() == "split"
}

/// Returns true when `name` is the element binding of an enclosing
/// `for (const name of <expr>.matchAll(...))` loop. Walks ancestors
/// innermost-first so the closest binding for-of wins (a nested loop shadowing
/// `name` is honored over an outer one). Each element of a `matchAll` iterator
/// is a `RegExpMatchArray` whose index 0 always exists, so `name[0]` in the body
/// is in-bounds.
fn is_matchall_for_of_element(
    node: &oxc_semantic::AstNode,
    name: &str,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(node.id()) {
        let AstKind::ForOfStatement(for_of) = ancestor.kind() else {
            continue;
        };
        if !for_of_binds_name(&for_of.left, name) {
            continue;
        }
        return is_matchall_call(&for_of.right);
    }
    false
}

/// Returns true when a `for...of` head `for (const <name> of ...)` binds exactly
/// the identifier `name` via a `const`/`let`/`var` declaration.
fn for_of_binds_name(left: &ForStatementLeft, name: &str) -> bool {
    let ForStatementLeft::VariableDeclaration(decl) = left else {
        return false;
    };
    decl.declarations.iter().any(|declarator| {
        matches!(&declarator.id, BindingPattern::BindingIdentifier(id) if id.name.as_str() == name)
    })
}

/// Returns true when `expr` is a `<recv>.matchAll(...)` call — the iterable form
/// that yields `RegExpMatchArray` elements. `<recv>` may itself be a member chain
/// (e.g. `this.text.matchAll(re)`), so only the called property name is checked.
fn is_matchall_call(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = peel_transparent_wrappers(expr) else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    member.property.name.as_str() == "matchAll"
}

/// Returns true when `name` is the element binding of an enclosing
/// `for (const name of <src>)` loop whose `<src>` is a `[K, V]`-tuple entries
/// source ([`is_key_value_entries_source`]). Walks ancestors innermost-first so
/// the closest binding for-of wins. Each element is a `[K, V]` tuple whose index 0
/// (the key) always exists, so `name[0]` in the loop body is in-bounds.
fn is_entries_for_of_element(
    node: &oxc_semantic::AstNode,
    name: &str,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(node.id()) {
        let AstKind::ForOfStatement(for_of) = ancestor.kind() else {
            continue;
        };
        if !for_of_binds_name(&for_of.left, name) {
            continue;
        }
        return is_key_value_entries_source(node, &for_of.right, semantic);
    }
    false
}

/// Array iteration methods that pass each element to the callback as its first
/// parameter. For a callback invoked on an entries source, that first parameter is
/// therefore a `[K, V]` tuple. `reduce`/`reduceRight` are excluded — their first
/// callback parameter is the accumulator, not the element.
const ENTRIES_ELEMENT_ITERATORS: [&str; 10] = [
    "map",
    "forEach",
    "flatMap",
    "filter",
    "find",
    "findIndex",
    "findLast",
    "findLastIndex",
    "some",
    "every",
];

/// Array methods whose callback is a comparator that receives TWO elements: both
/// the first and second parameters are element bindings (each a `[K, V]` tuple
/// when invoked on an entries source), so `a[0]` and `b[0]` in `(a, b) => …` are
/// both in-bounds.
const ENTRIES_COMPARATOR_ITERATORS: [&str; 2] = ["sort", "toSorted"];

/// Type/annotation names whose instances iterate to `[K, V]` two-tuples via
/// `.entries()`: `Map<K, V>`/`ReadonlyMap<K, V>` yield `[K, V]`, `Set<T>`/
/// `ReadonlySet<T>` yield `[T, T]`.
const MAP_SET_ANNOTATION_NAMES: [&str; 4] = ["Map", "ReadonlyMap", "Set", "ReadonlySet"];

/// Constructors whose result is a `Map`/`Set` instance.
const MAP_SET_CTOR_NAMES: [&str; 2] = ["Map", "Set"];

/// Returns true when the index access lives inside a callback whose element
/// parameter is `name`, and that callback is the argument of an entries iterator
/// invoked on a `[K, V]`-tuple source ([`is_key_value_entries_source`]). For an
/// element-yielding method (`.map`/`.forEach`/…) the element is the callback's
/// first parameter; for a comparator (`.sort`/`.toSorted`) BOTH parameters are
/// elements. Each such element is a `[K, V]` tuple, so `name[0]` (the key) is
/// in-bounds. Walks ancestors innermost-first so the closest enclosing callback
/// decides — a nested callback that binds `name` to a non-entries element keeps
/// the access flagged.
fn is_entries_callback_param(
    node: &oxc_semantic::AstNode,
    name: &str,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(node.id()) {
        let params = match ancestor.kind() {
            AstKind::ArrowFunctionExpression(arrow) => &arrow.params,
            AstKind::Function(func) => &func.params,
            _ => continue,
        };
        let AstKind::CallExpression(call) = nodes.parent_kind(ancestor.id()) else {
            return false;
        };
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return false;
        };
        let method = member.property.name.as_str();
        // `name` must fill an element slot of THIS callback: the first parameter for
        // an element-yielding method, or either comparator parameter for a sort. If
        // the closest enclosing callback binds `name` elsewhere (or its method is not
        // an entries iterator), it is not the entries element binder.
        let name_binds_element = if ENTRIES_ELEMENT_ITERATORS.contains(&method) {
            first_param_is_name(params, name)
        } else if ENTRIES_COMPARATOR_ITERATORS.contains(&method) {
            comparator_param_is_name(params, name)
        } else {
            return false;
        };
        if !name_binds_element {
            return false;
        }
        return is_key_value_entries_source(node, &member.object, semantic);
    }
    false
}

/// Returns true when `name` is the first or second formal parameter (a simple
/// identifier) of a comparator callback — both are elements of the same array.
fn comparator_param_is_name(params: &FormalParameters, name: &str) -> bool {
    params.items.iter().take(2).any(|param| {
        matches!(&param.pattern, BindingPattern::BindingIdentifier(id) if id.name.as_str() == name)
    })
}

/// Returns true when `expr` provably evaluates to an array whose elements are
/// `[K, V]` two-tuples: an `Object.entries(x)` result, a provable `Map`/`Set`
/// instance's `.entries()`, or either of those wrapped in `Array.from(...)` or an
/// array-spread literal `[...src]`. Every such element's index 0 (the key) is
/// guaranteed present. Conservative: an unresolved receiver returns false.
fn is_key_value_entries_source(
    node: &oxc_semantic::AstNode,
    expr: &Expression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    match peel_transparent_wrappers(expr) {
        Expression::CallExpression(call) => {
            let Expression::StaticMemberExpression(member) = &call.callee else {
                return false;
            };
            match member.property.name.as_str() {
                // `Array.from(<entries source>)` — bare global `Array`, one argument.
                // A second `mapFn` argument (`Array.from(src, fn)`) yields the mapped
                // results, not the `[K, V]` tuples, so it is excluded.
                "from" => {
                    call.arguments.len() == 1
                        && matches!(&member.object, Expression::Identifier(id) if id.name.as_str() == "Array")
                        && call
                            .arguments
                            .first()
                            .and_then(|arg| arg.as_expression())
                            .is_some_and(|inner| is_key_value_entries_source(node, inner, semantic))
                }
                // `Object.entries(...)` (bare global) or `<map|set>.entries()`.
                "entries" => {
                    matches!(&member.object, Expression::Identifier(id) if id.name.as_str() == "Object")
                        || expr_is_map_or_set(node, &member.object, semantic)
                }
                _ => false,
            }
        }
        // `[...<entries source>]` — a single spread of an entries source.
        Expression::ArrayExpression(arr) => {
            let mut elements = arr.elements.iter();
            match (elements.next(), elements.next()) {
                (Some(ArrayExpressionElement::SpreadElement(spread)), None) => {
                    is_key_value_entries_source(node, &spread.argument, semantic)
                }
                _ => false,
            }
        }
        _ => false,
    }
}

/// Returns true when `expr` provably denotes a `Map`/`Set` instance — so
/// `<expr>.entries()` yields `[K, V]` two-tuples. Recognized shapes: a direct
/// `new Map(...)`/`new Set(...)`; an identifier receiver whose binding is annotated
/// `Map`/`Set`/`ReadonlyMap`/`ReadonlySet` or is a `const` initialized with such a
/// construction; or a `this.<prop>` receiver whose class property is likewise
/// annotated or initialized. Conservative: an unresolved or differently-typed
/// receiver returns false, so the access keeps flagging.
fn expr_is_map_or_set(
    node: &oxc_semantic::AstNode,
    expr: &Expression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    match peel_transparent_wrappers(expr) {
        Expression::NewExpression(new_expr) => new_expr_constructs_map_or_set(new_expr),
        Expression::Identifier(ident) => {
            binding_declared_type(ident, semantic).is_some_and(ts_type_is_map_or_set)
                || binding_const_init(ident, semantic).is_some_and(init_is_map_or_set_construction)
        }
        Expression::StaticMemberExpression(member) => {
            matches!(&member.object, Expression::ThisExpression(_))
                && class_property_is_map_or_set(node, member.property.name.as_str(), semantic)
        }
        _ => false,
    }
}

/// Returns true when `new_expr` constructs a `Map` or `Set` — its callee is the
/// bare global identifier `Map`/`Set`.
fn new_expr_constructs_map_or_set(new_expr: &NewExpression) -> bool {
    matches!(&new_expr.callee, Expression::Identifier(id) if MAP_SET_CTOR_NAMES.contains(&id.name.as_str()))
}

/// Returns true when `expr` is (through transparent wrappers) a
/// `new Map(...)`/`new Set(...)` construction.
fn init_is_map_or_set_construction(expr: &Expression) -> bool {
    matches!(peel_transparent_wrappers(expr), Expression::NewExpression(new_expr) if new_expr_constructs_map_or_set(new_expr))
}

/// Returns true when `ty` is a `TSTypeReference` to `Map`/`Set`/`ReadonlyMap`/
/// `ReadonlySet` ([`MAP_SET_ANNOTATION_NAMES`]).
fn ts_type_is_map_or_set(ty: &TSType) -> bool {
    let TSType::TSTypeReference(reference) = ty else {
        return false;
    };
    let TSTypeName::IdentifierReference(name) = &reference.type_name else {
        return false;
    };
    MAP_SET_ANNOTATION_NAMES.contains(&name.name.as_str())
}

/// Returns true when the class enclosing `node` declares a property named
/// `prop_name` that is annotated `Map`/`Set`/`ReadonlyMap`/`ReadonlySet` or is
/// initialized with a `new Map(...)`/`new Set(...)` construction. Only the nearest
/// enclosing class is inspected — that is the instance `this` binds to; a property
/// inherited from a base class or absent here is unresolved, so it returns false.
fn class_property_is_map_or_set(
    node: &oxc_semantic::AstNode,
    prop_name: &str,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(node.id()) {
        let AstKind::ClassBody(body) = ancestor.kind() else {
            continue;
        };
        for element in &body.body {
            let ClassElement::PropertyDefinition(prop) = element else {
                continue;
            };
            if !property_key_is(&prop.key, prop_name) {
                continue;
            }
            let by_annotation = prop
                .type_annotation
                .as_ref()
                .is_some_and(|ann| ts_type_is_map_or_set(&ann.type_annotation));
            let by_init = prop.value.as_ref().is_some_and(init_is_map_or_set_construction);
            return by_annotation || by_init;
        }
        return false;
    }
    false
}

/// Returns true when a class property key is the identifier or string-literal
/// `name` (`x`, `"x"`, `["x"]`). Computed non-literal and private (`#x`) keys
/// don't match.
fn property_key_is(key: &PropertyKey, name: &str) -> bool {
    match key {
        PropertyKey::StaticIdentifier(id) => id.name.as_str() == name,
        PropertyKey::StringLiteral(s) => s.value.as_str() == name,
        _ => false,
    }
}

/// Returns true when the first formal parameter is a simple identifier named `name`.
fn first_param_is_name(params: &FormalParameters, name: &str) -> bool {
    matches!(
        params.items.first().map(|param| &param.pattern),
        Some(BindingPattern::BindingIdentifier(id)) if id.name.as_str() == name
    )
}

/// Returns true when a preceding sibling statement in the same block exits early
/// on `name` being nullish/falsy: `if (!name) return/throw`, `if (name === null)
/// return/throw`, or `if (name == null) return/throw`. Does not cross function
/// boundaries.
fn has_preceding_nullish_exit_guard(
    node: &oxc_semantic::AstNode,
    name: &str,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = node.id();
    let node_span_start = node.kind().span().start;
    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return false;
        }
        let parent = nodes.get_node(parent_id);
        let stmts: &[Statement] = match parent.kind() {
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => return false,
            AstKind::BlockStatement(block) => &block.body,
            AstKind::FunctionBody(body) => &body.statements,
            AstKind::Program(prog) => &prog.body,
            _ => {
                current_id = parent_id;
                continue;
            }
        };
        let our_idx = stmts
            .iter()
            .position(|s| s.span().start <= node_span_start && node_span_start < s.span().end);
        let Some(our_idx) = our_idx else { return false };
        return stmts[..our_idx].iter().any(|stmt| {
            matches!(stmt, Statement::IfStatement(if_stmt)
                if condition_is_nullish_check(&if_stmt.test, name)
                    && body_has_early_exit(&if_stmt.consequent))
        });
    }
}

/// Returns true when `expr` is a guard condition that holds whenever `name` is
/// nullish/falsy: `!name`, `name === null` / `name == null`,
/// `name === undefined` / `name == undefined`, or a compound `||` whose left or
/// right arm is itself such a check (e.g. `!name || name === "/"`).
///
/// The caller (`has_preceding_nullish_exit_guard`) only reaches the index access
/// when this condition is *false* — the guard is `if (test) { early_exit }`. For
/// `test = left || right`, fall-through means `!(left || right)`, i.e. both arms
/// are false. If either arm is a nullish check for `name`, that arm being false
/// proves `name` is non-nullish, so the access is safe. This is sound for `||`
/// only: under `&&`, fall-through is `!left || !right`, which does not prove
/// `name` is non-nullish even when one arm is `!name`, so `&&` is not recognized.
fn condition_is_nullish_check(expr: &Expression, name: &str) -> bool {
    match expr {
        Expression::UnaryExpression(unary) => {
            matches!(unary.operator, UnaryOperator::LogicalNot)
                && matches!(&unary.argument, Expression::Identifier(id) if id.name.as_str() == name)
        }
        Expression::BinaryExpression(bin) => {
            matches!(
                bin.operator,
                BinaryOperator::StrictEquality | BinaryOperator::Equality
            ) && binary_compares_identifier_to_nullish(&bin.left, &bin.right, name)
        }
        Expression::LogicalExpression(logical)
            if logical.operator == LogicalOperator::Or =>
        {
            condition_is_nullish_check(&logical.left, name)
                || condition_is_nullish_check(&logical.right, name)
        }
        _ => false,
    }
}

/// Returns true when one side of a binary comparison is the identifier `name`
/// and the other is the `null` literal or the `undefined` identifier
/// (order-insensitive).
fn binary_compares_identifier_to_nullish(
    left: &Expression,
    right: &Expression,
    name: &str,
) -> bool {
    let is_name = |e: &Expression| matches!(e, Expression::Identifier(id) if id.name.as_str() == name);
    let is_nullish = |e: &Expression| {
        matches!(e, Expression::NullLiteral(_))
            || matches!(e, Expression::Identifier(id) if id.name.as_str() == "undefined")
    };
    (is_name(left) && is_nullish(right)) || (is_nullish(left) && is_name(right))
}

/// Returns true when a preceding `if`-statement in the same block (or an
/// enclosing block/function/program, stopping at function boundaries) ensures
/// `obj_text` is a non-empty array: its test detects `obj_text` being
/// nullish/empty (see [`condition_is_nullish_or_empty_check`]) AND its consequent
/// assigns a non-empty array literal to that same `obj_text` (see
/// [`consequent_assigns_nonempty_array`]). After such a guard the array has at
/// least one element regardless of which branch ran — guard false ⇒ already
/// non-empty, guard true ⇒ assigned a non-empty literal — so a following
/// first/last read on `obj_text` is in-bounds.
///
/// Mirrors [`has_preceding_nullish_exit_guard`]'s block walk: anchors on the
/// statement containing the access in the innermost enclosing block/body/program
/// and scans its preceding siblings. The "same base" identity is the text of the
/// receiver of the `[0]` access (`obj_text`), matched against the test and the
/// assignment target by source text — the convention the other text-based
/// preceding-guard helpers (`scan_preceding_stmts`, `stmt_is_push_on`) use, so a
/// member-expression base like `options.domains` is matched exactly and a
/// different base (`options.commonName`) does not.
fn has_preceding_ensure_nonempty_guard(
    node: &oxc_semantic::AstNode,
    obj_text: &str,
    source: &str,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = node.id();
    let node_span_start = node.kind().span().start;
    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return false;
        }
        let parent = nodes.get_node(parent_id);
        let stmts: &[Statement] = match parent.kind() {
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => return false,
            AstKind::BlockStatement(block) => &block.body,
            AstKind::FunctionBody(body) => &body.statements,
            AstKind::Program(prog) => &prog.body,
            _ => {
                current_id = parent_id;
                continue;
            }
        };
        let our_idx = stmts
            .iter()
            .position(|s| s.span().start <= node_span_start && node_span_start < s.span().end);
        let Some(our_idx) = our_idx else { return false };
        return stmts[..our_idx].iter().any(|stmt| {
            matches!(stmt, Statement::IfStatement(if_stmt)
                if condition_is_nullish_or_empty_check(&if_stmt.test, obj_text, source)
                    && consequent_assigns_nonempty_array(&if_stmt.consequent, obj_text, source))
        });
    }
}

/// Returns true when `expr` (an `if` test) detects `obj_text` being nullish or
/// empty: `!obj_text`, `obj_text === null/undefined` / `== null/undefined`,
/// `obj_text.length === 0` / `== 0` (order-insensitive), or a `||` whose left or
/// right arm is itself such a check.
///
/// The caller only reaches the index access on fall-through — the guard is
/// `if (test) { obj_text = [literal] }`, so the access runs when `test` is
/// *false*. For `test = A || B`, fall-through means `!A && !B`: if either arm is
/// a nullish/empty check of the base, that arm being false proves the base is
/// non-nullish/non-empty, so the read is in-bounds. This soundness holds for
/// `||` only — under `&&`, fall-through is `!A || !B`, which does not prove the
/// base non-empty even when one arm is an empty check — so `&&` is not
/// recognized (mirrors [`condition_is_nullish_check`]).
fn condition_is_nullish_or_empty_check(expr: &Expression, obj_text: &str, source: &str) -> bool {
    match expr {
        Expression::ParenthesizedExpression(paren) => {
            condition_is_nullish_or_empty_check(&paren.expression, obj_text, source)
        }
        Expression::UnaryExpression(unary) => {
            matches!(unary.operator, UnaryOperator::LogicalNot)
                && expr_text(&unary.argument, source) == obj_text
        }
        Expression::BinaryExpression(bin) => {
            matches!(
                bin.operator,
                BinaryOperator::StrictEquality | BinaryOperator::Equality
            ) && binary_is_base_nullish_or_empty(&bin.left, &bin.right, obj_text, source)
        }
        Expression::LogicalExpression(logical)
            if logical.operator == LogicalOperator::Or =>
        {
            condition_is_nullish_or_empty_check(&logical.left, obj_text, source)
                || condition_is_nullish_or_empty_check(&logical.right, obj_text, source)
        }
        _ => false,
    }
}

/// Returns true when a strict/loose equality compares `obj_text` to a nullish
/// literal (`obj_text === null`, `obj_text == undefined`) or `obj_text.length`
/// to `0` (`obj_text.length === 0`). Order-insensitive. The `.length` receiver
/// must be the same `obj_text`, so `other.length === 0` does not qualify.
fn binary_is_base_nullish_or_empty(
    left: &Expression,
    right: &Expression,
    obj_text: &str,
    source: &str,
) -> bool {
    let is_base = |e: &Expression| expr_text(e, source) == obj_text;
    let is_nullish = |e: &Expression| {
        matches!(e, Expression::NullLiteral(_))
            || matches!(e, Expression::Identifier(id) if id.name.as_str() == "undefined")
    };
    let is_base_length = |e: &Expression| is_length_of(e, obj_text, source);
    let is_zero = |e: &Expression| is_numeric_literal(e, 0, source);
    (is_base(left) && is_nullish(right))
        || (is_nullish(left) && is_base(right))
        || (is_base_length(left) && is_zero(right))
        || (is_zero(left) && is_base_length(right))
}

/// Returns true when `stmt` (an `if` consequent — a `BlockStatement` or a bare
/// statement) contains an assignment `obj_text = [<>= 1 static element>]` to the
/// same base. Only a plain `=` to a non-empty array literal qualifies: an empty
/// literal (`= []`), a literal whose only elements are spreads (length unknown),
/// a non-array value, a compound assignment, or an assignment to a different
/// base does not — so the array cannot be proven non-empty and the read stays
/// flagged.
fn consequent_assigns_nonempty_array(stmt: &Statement, obj_text: &str, source: &str) -> bool {
    match stmt {
        Statement::BlockStatement(block) => block
            .body
            .iter()
            .any(|s| consequent_assigns_nonempty_array(s, obj_text, source)),
        Statement::ExpressionStatement(expr_stmt) => {
            let Expression::AssignmentExpression(assign) = &expr_stmt.expression else {
                return false;
            };
            if assign.operator != AssignmentOperator::Assign {
                return false;
            }
            let left_text =
                &source[assign.left.span().start as usize..assign.left.span().end as usize];
            left_text == obj_text
                && matches!(&assign.right, Expression::ArrayExpression(arr) if is_static_nonempty_array(arr))
        }
        _ => false,
    }
}

/// Returns true when the flagged index access is the initializer of a `const`/`let`
/// binding whose every use is null/undefined-guarded — so an out-of-bounds
/// `undefined` is already handled and the access is defensive, not accidental. A
/// use qualifies as guarded when it is consumed as a condition, short-circuited by
/// an optional chain or fallback, dominated by a preceding early-exit nullish
/// guard, or sits inside a truthy-narrowed branch (see [`reference_is_guarded`]).
///
/// Conservative by construction: if the access is not a `const`/`let` initializer,
/// the binding has no uses, or any reference is reachable unguarded, the function
/// returns false and the access stays flagged.
fn result_binding_is_null_guarded(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Some((symbol_id, name)) = declarator_binding_of_init(node, semantic) else {
        return false;
    };
    all_references_guarded(symbol_id, &name, semantic)
}

/// Returns the `(symbol_id, name)` of the binding when the flagged access is the
/// initializer of a `const`/`let` declarator with a plain identifier pattern
/// (`const x = arr[0]`). A destructuring pattern, a non-initializer position, or a
/// `var` (function-scoped, may be reassigned across the scope) does not qualify.
fn declarator_binding_of_init(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> Option<(oxc_semantic::SymbolId, String)> {
    let nodes = semantic.nodes();
    let node_span = node.kind().span();
    for ancestor in nodes.ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::VariableDeclarator(declarator) => {
                if declarator.kind == VariableDeclarationKind::Var {
                    return None;
                }
                let init_span = declarator.init.as_ref()?.span();
                if init_span != node_span {
                    return None;
                }
                let BindingPattern::BindingIdentifier(id) = &declarator.id else {
                    return None;
                };
                let symbol_id = id.symbol_id.get()?;
                return Some((symbol_id, id.name.to_string()));
            }
            // The access feeds something other than a bare declarator initializer
            // (a call argument, a member access, an array literal, …) — the
            // result-binding exemption does not apply.
            AstKind::ExpressionStatement(_)
            | AstKind::CallExpression(_)
            | AstKind::Function(_)
            | AstKind::ArrowFunctionExpression(_) => return None,
            _ => {}
        }
    }
    None
}

/// Returns true when every resolved value reference to the binding is individually
/// guarded against `name` being nullish (see [`reference_is_guarded`]). A binding
/// with no references is not vouched safe (returns false): there is no consuming
/// read, so the original flag is harmless to keep and avoids a vacuous exemption.
fn all_references_guarded(
    symbol_id: oxc_semantic::SymbolId,
    name: &str,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let scoping = semantic.scoping();
    let mut saw_reference = false;
    for reference in scoping.get_resolved_references(symbol_id) {
        if !reference.is_value() {
            continue;
        }
        saw_reference = true;
        if !reference_is_guarded(reference.node_id(), name, semantic) {
            return false;
        }
    }
    saw_reference
}

/// Returns true when `parent_kind` (the direct parent of a reference at `ref_span`)
/// consumes the binding `name` purely as a condition, never dereferencing it:
///   - `!name` (logical-not);
///   - an operand of `&&` / `||` / `??` (short-circuit / coalesce test);
///   - an equality comparison against `null` / `undefined`
///     (`name === null`, `name != undefined`, and the mirrored forms);
///   - the bare test of `if (name)`, `name ? … : …`, or `while (name)`.
///
/// In all these positions the binding is read for truthiness or compared to
/// nullish — the element behind a possibly-out-of-bounds index is never accessed —
/// so the read is inherently safe regardless of the array's length.
fn reference_is_pure_condition(
    parent_kind: AstKind,
    ref_span: oxc_span::Span,
    name: &str,
) -> bool {
    match parent_kind {
        AstKind::UnaryExpression(unary) => {
            matches!(unary.operator, UnaryOperator::LogicalNot)
                && unary.argument.span() == ref_span
        }
        AstKind::LogicalExpression(logical) => {
            logical.left.span() == ref_span || logical.right.span() == ref_span
        }
        AstKind::BinaryExpression(bin) => {
            matches!(
                bin.operator,
                BinaryOperator::StrictEquality
                    | BinaryOperator::Equality
                    | BinaryOperator::StrictInequality
                    | BinaryOperator::Inequality
            ) && binary_compares_identifier_to_nullish(&bin.left, &bin.right, name)
        }
        AstKind::IfStatement(if_stmt) => if_stmt.test.span() == ref_span,
        AstKind::ConditionalExpression(cond) => cond.test.span() == ref_span,
        AstKind::WhileStatement(while_stmt) => while_stmt.test.span() == ref_span,
        _ => false,
    }
}

/// Returns true when the reference node `ref_node_id` (an `IdentifierReference` to
/// the binding) is used in a position that handles `name` being nullish:
///   0. it is consumed purely as a condition — `!name`, an operand of
///      `&&`/`||`/`??`, an equality comparison against `null`/`undefined`, or a
///      bare `if`/ternary/`while` test (see [`reference_is_pure_condition`]);
///   1. it is the base of an optional chain — `name?.foo`, `name?.[i]`, `name?.()`;
///   2. a preceding early-exit nullish guard dominates it — `if (!name) return`
///      earlier in its block (see [`has_preceding_nullish_exit_guard`]);
///   3. it is the tested operand of a truthy guard whose consequent/expression it
///      stays within — `if (name) { … }`, `if (name && …) { … }`,
///      `name && name.foo`, `name ? name.foo : d`. The reference must sit inside
///      the guarded branch, so a use outside the narrowing is not vouched safe.
fn reference_is_guarded(
    ref_node_id: oxc_semantic::NodeId,
    name: &str,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let ref_span = nodes.kind(ref_node_id).span();
    let parent = nodes.parent_node(ref_node_id);
    // 0: the reference is consumed purely as a condition — reading the binding for
    // truthiness or comparing it to nullish never dereferences the (possibly
    // `undefined`) element, so it is inherently safe.
    if reference_is_pure_condition(parent.kind(), ref_span, name) {
        return true;
    }
    // 1: the reference is the base of an optional chain — `name?.foo`,
    // `name?.[i]`, `name?.()`. The `?.` short-circuits when `name` is nullish.
    match parent.kind() {
        AstKind::StaticMemberExpression(member) => {
            if member.optional && member.object.span() == ref_span {
                return true;
            }
        }
        AstKind::ComputedMemberExpression(member) => {
            if member.optional && member.object.span() == ref_span {
                return true;
            }
        }
        AstKind::CallExpression(call) => {
            if call.optional && call.callee.span() == ref_span {
                return true;
            }
        }
        _ => {}
    }
    // 2: an early-exit nullish guard preceding the reference in its block dominates
    // it — `if (!name) return/throw` runs before the reference, so reaching the
    // reference proves `name` was non-nullish.
    if has_preceding_nullish_exit_guard(nodes.get_node(ref_node_id), name, semantic) {
        return true;
    }
    // 3: the reference is dominated by a truthy guard on `name` — an enclosing
    // `if (name …) { <ref> }` / `name ? <ref> : …` / `name && <ref>` whose test
    // truthy-narrows `name`, and `<ref>` lives in the narrowed branch.
    reference_in_truthy_narrowed_branch(ref_node_id, ref_span, name, nodes)
}

/// Returns true when an enclosing construct truthy-narrows `name` and the
/// reference at `ref_span` lives in the branch that runs only when `name` was
/// truthy: the consequent of `if (<truthy name guard>)`, the consequent of a
/// `<truthy name guard> ? <ref> : …` ternary, or the right operand of a
/// `<truthy name guard> && <ref>` logical-and. The guard test is recognized by
/// [`condition_truthy_narrows`].
fn reference_in_truthy_narrowed_branch(
    ref_node_id: oxc_semantic::NodeId,
    ref_span: oxc_span::Span,
    name: &str,
    nodes: &oxc_semantic::AstNodes,
) -> bool {
    for ancestor in nodes.ancestors(ref_node_id) {
        match ancestor.kind() {
            AstKind::IfStatement(if_stmt) => {
                if condition_truthy_narrows(&if_stmt.test, name)
                    && span_contains(if_stmt.consequent.span(), ref_span)
                {
                    return true;
                }
            }
            AstKind::ConditionalExpression(cond) => {
                if condition_truthy_narrows(&cond.test, name)
                    && span_contains(cond.consequent.span(), ref_span)
                {
                    return true;
                }
            }
            AstKind::LogicalExpression(logical) => {
                if matches!(logical.operator, LogicalOperator::And)
                    && condition_truthy_narrows(&logical.left, name)
                    && span_contains(logical.right.span(), ref_span)
                {
                    return true;
                }
            }
            // Leaving the binding's scope without finding a guard.
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) | AstKind::Program(_) => {
                return false;
            }
            _ => {}
        }
    }
    false
}

/// Returns true when `expr` is a condition whose truth implies `name` is truthy
/// (non-nullish): a bare `name`, or `name && …` where the left operand is the
/// truthy check on `name`. This is the narrowing that makes a use of `name` in the
/// guarded branch safe.
fn condition_truthy_narrows(expr: &Expression, name: &str) -> bool {
    match expr {
        Expression::Identifier(id) => id.name.as_str() == name,
        Expression::ParenthesizedExpression(paren) => {
            condition_truthy_narrows(&paren.expression, name)
        }
        Expression::LogicalExpression(logical) => {
            matches!(logical.operator, LogicalOperator::And)
                && condition_truthy_narrows(&logical.left, name)
        }
        _ => false,
    }
}

/// Returns true when the boundary access at `node` sits inside the body of an
/// enclosing `while (<test>)` loop whose `<test>` proves `name` is non-null on
/// every iteration (see [`while_test_proves_non_null`]). Walks ancestors
/// innermost-first and stops at a function boundary, so a `while` in an enclosing
/// function does not vouch for an access in a nested one. The caller pairs this
/// with [`resolves_to_regex_match`]: the exec/match provenance is what makes a
/// non-null `name` guarantee index 0, so this predicate alone does not exempt an
/// arbitrary `while (arr != null) { arr[0] }`.
fn is_in_while_non_null_loop_on(
    node: &oxc_semantic::AstNode,
    name: &str,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let node_span = node.kind().span();
    for ancestor in nodes.ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::WhileStatement(while_stmt) => {
                if while_test_proves_non_null(&while_stmt.test, name)
                    && span_contains(while_stmt.body.span(), node_span)
                {
                    return true;
                }
            }
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) | AstKind::Program(_) => {
                return false;
            }
            _ => {}
        }
    }
    false
}

/// Returns true when `expr` is a loop test whose truth implies `name` is
/// non-nullish inside the body: `name != null`, `name !== null`,
/// `name != undefined`, `name !== undefined` (order-insensitive), or a bare truthy
/// `name` (including `name && …`). The truthy cases reuse
/// [`condition_truthy_narrows`].
fn while_test_proves_non_null(expr: &Expression, name: &str) -> bool {
    match expr {
        Expression::BinaryExpression(bin) => {
            matches!(
                bin.operator,
                BinaryOperator::StrictInequality | BinaryOperator::Inequality
            ) && binary_compares_identifier_to_nullish(&bin.left, &bin.right, name)
        }
        _ => condition_truthy_narrows(expr, name),
    }
}

/// Returns true when the boundary access at `node` sits in the truthy branch of a
/// same-variable truthiness guard on the indexed binding `ident`, AND that binding
/// is known to be a string. A truthy string is non-empty (`""` is the only falsy
/// string), so the boundary read is in-bounds; the string restriction is essential
/// because an empty array is truthy, leaving a truthy-guarded array index unsafe.
///
/// The guard shapes are `str ? <branch> : …`, `str && <branch>`, and
/// `if (str) { <branch> }`, where `<branch>` contains the access — recognized via
/// [`reference_in_truthy_narrowed_branch`]. String evidence is either a plain
/// `string` annotation on the binding, or a string-exclusive method called on the
/// same variable inside the guarded branch (see [`branch_has_string_method_on`]).
fn is_in_same_var_truthy_string_guard(
    node: &oxc_semantic::AstNode,
    ident: &IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let name = ident.name.as_str();
    let nodes = semantic.nodes();
    let node_span = node.kind().span();
    if !reference_in_truthy_narrowed_branch(node.id(), node_span, name, nodes) {
        return false;
    }
    binding_has_string_type(ident, semantic)
        || branch_has_string_method_on(node, name, semantic)
}

/// String-exclusive method names — present on `String.prototype` but not
/// `Array.prototype`. A call of one of these on a variable proves the variable is
/// a string, so an enclosing truthiness guard on it bounds the boundary access.
/// Methods shared with arrays (`slice`, `concat`, `indexOf`, `includes`) are
/// deliberately excluded: they would not distinguish a string from an array.
const STRING_EXCLUSIVE_METHODS: [&str; 12] = [
    "toUpperCase",
    "toLowerCase",
    "charAt",
    "charCodeAt",
    "codePointAt",
    "substring",
    "substr",
    "normalize",
    "padStart",
    "padEnd",
    "startsWith",
    "endsWith",
];

/// Returns true when, within the enclosing truthiness-guarded branch around the
/// access at `node`, the variable `name` is the receiver of a string-exclusive
/// method call (`name.toUpperCase()`, `name.slice(1)` is NOT counted — see
/// [`STRING_EXCLUSIVE_METHODS`]). Scans the consequent of the nearest enclosing
/// `if (name)` / `name ? … : …` or the right operand of `name && …` for such a
/// call, proving `name` is a string in the same scope as the access.
fn branch_has_string_method_on(
    node: &oxc_semantic::AstNode,
    name: &str,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let ref_span = node.kind().span();
    for ancestor in nodes.ancestors(node.id()) {
        let branch: &Expression = match ancestor.kind() {
            AstKind::ConditionalExpression(cond)
                if condition_truthy_narrows(&cond.test, name)
                    && span_contains(cond.consequent.span(), ref_span) =>
            {
                &cond.consequent
            }
            AstKind::LogicalExpression(logical)
                if matches!(logical.operator, LogicalOperator::And)
                    && condition_truthy_narrows(&logical.left, name)
                    && span_contains(logical.right.span(), ref_span) =>
            {
                &logical.right
            }
            AstKind::IfStatement(if_stmt)
                if condition_truthy_narrows(&if_stmt.test, name)
                    && span_contains(if_stmt.consequent.span(), ref_span) =>
            {
                return statement_has_string_method_on(&if_stmt.consequent, name);
            }
            AstKind::Function(_)
            | AstKind::ArrowFunctionExpression(_)
            | AstKind::Program(_) => return false,
            _ => continue,
        };
        if expression_has_string_method_on(branch, name) {
            return true;
        }
    }
    false
}

/// Returns true when `expr` is the bare identifier `name` or an index access into
/// it (`name[i]`). Indexing a string yields a (single-char) string, so a
/// string-exclusive method on `name[i]` proves `name` is a string just as a method
/// on `name` itself does — covering `str[0].toUpperCase()`.
fn receiver_roots_at_string(expr: &Expression, name: &str) -> bool {
    match expr {
        Expression::Identifier(id) => id.name.as_str() == name,
        Expression::ComputedMemberExpression(member) => {
            receiver_roots_at_string(&member.object, name)
        }
        _ => false,
    }
}

/// Recursively scans `expr` for a string-exclusive method call whose receiver is
/// the identifier `name` or an index into it (`name.toUpperCase()`,
/// `name[0].toUpperCase()`). Walks the operator nodes that hold sub-expressions of
/// a typical guarded branch — binary `+`, logical, member, call, parentheses —
/// without needing to cover every node kind.
fn expression_has_string_method_on(expr: &Expression, name: &str) -> bool {
    match expr {
        Expression::CallExpression(call) => {
            if let Expression::StaticMemberExpression(member) = &call.callee
                && STRING_EXCLUSIVE_METHODS.contains(&member.property.name.as_str())
                && receiver_roots_at_string(&member.object, name)
            {
                return true;
            }
            // The callee or any argument may itself contain the marker call.
            expression_has_string_method_on(&call.callee, name)
                || call
                    .arguments
                    .iter()
                    .filter_map(|arg| arg.as_expression())
                    .any(|arg| expression_has_string_method_on(arg, name))
        }
        Expression::BinaryExpression(bin) => {
            expression_has_string_method_on(&bin.left, name)
                || expression_has_string_method_on(&bin.right, name)
        }
        Expression::LogicalExpression(logical) => {
            expression_has_string_method_on(&logical.left, name)
                || expression_has_string_method_on(&logical.right, name)
        }
        Expression::StaticMemberExpression(member) => {
            expression_has_string_method_on(&member.object, name)
        }
        Expression::ComputedMemberExpression(member) => {
            expression_has_string_method_on(&member.object, name)
                || expression_has_string_method_on(&member.expression, name)
        }
        Expression::ParenthesizedExpression(paren) => {
            expression_has_string_method_on(&paren.expression, name)
        }
        _ => false,
    }
}

/// Scans an `if` consequent statement (an expression-statement or block) for a
/// string-exclusive method call on `name`. Only the direct statements are walked,
/// which covers the `if (str) { return str[0].toUpperCase(); }` idiom.
fn statement_has_string_method_on(stmt: &Statement, name: &str) -> bool {
    match stmt {
        Statement::BlockStatement(block) => block
            .body
            .iter()
            .any(|s| statement_has_string_method_on(s, name)),
        Statement::ExpressionStatement(expr_stmt) => {
            expression_has_string_method_on(&expr_stmt.expression, name)
        }
        Statement::ReturnStatement(ret) => ret
            .argument
            .as_ref()
            .is_some_and(|arg| expression_has_string_method_on(arg, name)),
        _ => false,
    }
}

/// Returns true when `outer` fully contains `inner`.
fn span_contains(outer: oxc_span::Span, inner: oxc_span::Span) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

/// Returns true when the index access lives inside a function whose parameter
/// list binds `name`, and that function is the argument of a `.then(...)` member
/// call — i.e. `something.then((name) => ... name[0] ...)`. This is the Cypress
/// `.then(($el) => $el[0])` pattern, where the wrapper is guaranteed non-empty.
fn is_then_callback_param(
    node: &oxc_semantic::AstNode,
    name: &str,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(node.id()) {
        let params = match ancestor.kind() {
            AstKind::ArrowFunctionExpression(arrow) => &arrow.params,
            AstKind::Function(func) => &func.params,
            _ => continue,
        };
        // `name` must be bound by this callback's parameter list. If not, the
        // enclosing function is not the binder — stop, the wrapper is not a
        // `.then` parameter.
        if !params_bind_name(params, name) {
            return false;
        }
        let parent = nodes.parent_node(ancestor.id());
        return matches!(parent.kind(), AstKind::CallExpression(call) if callee_is_then(&call.callee));
    }
    false
}

/// Returns true if a simple identifier parameter named `name` is present.
fn params_bind_name(params: &FormalParameters, name: &str) -> bool {
    params.items.iter().any(|param| {
        matches!(&param.pattern, BindingPattern::BindingIdentifier(id) if id.name.as_str() == name)
    })
}

/// Returns true if `callee` is a member access whose property is `then`
/// (e.g. `cy.get(...).then`), including optional-chained `?.then`.
fn callee_is_then(callee: &Expression) -> bool {
    matches!(callee, Expression::StaticMemberExpression(member) if member.property.name.as_str() == "then")
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
    use super::Check;
    
    fn run_on(src: &str) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn no_fp_early_exit_return() {
        let src = "function f(arr) { if (!arr.length) return; const x = arr[0]; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_early_exit_process_exit() {
        let src =
            "if (args.length === 0) { process.exit(1); } const cmd = args[0];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_early_exit_throw() {
        let src = "if (!items.length) throw new Error('empty'); const first = items[0];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_early_exit_continue_issue_7390() {
        let src = "for (const columns of lists) { if (!columns.length) continue; const fixed = columns[0].fixed === true ? 'left' : columns[0].fixed || 'default'; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_early_exit_break_issue_7390() {
        let src = "for (const c of lists) { if (!c.length) break; const x = c[0].id; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_no_length_guard_in_loop_issue_7390() {
        let src = "for (const c of lists) { const x = state.allColumns[0].id; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_expect_have_length_vitest() {
        let src = "expect(rows).toHaveLength(1); const first = rows[0];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_expect_length_to_be_issue_1985() {
        let src = "expect(releases.length).toBe(1); expect(releases[0]).toEqual({});";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_expect_length_to_be_multiple_accesses_issue_1985() {
        let src =
            "expect(releases.length).toBe(4); releases[0].name; releases[1].name;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_without_length_assertion_issue_1985() {
        let src = "const first = releases[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn unrelated_expect_does_not_suppress_issue_1985() {
        let src = "expect(other).toBe(1); const first = releases[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_expect_length_to_equal_issue_1341() {
        let src = "const traces = JSON.parse(x); expect(traces.length).toEqual(1); expect(traces[0].name).toEqual('test-span'); expect(traces[0].id).toEqual(127); expect(traces[0].duration).toEqual(321);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_expect_length_to_be_first_access_issue_1341() {
        let src = "expect(arr.length).toBe(3); const first = arr[0];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_without_length_assertion_issue_1341() {
        let src = "const first = arr[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn expect_length_to_equal_zero_is_not_guard_issue_1341() {
        let src = "expect(arr.length).toEqual(0); const first = arr[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn expect_length_to_be_zero_is_not_guard_issue_1341() {
        let src = "expect(arr.length).toBe(0); const first = arr[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_expect_length_greater_than_zero_issue_1341() {
        let src = "expect(arr.length).toBeGreaterThan(0); const first = arr[0];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_when_no_early_exit() {
        let src = "if (arr.length > 0) { doSomething(); } const x = arr[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_optional_chained_first_access_issue_1030() {
        assert!(run_on("const h = (arr: number[]) => arr?.[0];").is_empty());
    }

    #[test]
    fn no_fp_optional_chain_sequence_issue_1030() {
        assert!(run_on(
            "const f = (router: any, c: any) => !!router?.match(c)?.[0]?.[0]?.[0];"
        )
        .is_empty());
    }

    #[test]
    fn no_fp_optional_member_on_index_result_issue_1645() {
        // The issue's exact example: `methods[0]?.returns`. The `?.` on the
        // static member access acknowledges that `methods[0]` may be `undefined`.
        let src = "function f(methods) { let returns = methods[0]?.returns; return returns; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_optional_computed_on_index_result_issue_1645() {
        assert!(run_on("const f = (arr) => arr[0]?.[1];").is_empty());
    }

    #[test]
    fn no_fp_optional_call_on_index_result_issue_1645() {
        assert!(run_on("const f = (arr) => arr[0]?.();").is_empty());
    }

    #[test]
    fn still_flags_non_optional_member_on_index_result_issue_1645() {
        // Negative space: a plain (non-optional) `arr[0].prop` does not signal the
        // developer expects `undefined`, so the boundary read still flags.
        assert_eq!(run_on("const f = (arr) => arr[0].prop;").len(), 1);
    }

    #[test]
    fn still_flags_bare_first_access() {
        assert_eq!(run_on("const g = (arr: number[]) => arr[0];").len(), 1);
    }

    #[test]
    fn still_flags_bare_last_access() {
        assert_eq!(
            run_on("const i = (arr: number[]) => arr[arr.length - 1];").len(),
            1
        );
    }

    #[test]
    fn no_fp_cypress_then_dollar_unwrap_issue_1993() {
        let src = "cy.findByRole('listbox').then(($content) => { $content[0].parentElement; });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_cypress_then_dollar_click_issue_1993() {
        let src = "cy.findByText('x').then(($button) => { $button[0].click(); });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_plain_array_first_access_issue_1993() {
        let src = "const arr = getArr(); arr[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_dollar_var_not_then_param_issue_1993() {
        let src = "const $x = getList(); $x[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_index0_of_same_scope_array_literal_issue_1967() {
        let src = "const colors = ['a', 'b', 'c']; const x = colors[0];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_index0_of_array_literal_in_block_issue_1967() {
        let src = "function f() { const colors = ['a', 'b']; return colors[0]; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_index0_of_call_init_issue_1967() {
        let src = "const colors = getColors(); const x = colors[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_index0_of_param_issue_1967() {
        let src = "function f(arr) { return arr[0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_index0_of_spread_array_literal_issue_1967() {
        let src = "const colors = [...other]; const x = colors[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_index0_of_empty_array_literal_issue_1967() {
        let src = "const colors = []; const x = colors[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_last_index_of_array_literal_issue_1967() {
        // The exemption is scoped to index 0; `arr[arr.length - 1]` stays flagged.
        let src = "const colors = ['a', 'b']; const x = colors[colors.length - 1];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_index0_of_reassigned_let_issue_1967() {
        let src = "let colors = ['a', 'b']; const x = colors[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_index0_of_as_asserted_array_literal_issue_7147() {
        // hono's `const buffer = [''] as StringBufferWithCallbacks; buffer[0]` —
        // the `as T` assertion is transparent, so the literal is still non-empty.
        let src =
            "const buffer = [''] as StringBufferWithCallbacks; const x = buffer[0];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_index0_of_angle_bracket_asserted_array_literal_issue_7147() {
        let src = "const buf = <string[]>['x']; const y = buf[0];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_index0_of_satisfies_array_literal_issue_7147() {
        let src = "const buf = ['x'] satisfies string[]; const y = buf[0];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_index0_of_parenthesized_asserted_array_literal_issue_7147() {
        let src = "const buf = (['x'] as string[])!; const y = buf[0];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_index0_of_empty_asserted_array_literal_issue_7147() {
        // Peeling the assertion must not weaken the emptiness check.
        let src = "const arr = [] as string[]; const x = arr[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_index0_of_asserted_call_init_issue_7147() {
        // The initializer is a call, not an array literal — unaffected by peeling.
        let src = "const arr = getArr() as string[]; const x = arr[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_index0_of_const_object_of_nonempty_arrays_issue_6182() {
        // `units` is a function-local `const` object whose every value is a
        // non-empty array, so `units[unit][0]` is provably in-bounds.
        let src = "function f(unit) { const units = { days: ['day', 'days'], hours: ['hour', 'hr.'] }; return `next ${units[unit][0]}`; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_index0_of_const_object_in_ternary_issue_6182() {
        let src = "function f(unit, narrow) { const units = { days: ['day', 'days'] }; const fmtUnit = narrow ? units[unit][0] : unit; return fmtUnit; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_index0_of_let_object_issue_6182() {
        // A `let` object may be reassigned to one with empty arrays.
        let src = "function f(unit) { let units = { days: ['day'] }; return units[unit][0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_index0_of_object_with_empty_array_value_issue_6182() {
        let src = "function f(unit) { const units = { days: ['day'], hours: [] }; return units[unit][0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_index0_of_param_object_issue_6182() {
        let src = "function f(units, unit) { return units[unit][0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_index0_of_object_with_nonarray_value_issue_6182() {
        let src = "function f(unit) { const units = { days: ['day'], hours: getHours() }; return units[unit][0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_index0_of_object_with_spread_array_value_issue_6182() {
        let src = "function f(unit) { const units = { days: [...base] }; return units[unit][0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_index0_of_object_with_spread_property_issue_6182() {
        // An object-level spread (`...base`) can contribute empty arrays.
        let src = "function f(unit) { const units = { days: ['day'], ...base }; return units[unit][0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_index0_of_object_with_getter_value_issue_6182() {
        // A getter value is not a statically known array literal.
        let src = "function f(unit) { const units = { get days() { return []; } }; return units[unit][0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_index0_of_empty_object_issue_6182() {
        let src = "function f(unit) { const units = {}; return units[unit][0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_truthy_index0_guard_issue_1178() {
        // `if (data?.choices?.[0])` proves the element exists, so neither the
        // guard condition's own access nor same-array `[0]` reads in the block flag.
        let src = "function f(data) { if (data?.choices?.[0]) { console.log(data.choices[0].message); return data.choices[0].message; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_truthy_index0_guard_plain_array_issue_1178() {
        // Non-optional `if (arr[0])` is the truthiness equivalent of `if (arr.length)`.
        let src = "function f(arr) { if (arr[0]) { return arr[0].name; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_index0_inside_block_for_other_array_issue_1178() {
        // The guard is for `a`; `b[0]` inside the block is unrelated and stays flagged.
        let src = "function f(a, b) { if (a[0]) { return b[0].name; } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_predicate_call_on_index0_execa_issue_6237() {
        // `if (isPlainObject(arr[0]))` passes the first element to a boolean
        // predicate; the positive branch runs only when that element exists, so
        // neither the condition's own `[0]` read nor the body read is out-of-bounds.
        let src = "export const pipeToSubprocess = (sourceInfo, ...pipeArguments) => { if (isPlainObject(pipeArguments[0])) { return foo({ ...sourceInfo, boundOptions: { ...sourceInfo.boundOptions, ...pipeArguments[0] } }); } return bar; };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_predicate_call_on_index0_then_branch_issue_6237() {
        let src = "function f(arr) { if (isPlainObject(arr[0])) { use(arr[0]); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_array_isarray_on_index0_issue_6237() {
        // `Array.isArray(arr[0])` is true only when the first element exists.
        let src = "function f(arr) { if (Array.isArray(arr[0])) { return arr[0].length; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_negated_predicate_call_on_index0_issue_6237() {
        // A negated predicate (`!isPlainObject(arr[0])`) narrows the branch to the
        // element being absent or failing the predicate, so neither the condition's
        // own read nor the body read is proven in-bounds — both stay flagged.
        let src = "function f(arr) { if (!isPlainObject(arr[0])) { use(arr[0]); } }";
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn still_flags_array_isarray_on_array_itself_issue_6237() {
        // The structural signal is `arr[0]` as a call argument; `Array.isArray(arr)`
        // tests array-ness of the whole array, not non-emptiness, so `arr[0]` stays
        // flagged.
        let src = "function f(arr) { if (Array.isArray(arr)) { use(arr[0]); } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_index0_with_unrelated_condition_issue_6237() {
        let src = "function f(arr, someFlag) { if (someFlag) { use(arr[0]); } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_index0_no_guard_issue_6237() {
        let src = "function f(arr) { const x = arr[0]; return x; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_regex_exec_index0_after_null_guard_issue_1822() {
        // Canonical regex idiom: a non-null `exec` result is a `RegExpExecArray`
        // whose `[0]` (full match) always exists.
        let src = "function f(text) { const match = /`([^`]+)`(?!`)$/.exec(text); if (!match) { return null; } return { text: match[0], replaceWith: match[1] }; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_regex_match_index0_after_null_guard_issue_1822() {
        let src = "function f(s) { const m = s.match(/(\\d+)/); if (!m) return; return m[0]; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_regex_exec_index0_after_strict_null_guard_issue_1822() {
        let src = "function f(s) { const m = re.exec(s); if (m === null) { throw new Error('no match'); } return m[0]; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_regex_exec_index0_after_loose_null_guard_issue_1822() {
        let src = "function f(s) { const m = re.exec(s); if (m == null) return; return m[0]; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_regex_exec_index0_without_guard_issue_1822() {
        // No null guard: `m` may be null, so the read is not vouched safe here.
        let src = "function f(s) { const m = re.exec(s); return m[0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_plain_array_index0_after_null_guard_issue_1822() {
        // A plain array survives `if (!arr)` while still being empty, so `arr[0]`
        // can be `undefined` — the regex-origin requirement keeps this flagged.
        let src = "function f() { const arr = getArr(); if (!arr) return; return arr[0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_regex_exec_last_index_after_null_guard_issue_1822() {
        // Only `[0]` (the full match) is guaranteed; `[length - 1]` is not.
        let src = "function f(s) { const m = re.exec(s); if (!m) return; return m[m.length - 1]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_regex_exec_index0_truthy_ternary_guard_issue_6193() {
        // The issue's exact pattern: `m ? m[0] : def`. The ternary's truthy test on
        // `m` discards the `null` exec result, and a non-null exec result always has
        // index 0 — the ternary equivalent of the `if (!m) return; m[0]` guard.
        let src = "function f(val, def) { const m = regex.exec(val); return m ? m[0] : def; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_regex_match_index0_truthy_and_guard_issue_6193() {
        // The `&&` form: `m && m[0]` reaches `m[0]` only when `m` is truthy.
        let src = "function f(s) { const m = s.match(/(\\d+)/); return m && m[0]; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_regex_exec_index0_truthy_if_guard_issue_6193() {
        // The `if (m) { m[0] }` form: the access lives in the truthy-narrowed branch.
        let src = "function f(s) { const m = re.exec(s); if (m) { return m[0]; } return null; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_regex_exec_index0_truthy_guard_different_var_issue_6193() {
        // Negative space: the truthiness test is on a different variable (`n`), so it
        // does not narrow `m` — the `null` exec result is not discarded for `m[0]`.
        let src = "function f(s, n, def) { const m = re.exec(s); return n ? m[0] : def; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_plain_array_index0_truthy_ternary_guard_issue_6193() {
        // Negative space (soundness): an arbitrary array is not from exec/match, and
        // an empty array (`[]`) is truthy — so a truthy-guarded `arr[0]` can still be
        // `undefined`. The exec/match provenance requirement keeps this flagged.
        let src = "function f(def) { const arr = getArr(); return arr ? arr[0] : def; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_regex_exec_last_index_truthy_ternary_guard_issue_6193() {
        // Negative space: only `[0]` (the full match) is guaranteed for a non-null
        // exec result, so the truthy-guarded `[length - 1]` last read stays flagged.
        let src = "function f(s, def) { const m = re.exec(s); return m ? m[m.length - 1] : def; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_regex_exec_index0_in_ternary_alternate_issue_6193() {
        // Negative space: `m[0]` in the ALTERNATE branch runs when `m` is falsy
        // (`null`), where the access is unsafe — only the truthy consequent is
        // narrowed, so the alternate-branch read stays flagged.
        let src = "function f(s, def) { const m = re.exec(s); return m ? def : m[0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_regex_exec_index0_while_loose_null_loop_issue_6334() {
        // The issue's exact pattern: `token[0]` inside `while (token != null)`
        // where `token` is reassigned each iteration from `RE.exec(string)`. The
        // loop condition is the null narrowing, and a non-null exec result always
        // has a full match at index 0, so the read is in-bounds.
        let src = "function f(string) { let token = RE.exec(string); while (token != null) { const len = token[0].length; token = RE.exec(string); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_regex_match_index0_while_strict_null_loop_issue_6334() {
        let src = "function f(s) { let m = s.match(RE); while (m !== null) { use(m[0]); m = s.match(RE); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_regex_exec_index0_while_truthy_loop_issue_6334() {
        // Bare-truthy loop condition `while (m)` — a non-null exec result is truthy.
        let src = "function f(s) { let m = re.exec(s); while (m) { use(m[0]); m = re.exec(s); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_regex_exec_index0_no_while_or_guard_issue_6334() {
        // Negative control: `m[0]` from an exec result with NO while-condition and
        // NO null guard narrowing it stays flagged — the result may be `null`.
        let src = "function f(s) { const m = re.exec(s); return m[0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_arbitrary_array_index0_in_while_null_loop_issue_6334() {
        // Negative space (soundness): a `while (arr != null)` on an arbitrary array
        // (not from exec/match) proves `arr` is non-null but not non-empty — a
        // non-null empty array still has no index 0, so the read stays flagged.
        let src = "function f(arr) { while (arr != null) { use(arr[0]); arr = next(); } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_matchall_for_of_index0_issue_1639() {
        // The issue's exact pattern: `match[0]` inside
        // `for (const match of text.matchAll(re))`. Each yielded element is a
        // `RegExpMatchArray` whose `[0]` always exists, with no null guard needed.
        let src = "function f(text) { for (const match of text.matchAll(RE)) { const end = match.index + match[0].length; nodes.push(match[0]); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_matchall_for_of_index0_member_receiver_issue_1639() {
        // The `matchAll` receiver may be a member chain (`this.text.matchAll`).
        let src = "function f() { for (const m of this.text.matchAll(/x/g)) { return m[0]; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_matchall_for_of_last_index_issue_1639() {
        // Negative space: only `[0]` (the full match) is guaranteed. The
        // `[length - 1]` last-element read is not, so it stays flagged.
        let src = "function f(text) { for (const match of text.matchAll(RE)) { return match[match.length - 1]; } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_for_of_index0_not_matchall_issue_1639() {
        // Negative space: a plain `for...of` over an arbitrary array yields
        // elements that may themselves be empty arrays, so `row[0]` stays flagged.
        let src = "function f(rows) { for (const row of rows) { return row[0]; } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_index0_outside_matchall_for_of_issue_1639() {
        // The binding `match` only vouches reads inside the loop body; a same-named
        // `match[0]` outside the loop is unrelated and stays flagged.
        let src = "function f(text) { for (const match of text.matchAll(RE)) {} const match = getArr(); return match[0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_index0_after_push_same_scope_issue_1857() {
        // The second push accesses `data[0]`; the first push already ran, so it is
        // in-bounds.
        let src = "const data = []; data.push({ a: 1 }); data.push({ ...data[0], b: 2 });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_index0_after_push_in_nested_callback_issue_1857() {
        // Pushes at module scope run before the nested callback executes, so the
        // `data[0]` reads inside the test body are in-bounds.
        let src = "const data = []; data.push({ a: 1 }); data.push({ a: 2 }); test('x', () => { resolve(data[0]); expect(state).toStrictEqual(data[0]); });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_last_index_after_push_issue_1857() {
        // A single push guarantees the array is non-empty, so `length - 1` is valid.
        let src = "const data = []; data.push(1); const last = data[data.length - 1];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_index0_without_preceding_push_issue_1857() {
        let src = "const data = []; const x = data[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_index0_when_push_is_on_other_array_issue_1857() {
        // The push targets `other`, not `data`; `data` may still be empty.
        let src = "const data = []; other.push(1); const x = data[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_index0_when_push_is_conditional_issue_1857() {
        // The push is inside an `if`, so it may not run — the array can be empty.
        let src = "const data = []; if (cond) { data.push(1); } const x = data[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_index0_when_push_follows_access_issue_1857() {
        // The push comes after the access, so it does not vouch it safe.
        let src = "const data = []; const x = data[0]; data.push(1);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_duplicate_positions_on_chained_index_issue_1067() {
        // `rows[0][0][0]` is three computed accesses; each is a real unchecked
        // read, but they must land on distinct positions (their own `[`), not
        // collapse onto the chain start.
        let diags = run_on("const lgs = rows[0][0][0];");
        assert_eq!(diags.len(), 3);
        let mut positions: Vec<(usize, usize)> =
            diags.iter().map(|d| (d.line, d.column)).collect();
        positions.sort_unstable();
        positions.dedup();
        assert_eq!(positions.len(), 3, "each access must report a unique column");
    }

    #[test]
    fn no_fp_switch_on_length_case_and_default_issue_1602() {
        // The issue's exact example: `case 1: return authors[0]` and
        // `default: authors[authors.length - 1]` inside `switch (authors.length)`.
        let src = "function transform(authors) { if (!authors) { return 'Author Unknown'; } switch (authors.length) { case 0: return 'Author Unknown'; case 1: return authors[0]; case 2: return authors.join(' and '); default: const last = authors[authors.length - 1]; return last; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_switch_on_length_positive_case_first_issue_1602() {
        let src = "function f(arr) { switch (arr.length) { case 0: return; case 1: return arr[0]; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_switch_on_length_default_last_with_zero_case_issue_1602() {
        let src = "function f(arr) { switch (arr.length) { case 0: return; default: return arr[arr.length - 1]; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_switch_case_zero_first_access_issue_1602() {
        // `case 0:` means length is 0, so `arr[0]` is genuinely out of bounds.
        let src = "function f(arr) { switch (arr.length) { case 0: return arr[0]; } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_switch_default_without_zero_case_issue_1602() {
        // No `case 0:`, so `default` can still be reached with length 0.
        let src = "function f(arr) { switch (arr.length) { case 1: return; default: return arr[arr.length - 1]; } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_switch_on_other_discriminant_issue_1602() {
        // The discriminant is not `arr.length`, so the cases say nothing about
        // `arr`'s size.
        let src = "function f(arr, kind) { switch (kind) { case 'a': return arr[0]; } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_switch_on_other_array_length_issue_1602() {
        // `switch (other.length)` guards `other`, not `arr`.
        let src = "function f(arr, other) { switch (other.length) { case 1: return arr[0]; } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_switch_discriminant_first_element_issue_6180() {
        // The issue's exact pattern: `arr[0]` as the switch discriminant. An empty
        // array yields `undefined`, which matches no `case` and hits `default:`.
        let src = "const tokenToField = (token) => { switch (token[0]) { case 'S': return 'milliseconds'; case 's': return 'seconds'; default: return null; } };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_switch_discriminant_no_default_issue_6180() {
        // Even without a `default:` arm, a non-matching `undefined` discriminant
        // falls through past the switch without crashing.
        let src = "function f(arr) { switch (arr[0]) { case 'a': return 1; case 'b': return 2; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_switch_discriminant_last_element_issue_6180() {
        // The last-element read is equally safe in the discriminant position.
        let src = "function f(arr) { switch (arr[arr.length - 1]) { case 'a': return 1; default: return 0; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_first_element_in_switch_case_body_issue_6180() {
        // The discriminant `arr[0]` is exempt, but the same array's `arr[0]` in a
        // `case` consequent is a different role and stays flagged — the exemption
        // must not bleed from the discriminant position into the case body.
        let src = "function f(arr) { switch (arr[0]) { case 'a': return arr[0]; } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_chai_length_should_be_equal_issue_2312() {
        // The issue's exact pattern: `arr.length.should.be.equal(N)` throws if the
        // length differs, so the subsequent `arr[0]` read is in-bounds.
        let src = "mymigr.length.should.be.equal(1); mymigr[0].name.should.be.equal(\"InitUsers1530542855524\");";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_chai_length_should_be_greater_than_issue_2312() {
        let src = "rows.length.should.be.greaterThan(0); const first = rows[0];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_chai_length_should_be_at_least_issue_2312() {
        let src = "items.length.should.be.at.least(1); const last = items[items.length - 1];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_chai_should_have_length_issue_2312() {
        // The alternative chai syntax: `arr.should.have.length(N)`.
        let src = "rows.should.have.length(2); rows[0].id; rows[1].id;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_chai_should_have_length_of_issue_2312() {
        let src = "rows.should.have.lengthOf(2); const first = rows[0];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_chai_length_assertion_on_other_array_issue_2312() {
        // Negative space: the chai length assertion is on `other`, not `arr`, so
        // `arr` may still be empty.
        let src = "other.length.should.be.equal(1); const first = arr[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_bare_chai_should_without_length_issue_2312() {
        // Negative space: a bare `.should` on the array (not on its `.length` and
        // not a `.have.length` assertion) says nothing about its size.
        let src = "rows.should.be.an('array'); const first = rows[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_glmatrix_vec2_param_index0_issue_5276() {
        // The issue's `uniform_binding.ts` case: a `vec2` parameter is a fixed
        // two-element tuple, so `v[0]` is always in-bounds.
        let src = "import { vec2 } from 'gl-matrix'; function set(v: vec2) { return v[0]; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_glmatrix_mat4_param_index0_issue_5276() {
        // The issue's `fast_maths.ts` case: a `mat4` parameter is a fixed 16-element
        // tuple, so `src[0]` is always in-bounds.
        let src = "import { mat4 } from 'gl-matrix'; function inv(src: mat4) { return src[0]; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_glmatrix_type_import_index0_issue_5276() {
        // gl-matrix types are commonly brought in via `import type`.
        let src = "import type { vec3 } from 'gl-matrix'; function f(v: vec3) { return v[0]; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_glmatrix_mat4_last_index_issue_5276() {
        // A fixed-size matrix has a known length, so the last-element read is also
        // in-bounds.
        let src = "import { mat4 } from 'gl-matrix'; function f(m: mat4) { return m[m.length - 1]; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_glmatrix_quat_param_index0_issue_5276() {
        // Beyond the issue's listed types: `quat` is also a fixed-size tuple, so
        // the same exemption applies.
        let src = "import { quat } from 'gl-matrix'; function f(q: quat) { return q[0]; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_glmatrix_named_type_not_imported_issue_5276() {
        // Negative space: a same-named local type that is NOT imported from
        // `gl-matrix` must not trigger the exemption — without type info it could
        // be any shape, so the read stays flagged.
        let src = "type vec2 = number[]; function f(v: vec2) { return v[0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_plain_number_array_param_index0_issue_5276() {
        // Negative space: a genuine variable-length array stays flagged even when
        // `gl-matrix` is imported elsewhere in the file.
        let src = "import { vec2 } from 'gl-matrix'; function f(arr: number[]) { return arr[0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_mock_calls_index0_issue_2386() {
        // `<spy>.mock.calls[0]` is a jest/vitest mock-introspection array read.
        let src = "const arg = myMock.mock.calls[0];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_mock_calls_nested_index_issue_2386() {
        // The issue's exact pattern: `<spy>.mock.calls[0][1]` indexes a recorded
        // call's argument list — both computed accesses are exempt.
        let src = "const arg = myMock.mock.calls[0][1];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_mock_results_index0_issue_2386() {
        let src = "const r = fn.mock.results[0];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_mock_instances_index0_issue_2386() {
        let src = "const inst = fn.mock.instances[0];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_mock_calls_on_member_receiver_issue_2386() {
        // The issue's source line: the spy is itself a member chain.
        let src = "expect(driverAdapter.executeRawMock.mock.calls[0][0].sql).toEqual('COMMIT');";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_plain_array_index0_issue_2386() {
        // Negative space: an ordinary array read stays flagged.
        let src = "const x = someArray[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_calls_not_under_mock_issue_2386() {
        // Negative space: `calls` not hung off `.mock` is an ordinary array.
        let src = "const x = obj.calls[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_ternary_length_guard_consequent_issue_2276() {
        // The issue's Pattern 2: `Array.isArray(scale) && scale.length === 2`
        // bounds the element count, so `scale[0]` / `scale[1]` in the truthy
        // branch are in-bounds — the ternary equivalent of the `if`-condition
        // `.length` guard.
        let src = "function f(scale) { return Array.isArray(scale) && scale.length === 2 ? [scale[0], scale[1], 1] : scale; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_array_isarray_ternary_without_length_issue_2276() {
        // Negative space: the issue's Pattern 1. `Array.isArray(anchor)` proves
        // `anchor` is an array but NOT that it is non-empty — an empty array
        // passes the guard and `anchor[0]` is still `undefined`, so the
        // first-element read stays flagged.
        let src = "function f(anchor) { return Array.isArray(anchor) ? anchor[0] : anchor.x; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_ternary_length_guard_on_other_array_issue_2276() {
        // Negative space: the ternary condition bounds `other`, not `arr`, so
        // `arr` may still be empty.
        let src = "function f(arr, other) { return other.length === 2 ? arr[0] : null; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_ternary_length_guard_in_alternate_issue_2276() {
        // Negative space: the `.length` guard holds only in the truthy branch.
        // An access in the `alternate` (falsy) branch runs when the guard failed,
        // so it stays flagged.
        let src = "function f(arr) { return arr.length === 2 ? null : arr[0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_index0_ternary_alternate_length_zero_or_disjunct_execa_issue_6236() {
        // The issue's repro: `nextTokens[0]` sits in the ALTERNATE of a ternary whose
        // condition has `nextTokens.length === 0` as an OR-disjunct. The alternate runs
        // only when every disjunct is false, so `nextTokens.length > 0` and `[0]` is
        // in-bounds.
        let src = "const concatTokens = (tokens, nextTokens, isSeparated) => isSeparated || tokens.length === 0 || nextTokens.length === 0 ? [...tokens, ...nextTokens] : [...tokens.slice(0, -1), `${tokens.at(-1)}${nextTokens[0]}`, ...nextTokens.slice(1)];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_index0_ternary_alternate_length_eq_zero_issue_6236() {
        let src = "function f(arr) { return arr.length === 0 ? fallback : arr[0]; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_index0_ternary_alternate_length_lt_one_issue_6236() {
        let src = "function f(arr) { return arr.length < 1 ? x : arr[0]; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_index0_ternary_alternate_length_zero_under_and_issue_6236() {
        // Negative space: the `length === 0` check sits under `&&`. The condition being
        // false does NOT force `arr.length === 0` false (it can be false because `a` is
        // false), so the alternate is not proven non-empty — it stays flagged.
        let src = "function f(arr, a) { return a && arr.length === 0 ? x : arr[0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_index0_ternary_consequent_length_zero_issue_6236() {
        // Negative space: `arr.length === 0` truthy is the EMPTY case, and the
        // consequent runs on truthy — so the consequent `arr[0]` is out-of-bounds.
        let src = "function f(arr) { return arr.length === 0 ? arr[0] : y; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_index0_ternary_alternate_length_zero_other_array_issue_6236() {
        // Negative space: the guard is `other.length === 0`, but the access is on a
        // DIFFERENT array `arr`, so `arr` may still be empty.
        let src = "function f(arr, other) { return other.length === 0 ? x : arr[0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_truthy_index0_ternary_test_and_consequent_issue_3790() {
        // The issue's repro: a truthy `sources[0]` test narrows the consequent,
        // so the consequent's `sources[0]` is in-bounds, and the test's own
        // `sources[0]` is exempt too — the ternary equivalent of `if (sources[0])`.
        let src = "function f(sources: string[]) { return sources[0] ? new URL(sources[0]).hostname : null; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_truthy_index0_ternary_simple_issue_3790() {
        let src = "function f(arr) { const v = arr[0] ? arr[0].id : null; return v; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_length_ternary_consequent_issue_3790() {
        // The pre-existing `.length`-guarded ternary stays exempt.
        let src = "function f(arr) { const v = arr.length ? arr[0] : 0; return v; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_index0_ternary_alternate_issue_3790() {
        // Negative space: the truthy `a[0]` test narrows only the consequent, so
        // the test's own `a[0]` is exempt but the `alternate` access runs when
        // `a[0]` is falsy (`undefined`) — exactly one diagnostic, the alternate.
        let src = "function f(a: string[]) { return a[0] ? 'x' : a[0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_index0_in_right_operand_of_length_and_issue_6227() {
        // The issue's exact pattern: `node.arguments[0]` inside the right operand
        // of `node.arguments.length && ts.isStringLiteral(node.arguments[0])`. The
        // `&&` short-circuit evaluates the right side only when the left
        // `.length` check is truthy, so the index-0 access is guarded.
        let src = "function f(node) { const name = node.arguments.length && ts.isStringLiteral(node.arguments[0]) ? node.arguments[0].text : defaultName; return name; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_index0_simple_length_and_issue_6227() {
        // The bare idiom `arr.length && use(arr[0])` with no surrounding ternary.
        let src = "function f(arr) { return arr.length && arr[0].id; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_index0_with_length_and_on_other_array_issue_6227() {
        // Negative space: the left operand checks `foo.length`, but the index-0
        // access is on a DIFFERENT array `bar`, so `bar` may still be empty.
        let src = "function f(foo, bar) { return foo.length && bar[0].id; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_index0_in_left_operand_of_and_issue_6643() {
        // The index-0 access in the LEFT operand of `&&` is evaluated for truthiness
        // only — it is never dereferenced, short-circuits harmlessly to `undefined` on
        // an empty array, and its value is discarded. It is a truthiness guard (the
        // `&&` form of `if (arr[0])`), not an unchecked read, so it is exempt.
        let src = "function f(arr) { return arr[0] && arr.length; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_unguarded_index0_issue_6227() {
        // Negative space: a plain `arr[0]` with no `.length &&` guard at all.
        let src = "function f(arr) { return arr[0].id; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_optional_chained_length_ternary_consequent_issue_6228() {
        // The issue's repro: `node.typeArguments?.length === 1` guards the truthy
        // consequent, where the `?.` is normalized so `node.typeArguments?.length`
        // matches `node.typeArguments.length`. All three index-0 accesses in the
        // (nested) consequent are in-bounds.
        let src = "function f(node, ast, ts) { const type = node.typeArguments?.length === 1 ? ts.isFunctionTypeNode(node.typeArguments[0]) ? `Parameters<${getText(node.typeArguments[0], ast, ts)}>` : getText(node.typeArguments[0], ast, ts) : '[]'; return type; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_optional_chained_length_and_issue_6228() {
        // The `&&` short-circuit form with an optional-chained `.length` guard:
        // `arr?.length && arr[0]` — `?.length` is normalized to `.length`.
        let src = "function f(arr) { return arr?.length && arr[0].id; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_optional_chained_length_other_array_ternary_issue_6228() {
        // Negative space: the optional-chained `.length` guard is on `foo`, but
        // the index-0 access is on a DIFFERENT array `bar`, so `bar` may be empty.
        let src = "function f(foo, bar) { return foo?.length === 1 ? bar[0].id : null; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_optional_chained_length_other_array_and_issue_6228() {
        // Negative space (`&&` form): the optional-chained `.length` guard is on
        // `foo`, but the index-0 access is on a DIFFERENT array `bar`.
        let src = "function f(foo, bar) { return foo?.length && bar[0].id; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_assert_length_strict_equality_issue_2313() {
        // The issue's exact pattern: `assert(authors.length === 1, ...)` throws
        // unless the length is 1, so the subsequent `authors[0]` read is in-bounds.
        let src = "const authors = await em.findAll(Author); assert(authors.length === 1, `got ${authors.length}`); assert(authors[0].name === 'John', 'bad');";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_assert_length_strict_equality_multiple_accesses_issue_2313() {
        let src = "assert(arr.length === 2); arr[0]; arr[1];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_assert_length_greater_equal_one_issue_2313() {
        let src = "assert(arr.length >= 1); const first = arr[0];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_assert_length_greater_than_zero_issue_2313() {
        let src = "assert(arr.length > 0); const last = arr[arr.length - 1];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_assert_ok_length_issue_2313() {
        let src = "assert.ok(rows.length === 2); rows[0].id;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_assert_equal_length_issue_2313() {
        // `assert.equal(arr.length, N)` / `assert.strictEqual(arr.length, N)`.
        let src = "assert.strictEqual(rows.length, 1); const first = rows[0];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_index0_without_preceding_assert_issue_2313() {
        // Negative space: no preceding assertion, so the read stays flagged.
        let src = "const x = arr[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_assert_length_on_other_array_issue_2313() {
        // Negative space: the assertion is on `other`, not `arr`, so `arr` may
        // still be empty.
        let src = "assert(other.length === 2); const first = arr[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_assert_length_zero_issue_2313() {
        // Negative space: `assert(arr.length === 0)` proves the array is EMPTY,
        // so `arr[0]` is genuinely out of bounds and must stay flagged.
        let src = "assert(arr.length === 0); const first = arr[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_non_length_assert_issue_2313() {
        // Negative space: a non-length assertion says nothing about the size.
        let src = "assert(arr.includes(2)); const first = arr[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_literal_tuple_param_index0_issue_1240() {
        // A parameter annotated with a non-empty literal tuple type guarantees the
        // first element exists, so `p[0]` needs no runtime guard.
        let src = "function f(p: [number, number]) { return p[0]; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_literal_tuple_three_elements_index0_issue_1240() {
        let src = "function f(p: [number, number, number]) { return p[0]; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_literal_tuple_const_index0_issue_1240() {
        let src = "const p: [number, number] = getPair(); const x = p[0];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_literal_tuple_arrow_param_index0_issue_1240() {
        let src = "const f = (seg: [number, number]) => seg[0];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_readonly_literal_tuple_param_index0_issue_1240() {
        // `readonly [T, T]` is still a fixed-length tuple — index 0 is guaranteed.
        let src = "function f(p: readonly [number, number]) { return p[0]; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_empty_tuple_param_index0_issue_1240() {
        // Negative space: an empty tuple `[]` has no element at index 0, so the
        // read is genuinely out of bounds and stays flagged.
        let src = "function f(p: []) { return p[0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_plain_array_param_index0_issue_1240() {
        // Negative space: a plain array (variable length) is NOT a tuple, so
        // `arr[0]` may be `undefined` and stays flagged.
        let src = "function f(arr: number[]) { return arr[0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_aliased_tuple_param_index0_issue_1240() {
        // Negative space: a generic reference (`LineSegment<GlobalPoint>`, with type
        // arguments) cannot be resolved to its tuple definition without type
        // information, so it stays flagged. Only a bare alias (no type arguments) is
        // followed.
        let src = "function f(seg: LineSegment<GlobalPoint>) { return seg[0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_named_tuple_alias_param_index0_issue_6675() {
        // The issue's repro: `Semver` is a bare alias for the fixed-length tuple
        // `[number, number, number]`, so `semverA[0]` / `semverB[0]` are in-bounds.
        let src = "type Semver = [number, number, number]; const compareSemver = (semverA: Semver, semverB: Semver) => (semverA[0] - semverB[0] || semverA[1] - semverB[1] || semverA[2] - semverB[2]);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_array_alias_param_index0_issue_6675() {
        // Negative space: a bare alias to a plain array (`type Nums = number[]`) is
        // NOT a fixed-length tuple, so `a[0]` may be `undefined` and stays flagged.
        let src = "type Nums = number[]; const f = (a: Nums) => a[0] - 1;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_named_tuple_alias_chain_param_index0_issue_6675() {
        // A bounded alias chain (`type A = B; type B = [..]`) resolves to the tuple,
        // so `p[0]` is in-bounds.
        let src = "type B = [number, number]; type A = B; const f = (p: A) => p[0];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_self_referential_alias_param_index0_issue_6675() {
        // A self-referential alias (`type A = A`) is not a tuple; the cycle guard
        // must terminate the resolution (no hang) and leave the read flagged.
        let src = "type A = A; const f = (p: A) => p[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_destructured_prop_generic_interface_tuple_member_issue_7845() {
        // The issue's repro: `nouns` is destructured from a generic interface whose
        // member is a fixed 2-tuple. The tuple lives on the interface member, not on
        // the pattern annotation, and its type is independent of `T`, so the member
        // is resolved by name and `nouns[0]` is in-bounds.
        let src = "interface P<T> { nouns?: [string, string] } function f<T>({ nouns }: P<T>) { return nouns[0]; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_destructured_defaulted_prop_extends_interface_tuple_member_issue_7845() {
        // The issue's exact repro shape: a defaulted destructuring (`{ nouns = [...] }`)
        // from an interface that reaches the tuple member through `extends`. The
        // assignment-pattern default is peeled to the leaf binding and `extends`
        // heritage is followed, so `nouns[0]` is in-bounds.
        let src = "interface Base { nouns?: [string, string] } interface P<T> extends Base { bordered?: boolean } function f<T>({ nouns = ['entry', 'entries'], bordered = false }: P<T>) { return nouns[0]; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_destructured_prop_inline_type_literal_tuple_member_issue_7845() {
        // The member's tuple type is read straight off an inline type literal.
        let src = "function f({ nouns }: { nouns?: [string, string] }) { return nouns[0]; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_destructured_renamed_prop_tuple_member_issue_7845() {
        // A renamed prop (`{ nouns: n }`) is resolved by the source KEY `nouns`, so
        // the local `n` still sees the tuple member and `n[0]` is in-bounds.
        let src = "function f({ nouns: n }: { nouns?: [string, string] }) { return n[0]; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_destructured_prop_plain_array_member_issue_7845() {
        // Negative space: a plain array member (`arr?: string[]`) is variable-length,
        // not a tuple, so `arr[0]` may be `undefined` and stays flagged.
        let src = "function f({ arr }: { arr?: string[] }) { return arr[0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_destructured_prop_empty_tuple_member_issue_7845() {
        // Negative space: an empty tuple member (`x: []`) has no element at index 0,
        // so the read is genuinely out of bounds and stays flagged.
        let src = "function f({ x }: { x: [] }) { return x[0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_destructured_prop_missing_member_issue_7845() {
        // Negative space: the destructured binding has no matching member on the
        // annotated type, so its type is unresolved and the read stays flagged.
        let src = "function f({ nouns }: { other?: [string, string] }) { return nouns[0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_member_access_interface_tuple_member_issue_7878() {
        // The issue's minimal reproducer: `prop.embedded[0]` where `embedded` is a
        // fixed 2-tuple member of an interface. The indexed receiver is a member
        // access, not an identifier, so the identifier-only tuple resolvers never
        // ran; resolving `prop`'s type and looking up `embedded` proves the read
        // in-bounds.
        let src = "interface Prop { embedded?: [string, string]; } export function first(prop: Prop): string { if (!prop.embedded) { return ''; } return prop.embedded[0]; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_member_access_inline_type_literal_tuple_member_issue_7878() {
        // The container type is an inline object literal; the tuple member is read
        // straight off its signatures.
        let src = "function f(p: { embedded: [string, string] }) { return p.embedded[0]; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_member_access_type_alias_tuple_member_issue_7878() {
        // The container is a `type` alias with an object-literal body; the member's
        // tuple type is resolved by name.
        let src = "type Prop = { embedded: [string, string] }; function f(p: Prop) { return p.embedded[0]; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_member_access_plain_array_member_issue_7878() {
        // Negative space: a plain array member (`embedded?: string[]`) is
        // variable-length, so `p.embedded[0]` may be `undefined` and stays flagged.
        let src = "interface Prop { embedded?: string[]; } function f(p: Prop) { return p.embedded[0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_member_access_empty_tuple_member_issue_7878() {
        // Negative space: an empty tuple member (`embedded: []`) has no element at
        // index 0, so the read is genuinely out of bounds and stays flagged.
        let src = "interface Prop { embedded: []; } function f(p: Prop) { return p.embedded[0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_member_access_missing_member_issue_7878() {
        // Negative space: the accessed member has no declaration on the container
        // type, so its type is unresolved and the read stays flagged.
        let src = "interface Prop { other?: [string, string]; } function f(p: Prop) { return p.embedded[0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_last_index_on_tuple_issue_1240() {
        // The exemption is scoped to the first-element read. A `<obj>.length - 1`
        // last-read is not covered, so it stays flagged.
        let src = "function f(p: [number, number]) { return p[p.length - 1]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_sort_callback_tuple_element_index0_issue_6644() {
        // The issue's repro: `sections` is `Record<string, [string, string[]][]>`,
        // so `sections[group]!` is `[string, string[]][]`; the `.sort` callback
        // params `a`/`b` are inferred as the tuple `[string, string[]]`. Neither is
        // annotated, yet `a[0]`/`b[0]` are in-bounds. Both reads must be exempt.
        let src = "const sections = Object.create(null) as Record<string, [string, string[]][]>; const sortedSections = sections[group]!.sort((a, b) => a[0].localeCompare(b[0]));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_for_of_tuple_element_index0_issue_6644() {
        // `sortedSections` is an unannotated `const` whose `.sort(...)` initializer
        // preserves the `[string, string[]]` element type; the `for...of` binding
        // `section` is inferred as that tuple, so `section[0]` is in-bounds.
        let src = "const sections = Object.create(null) as Record<string, [string, string[]][]>; const sortedSections = sections[group]!.sort((a, b) => 0); for (const section of sortedSections) { const heading = section[0]; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_for_of_tuple_element_direct_array_index0_issue_6644() {
        // `for...of` over a directly tuple-array-typed binding (`[number, number][]`)
        // infers each element as `[number, number]`, so `row[0]` is in-bounds.
        let src = "function f(rows: [number, number][]) { for (const row of rows) { return row[0]; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_map_callback_tuple_element_index0_issue_6644() {
        // `.map` callback param is the receiver's element; a tuple-array receiver
        // makes the unannotated `pair[0]` in-bounds.
        let src = "function f(pairs: [string, number][]) { return pairs.map((pair) => pair[0]); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_sort_callback_string_array_element_issue_6644() {
        // Negative control: the receiver is `string[]`, so the element is a plain
        // `string`, not a tuple — `a[0]` may be `undefined` and stays flagged.
        let src = "function f(xs: string[]) { return xs.sort((a, b) => a[0].localeCompare(b[0])); }";
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn still_flags_for_of_string_array_element_issue_6644() {
        // Negative control: a `string[]` element is not a tuple, so the `for...of`
        // binding's `[0]` stays flagged.
        let src = "function f(xs: string[]) { for (const x of xs) { return x[0]; } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_sort_callback_unresolvable_receiver_issue_6644() {
        // Negative control (conservative fallback): the `.sort` receiver is an
        // untyped/unresolvable identifier, so no tuple element type can be derived
        // and the access stays flagged.
        let src = "function f(xs) { return xs.sort((a, b) => a[0] - b[0]); }";
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn still_flags_sort_callback_empty_tuple_element_issue_6644() {
        // Negative control: an empty-tuple element `[]` has no index 0, so the
        // callback param's `[0]` stays flagged.
        let src = "function f(xs: [][]) { return xs.sort((a, b) => a[0]); }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_annotated_non_tuple_array_alias_callback_issue_6644() {
        // Negative control: an aliased element type (`Pair[]`) cannot be resolved to
        // a tuple without type info, so the callback param's `[0]` stays flagged.
        let src = "function f(xs: Pair[]) { return xs.sort((a, b) => a[0]); }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_watch_array_source_tuple_param_index0_issue_7437() {
        // The issue's soybean-admin repro: `val` is the callback parameter of a Vue
        // `watch` whose source is the array literal `[grayscaleMode,
        // colourWeaknessMode]`, so Vue types it as the fixed-length tuple
        // `[boolean, boolean]` — `val[0]` is always in-bounds (`val[1]` is a
        // non-boundary index the rule never inspects).
        let src = "watch([grayscaleMode, colourWeaknessMode], val => { toggleAuxiliaryColorModes(val[0], val[1]); }, { immediate: true });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_watch_array_source_three_tuple_issue_7437() {
        // A three-element array source ⇒ three-tuple parameter; the boundary read
        // `val[0]` (index 0 < 3) is in-bounds and exempt, and `val[2]` is a
        // non-boundary index the rule never inspects — neither is flagged.
        let src = "watch([a, b, c], val => { use(val[0], val[2]); });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_watch_non_array_literal_source_issue_7437() {
        // Negative control: the `watch` source is a single ref, not an array literal,
        // so `val` is not a fixed-length tuple — `val[0]` stays flagged.
        let src = "watch(singleRef, val => { use(val[0]); });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_watch_callback_not_second_arg_issue_7437() {
        // Negative control: the callback is the third argument, not the second, so
        // it is not the source callback of `watch([sources], cb)` — `val[0]` stays
        // flagged (guards the `callback_is_second_arg` span check).
        let src = "watch([a, b], other, val => { use(val[0]); });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_watch_empty_array_source_issue_7437() {
        // Negative control: an empty-array source `[]` is a zero-length tuple with no
        // index 0, so `val[0]` stays flagged.
        let src = "watch([], val => { use(val[0]); });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_plain_dynamic_array_index0_issue_7437() {
        // Negative control: a plain dynamic-array access whose receiver is not a
        // watch tuple param stays flagged.
        let src = "const arr = getArr(); const x = arr[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_call_return_tuple_arrow_index0_issue_7148() {
        // The issue's hono repro: `skipResult` is an unannotated `const` bound to a
        // call to a same-file arrow with a `[number, boolean]` return type, so
        // `skipResult[0]` / `skipResult[1]` are in-bounds.
        let src = "const skipInvalidParam = (h: string, i: number): [number, boolean] => [i + 1, true]; const skipResult = skipInvalidParam(h, i); use(skipResult[0], skipResult[1]);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_call_return_tuple_union_undefined_index0_issue_7148() {
        // The issue's hono `getEventSpec` repro: a `[string, boolean] | undefined`
        // return type qualifies through its non-nullish member, so `eventSpec[0]` is
        // in-bounds. The `if (eventSpec)` mirrors the hono source (the exemption
        // relies on the non-nullish tuple member, not the guard).
        let src = "const getEventSpec = (k: string): [string, boolean] | undefined => undefined; const eventSpec = getEventSpec(k); if (eventSpec) { use(eventSpec[0], eventSpec[1]); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_call_return_union_non_tuple_member_index0_issue_7148() {
        // Negative control: a union with a non-tuple non-nullish member
        // (`[number, boolean] | number[]`) is not provably a tuple — the `number[]`
        // branch may be empty — so `y[0]` stays flagged.
        let src = "const f = (): [number, boolean] | number[] => []; const y = f(); use(y[0]);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_call_return_only_nullish_union_index0_issue_7148() {
        // Negative control: a union with no non-nullish member (`undefined | null`)
        // yields no tuple evidence, so `y[0]` stays flagged.
        let src = "const f = (): undefined | null => undefined; const y = f(); use(y[0]);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_call_return_tuple_let_binding_index0_issue_7148() {
        // Negative control: a `let` receiver may be reassigned to a shorter or
        // differently-typed value, so the tuple inference does not hold and `y[0]`
        // stays flagged.
        let src = "const f = (): [number, boolean] => [1, true]; let y = f(); use(y[0]);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_call_return_tuple_function_expression_index0_issue_7148() {
        // A `const`-bound function expression callee with a tuple return type
        // resolves the same way as the arrow and `function`-declaration forms.
        let src = "const pair = function (): [number, boolean] { return [1, true]; }; const r = pair(); use(r[0]);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_call_return_tuple_function_decl_index0_issue_7148() {
        // A `function` declaration callee with a tuple return type resolves the same
        // way as the arrow form.
        let src = "function pair(): [number, boolean] { return [1, true]; } const r = pair(); use(r[0]);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_call_return_array_index0_issue_7148() {
        // Negative control: a callee returning a plain array (`number[]`, not a
        // tuple) may yield an empty array, so `x[0]` stays flagged.
        let src = "const someFn = (): number[] => []; const x = someFn(); use(x[0]);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_call_return_unannotated_callee_index0_issue_7148() {
        // Negative control: a callee with NO return-type annotation gives no tuple
        // evidence, so `x[0]` stays flagged.
        let src = "const someFn = () => makeArr(); const x = someFn(); use(x[0]);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_call_return_unresolved_callee_index0_issue_7148() {
        // Negative control: an imported / non-same-file callee has no in-file
        // declaration to read a return type from, so `x[0]` stays flagged.
        let src = "import { fetchPair } from './p'; const x = fetchPair(); use(x[0]);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_annotated_array_binding_over_tuple_call_index0_issue_7148() {
        // Negative control: an explicit `number[]` annotation overrides the call's
        // inferred tuple type, so the binding is a plain array and `x[0]` stays
        // flagged — the inferred-type path only applies to unannotated bindings.
        let src = "const pair = (): [number, boolean] => [1, true]; const x: number[] = pair(); use(x[0]);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_typed_array_const_literal_length_index0_issue_2127() {
        // The issue's Web Crypto nonce idiom: `new Uint32Array(1)` allocates one
        // slot, so the subsequent `array[0]` read is in-bounds.
        let src = "function f() { const array = new Uint32Array(1); window.crypto.getRandomValues(array); return array[0].toString(); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_typed_array_const_element_list_index0_issue_2127() {
        // `new Uint32Array([hash])` builds a single-element array; `[0]` is in-bounds.
        let src = "const a = new Uint32Array([hash]); const s = a[0].toString(36);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_array_const_literal_length_index0_issue_2127() {
        // `new Array(N)` with `N >= 1` has a known length.
        let src = "const a = new Array(3); const x = a[0];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_typed_array_dynamic_length_index0_issue_2127() {
        // Negative space: a non-constant length leaves the size unknown — for `n`
        // of 0 the array is empty, so `[0]` stays flagged.
        let src = "function f(n) { const a = new Uint32Array(n); return a[0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_typed_array_zero_length_index0_issue_2127() {
        // Negative space: `new Uint32Array(0)` is empty, so `[0]` is out of bounds.
        let src = "const a = new Uint32Array(0); const x = a[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_typed_array_reassigned_let_index0_issue_2127() {
        // Negative space: a `let` may be reassigned to a shorter array, so the
        // fixed-size construction no longer proves the length at the read site.
        let src = "let a = new Uint32Array(1); a = new Uint32Array(0); const x = a[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_non_array_constructor_index0_issue_2127() {
        // Negative space: `new Foo(1)` is not a known fixed-size array, so `[0]`
        // says nothing about bounds and stays flagged.
        let src = "const a = new Foo(1); const x = a[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_split_result_index0_issue_2128() {
        // The issue's exact pattern: a `const` bound to a `String.split()` result.
        // `split()` always returns an array with at least one element, so `[0]` is
        // always in-bounds.
        let src = "const parts = pathname.split('/'); const firstPart = parts[0];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_split_result_last_index_issue_2128() {
        // The String.split contract guarantees `length >= 1`, so the last-element
        // read `parts[parts.length - 1]` is also in-bounds.
        let src = "const segments = noExtension.split('/'); const last = segments[segments.length - 1];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_split_result_member_receiver_issue_2128() {
        // The split receiver may itself be a member chain (`this.name.split`).
        let src = "const parts = this.name.split('.'); const ext = parts[parts.length - 1];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_split_no_args_index0_issue_2128() {
        // `split()` with no argument still returns `[wholeString]` — non-empty.
        let src = "const parts = s.split(); const first = parts[0];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_arbitrary_call_init_index0_issue_2128() {
        // Negative space: a non-`split` call initializer leaves emptiness unknown,
        // so `parts[0]` stays flagged.
        let src = "const parts = getParts(); const x = parts[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_split_reassigned_let_index0_issue_2128() {
        // Negative space: a `let` may be reassigned to a non-split value, so the
        // split contract no longer proves non-emptiness at the read site.
        let src = "let parts = s.split(','); parts = []; const x = parts[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_split_result_as_tuple_assertion_issue_6493() {
        // unjs/pathe repro: the `.split()` initializer is wrapped in a transparent
        // `as [string, ...string[]]` assertion. The assertion is compile-time-only,
        // so the binding still holds the (non-empty) split result and `_from[0]` is
        // in-bounds.
        let src = "const _from = resolve(from).replace(re, '$1').split('/') as [string, ...string[]]; const c = _from[0][1];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_split_result_legacy_type_assertion_issue_6493() {
        // The legacy `<T>expr` assertion form is equally transparent.
        let src = "const parts = <[string, ...string[]]>s.split('/'); const c = parts[0];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_non_split_call_wrapped_in_as_issue_6493() {
        // Negative space: peeling the `as` wrapper must not turn a non-`split` call
        // into a split exemption — `getParts()` provides no non-emptiness guarantee,
        // so `parts[0]` stays flagged.
        let src = "const parts = getParts() as string[]; const x = parts[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_result_binding_early_exit_not_first_issue_2132() {
        // The issue's InfiniteList example: `const lastItem = items[items.length - 1]`
        // followed by `if (!lastItem) return`. The early exit handles the
        // out-of-bounds `undefined`, so the last-element read is defensive.
        let src = "function f(virtualItems) { const lastItem = virtualItems[virtualItems.length - 1]; if (!lastItem) return; if (lastItem.index >= 0) onLoadNextPage(); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_result_binding_early_exit_first_issue_2132() {
        let src = "function f(items) { const first = items[0]; if (!first) return; use(first); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_result_binding_early_exit_loose_null_issue_2132() {
        let src = "function f(arr) { const x = arr[0]; if (x == null) return; use(x); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_result_binding_early_exit_strict_undefined_issue_2132() {
        let src = "function f(arr) { const x = arr[0]; if (x === undefined) return; use(x); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_result_binding_early_exit_throw_issue_2132() {
        let src = "function f(arr) { const x = arr[0]; if (!x) throw new Error('empty'); return x.id; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_result_binding_truthy_and_narrowing_issue_2132() {
        // The issue's AIAssistant example: `if (lastMessage && lastMessage.role ===
        // 'assistant') { state.updateMessage(lastMessage) }`. Every use of
        // `lastMessage` is inside the truthy-narrowed branch.
        let src = "function f(chatMessages, state) { const lastMessage = chatMessages[chatMessages.length - 1]; if (lastMessage && lastMessage.role === 'assistant') { state.updateMessage(lastMessage); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_result_binding_if_truthy_block_issue_2132() {
        let src = "function f(arr) { const x = arr[0]; if (x) { return x.name; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_result_binding_optional_chain_use_issue_2132() {
        let src = "function f(arr) { const x = arr[0]; return x?.foo; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_result_binding_nullish_fallback_use_issue_2132() {
        let src = "function f(arr) { const x = arr[0]; return x ?? defaultValue; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_result_binding_logical_or_fallback_use_issue_2132() {
        let src = "function f(arr) { const x = arr[0]; return x || defaultValue; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_result_binding_logical_and_use_issue_2132() {
        let src = "function f(arr) { const x = arr[0]; return x && x.foo; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_result_binding_unguarded_use_issue_2132() {
        // Negative space: the binding is used without any null guard, so the
        // out-of-bounds `undefined` is dereferenced — still a true positive.
        let src = "function f(arr) { const x = arr[0]; use(x); }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_result_binding_member_use_issue_2132() {
        // Negative space: `x.foo` dereferences a possibly-`undefined` element with
        // no guard, so it stays flagged.
        let src = "function f(arr) { const x = arr[0]; return x.foo; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_result_binding_use_before_guard_issue_2132() {
        // Negative space: the unguarded `use(x)` runs before the `if (!x)` guard,
        // so the early read is not vouched safe.
        let src = "function f(arr) { const x = arr[0]; use(x); if (!x) return; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_result_binding_guard_on_other_var_issue_2132() {
        // Negative space: the early-exit guard is on `y`, not `x`, so `x` is still
        // an unguarded out-of-bounds read.
        let src = "function f(arr, y) { const x = arr[0]; if (!y) return; return x.foo; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_result_binding_use_outside_narrowed_branch_issue_2132() {
        // Negative space: `x` is read in the `else` branch, where the truthy
        // narrowing does not hold, so it can be `undefined`.
        let src = "function f(arr) { const x = arr[0]; if (x) { use(x); } else { return x.foo; } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_result_binding_var_declaration_issue_2132() {
        // Negative space: a `var` is function-scoped and may be reassigned anywhere
        // in the function, so the binding-level reasoning does not apply.
        let src = "function f(arr) { var x = arr[0]; if (!x) return; use(x); }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_first_index_not_a_binding_initializer_issue_2132() {
        // Negative space: the access is a bare expression statement, not a
        // `const`/`let` initializer, so the result-binding exemption never applies.
        let src = "function f(arr) { arr[0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_string_last_char_after_truthy_exit_issue_1337() {
        // The issue's exact shape: a `string | undefined` param guarded by
        // `if (!word) return` is non-empty at `word[word.length - 1]`, since an
        // empty string is falsy.
        let src = "function withDefiniteArticle(word: string | undefined): string {\n  if (!word) return \"\";\n  const vowels = [\"a\"];\n  const lastChar = word[word.length - 1];\n  return word + (vowels.includes(lastChar) ? \"x\" : \"y\");\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_string_first_char_after_truthy_exit_issue_1337() {
        // The first-element read is in-bounds under the same truthy-string guard.
        let src = "function f(s: string) { if (!s) throw new Error(); const c = s[0]; return c; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_array_last_after_truthy_exit_issue_1337() {
        // Negative space: an array is truthy even when empty (`[]`), so a
        // truthiness guard does not prove non-emptiness — the read stays flagged.
        let src = "function f(arr: number[]) { if (!arr) return; const x = arr[arr.length - 1]; return x; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_string_last_without_truthy_exit_issue_1337() {
        // Negative space: no preceding `if (!s)` guard, so the string may be empty.
        let src = "function f(s: string) { const c = s[s.length - 1]; return c; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_string_last_char_after_compound_or_exit_issue_4430() {
        // The issue's exact shape: `if (!base || base === "/") return ...` excludes
        // both empty/undefined and the single-char "/" string, so `base` has length
        // >= 2 at the last-element read. The left arm `!base` is a nullish check, so
        // the compound `||` guard is recognized.
        let src = "function joinURL(base?: string, path?: string): string {\n  if (!base || base === \"/\") {\n    return path || \"/\";\n  }\n  if (!path || path === \"/\") {\n    return base || \"/\";\n  }\n  const baseHasTrailing = base[base.length - 1] === \"/\";\n  const pathHasLeading = path[0] === \"/\";\n  return baseHasTrailing ? base + path : base + \"/\" + path;\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_string_index_after_compound_or_right_arm_issue_4430() {
        // Right-arm recursion: the nullish check sits on the right of the `||`
        // (`other || !base`), still proving `base` non-nullish on fall-through.
        let src = "function f(base: string, other: boolean) { if (other || !base) { return \"\"; } const c = base[base.length - 1]; return c; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_string_last_after_compound_and_guard_issue_4430() {
        // Load-bearing negative: under `&&`, fall-through is `!base || !other`, which
        // does not prove `base` is non-nullish even though one arm is `!base`. The
        // `&&` guard must not be recognized, so the read stays flagged.
        let src = "function f(base: string, other: boolean) { if (!base && other) { return \"\"; } const c = base[base.length - 1]; return c; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_string_index0_in_truthy_ternary_consequent_issue_5254() {
        // The issue's exact shape (unjs/scule): `str ? str[0].toUpperCase() +
        // str.slice(1) : ""`. The ternary tests `str` for truthiness — an empty
        // string is falsy — so in the consequent `str` is a non-empty string and
        // `str[0]` is in-bounds. `str: S` (generic `S extends string`) has no plain
        // `string` annotation, so the string-method call on `str` supplies the
        // evidence.
        let src = "function upperFirst<S extends string>(str: S): Capitalize<S> {\n  return (str ? str[0].toUpperCase() + str.slice(1) : \"\") as Capitalize<S>;\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_string_index0_in_truthy_ternary_annotated_issue_5254() {
        // A plain `: string` annotation alone supplies the string evidence under the
        // same-variable truthy ternary guard — no method call needed.
        let src = "function f(str: string) { return str ? str[0] : \"\"; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_string_index0_in_truthy_logical_and_issue_5254() {
        // The logical-and form: `str && str[0].toUpperCase()` accesses `str[0]` only
        // when `str` is truthy (non-empty string).
        let src = "function f<S extends string>(str: S) { return str && str[0].toUpperCase(); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_string_index0_in_truthy_if_block_issue_5254() {
        // The `if (str) { … }` form: inside the truthy block `str` is a non-empty
        // string, proven by the `.charAt`-style method (here `.toUpperCase`).
        let src = "function f<S extends string>(str: S) { if (str) { return str[0].toUpperCase(); } return \"\"; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_array_index0_in_truthy_ternary_issue_5254() {
        // Load-bearing negative: an empty array is truthy (`[]`), so `arr ? arr[0] :
        // null` does NOT prove non-emptiness — there is no string evidence, so the
        // boundary read stays flagged.
        let src = "function f(arr: number[]) { return arr ? arr[0] : null; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_untyped_index0_in_truthy_ternary_no_string_evidence_issue_5254() {
        // No `string` annotation and no string-exclusive method on the variable: the
        // truthy guard could be on an array, so the read stays flagged. `.slice` is
        // shared with arrays and is deliberately not counted as string evidence.
        let src = "function f(x) { return x ? x[0].slice(1) : null; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_string_index0_in_ternary_alternate_branch_issue_5254() {
        // The alternate (falsy) branch runs when `str` is empty, so a boundary access
        // there is genuinely unsafe and stays flagged.
        let src = "function f(str: string) { return str ? str.toUpperCase() : str[0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    // Regression #4189: first/last-element access is an idiom in test files —
    // an empty array makes `arr[0]` `undefined`, which fails the assertion (the
    // test doing its job), not a shipped production bug. The rule is gated out of
    // test dirs via the central `skip_in_test_dir` mechanism.
    #[test]
    fn skips_first_element_access_in_test_dir_issue_4189() {
        let src = r#"const x = expect(withYears.result.current[0].filters["years"]).toEqual(["2024","2025"]);"#;
        assert!(
            crate::rules::test_helpers::run_rule_gated(
                &Check,
                src,
                "src/app/hooks/use-list-search-sync.test.ts",
            )
            .is_empty()
        );
    }

    #[test]
    fn skips_bare_first_access_in_test_dir_issue_4189() {
        let src = "const first = arr[0];";
        assert!(
            crate::rules::test_helpers::run_rule_gated(
                &Check,
                src,
                "src/api/features/imports/process.integration.test.ts",
            )
            .is_empty()
        );
    }

    #[test]
    fn still_flags_first_access_in_production_file_issue_4189() {
        // The same unguarded access in a non-test path stays flagged — only test
        // files are exempt, production code is unchanged.
        let src = "const first = arr[0];";
        assert_eq!(
            crate::rules::test_helpers::run_rule_gated(&Check, src, "src/api/feature.ts").len(),
            1
        );
    }

    #[test]
    fn no_fp_typeof_index0_type_guard_issue_5302() {
        // The issue's `is-bezier-definition.ts` shape: `typeof easing[0] === "number"`
        // is a type-narrowing guard. On an empty array `easing[0]` is `undefined`
        // and `typeof undefined` is `"undefined"` (never throws), so the guard
        // simply evaluates false — no boundary violation.
        let src = "function f(easing) { return Array.isArray(easing) && typeof easing[0] === 'number'; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_typeof_index0_not_equal_issue_5302() {
        // The issue's `is-easing-array.ts` shape: `typeof ease[0] !== "number"`.
        let src = "function f(ease) { return Array.isArray(ease) && typeof ease[0] !== 'number'; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_typeof_parenthesized_index0_issue_5302() {
        // A parenthesized operand is still a `typeof` operand: `typeof (arr[0])`
        // is identical to `typeof arr[0]`.
        let src = "function f(arr) { return typeof (arr[0]) === 'number'; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_typeof_computed_member_issue_5302() {
        // `typeof obj[key]` — a computed (non-zero-literal) access is also safe as
        // a `typeof` operand.
        let src = "function f(obj, key) { return typeof obj[key] === 'string'; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_index0_equality_operand_against_typeof_issue_6231() {
        // `typeof x === arr[0]` — `arr[0]` is the direct right operand of `===`,
        // and the other operand (`typeof x`) is a non-nullish string, so the
        // equality-operand exemption applies: `<string> === undefined` is `false`
        // and never throws. (`arr[0]` is correctly NOT a `typeof` operand here, the
        // precision of `is_typeof_operand` checked by the `arr[0].length` test
        // below.) Issue #6231.
        let src = "function f(arr) { return typeof x === arr[0]; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_member_on_typeof_index0_issue_5302() {
        // Negative space: `typeof arr[0].length` — `typeof`'s operand is
        // `arr[0].length`, not `arr[0]`. The inner `arr[0]` is read as a value
        // (its `.length` is accessed), so an empty array would throw. Still flagged.
        let src = "function f(arr) { return typeof arr[0].length === 'number'; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_unguarded_last_index_value_use_issue_5302() {
        // Negative space: an unguarded last-element read used as a value stays
        // flagged — the `typeof` exemption is strictly about the operand position.
        let src = "function f(arr) { return arr[arr.length - 1].id; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_object_entries_map_callback_issue_6297() {
        let src = "const _filters = Object.entries(filters || {}).map(e => `${e[0]}(${e[1]})`);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_object_entries_for_of_issue_6297() {
        let src = "for (const e of Object.entries(data)) { const dep = { name: e[0], range: e[1] }; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_object_entries_array_literal_callback_issue_6297() {
        let src = "Object.entries(options.alias).map(e => [e[0], e[1]]);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_ordinary_array_first_access_issue_6297() {
        // Negative control: no `Object.entries` provenance — an ordinary array's
        // first-element read stays flagged.
        let src = "const arr = getArr(); const x = arr[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_object_keys_for_of_first_access_issue_6297() {
        // `Object.keys` elements are `string`, not tuples — `k[0]` is a character
        // index that may be out of bounds (e.g. an empty-string key). Stays flagged.
        let src = "for (const k of Object.keys(data)) { const c = k[0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_object_values_map_callback_first_access_issue_6297() {
        // `Object.values` elements are scalar `T`, not tuples — the callback-path
        // exemption must reject a non-`entries` receiver. Stays flagged.
        let src = "Object.values(o).map(e => e[0]);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_map_entries_spread_sort_comparator_issue_7697() {
        // The issue's first repro: `[...map.entries()].sort((a, b) => …)`. Both
        // comparator params are `[K, V]` tuples, so `a[0]`/`b[0]` are in-bounds.
        let src = "const classTotals = new Map<string, number>(); const top = [...classTotals.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_map_entries_array_from_sort_class_property_issue_7697() {
        // The issue's second repro: `Array.from(this.<map>.entries()).sort(...)`
        // where the receiver is a class property initialized `new Map(...)`.
        let src = "class C { private readonly citations = new Map<number, string>(); m() { return Array.from(this.citations.entries()).sort((a, b) => a[0] - b[0]); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_set_entries_annotated_receiver_sort_issue_7697() {
        // A `Set<...>`-annotated receiver: `.entries()` yields `[T, T]` tuples.
        let src = "const s: Set<string> = new Set(); const r = [...s.entries()].sort((a, b) => a[0].localeCompare(b[0]));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_map_entries_direct_chain_sort_issue_7697() {
        // Chaining `.entries()` directly on a `new Map(...)` construction.
        let src = "const r = [...new Map<string, number>().entries()].sort((a, b) => a[0].localeCompare(b[0]));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_sort_comparator_array_of_arrays_issue_7697() {
        // Negative control: the receiver is a genuine array of arrays of unknown
        // length (`number[][]`), not a Map/Set entries source, so its elements are
        // `number[]` (possibly empty) — `a[0]`/`b[0]` stay flagged.
        let src = "function f(xs: number[][]) { return xs.sort((a, b) => a[0] - b[0]); }";
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn still_flags_untyped_entries_sort_comparator_issue_7697() {
        // Negative control: `foo` does not resolve to a Map/Set, so `foo.entries()`
        // is an arbitrary user method with no tuple guarantee — stays flagged.
        let src = "function f(foo) { return foo.entries().sort((a, b) => a[0] - b[0]); }";
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn no_fp_while_length_gt_zero_drain_issue_6510() {
        // The issue's exact repro: a work-list drain loop whose `while` condition
        // proves the array is non-empty before each `promises[0]` read.
        let src = "while (promises.length > 0) { const x = await promises[0]; promises.shift(); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_while_bare_length_drain_issue_6510() {
        // `while (arr.length)` — a bare truthy length is non-zero, hence non-empty.
        let src = "while (queue.length) { handle(queue[0]); queue.shift(); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_while_length_ge_one_issue_6510() {
        let src = "while (items.length >= 1) { use(items[0]); items.pop(); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_while_length_ne_zero_issue_6510() {
        let src = "while (items.length !== 0) { use(items[0]); items.shift(); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_while_zero_lt_length_mirrored_issue_6510() {
        let src = "while (0 < stack.length) { peek(stack[0]); stack.shift(); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_while_length_gt_zero_last_element_issue_6510() {
        // The issue covers `arr[arr.length - 1]`: a `length > 0` guard proves the
        // last element exists too.
        let src = "while (stack.length > 0) { const top = stack[stack.length - 1]; stack.pop(); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_for_length_gt_zero_drain_issue_6510() {
        // A `for` loop whose test proves non-emptiness is analogous to `while`.
        let src = "for (; queue.length > 0; ) { const x = queue[0]; queue.shift(); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_unguarded_first_access_issue_6510() {
        // Negative control: an unguarded `arr[0]` with no dominating length guard
        // stays flagged.
        let src = "function f(arr) { const x = arr[0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_while_length_guard_on_different_array_issue_6510() {
        // Negative control: the non-empty check is on `other`, not the indexed
        // `arr` — it does not prove `arr` non-empty, so `arr[0]` stays flagged.
        let src = "while (other.length > 0) { const x = arr[0]; other.shift(); }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_do_while_length_guard_issue_6510() {
        // Negative control: a `do…while` body runs once before the test, so the
        // `length > 0` condition does not dominate the first iteration's read.
        let src = "do { const x = arr[0]; arr.shift(); } while (arr.length > 0);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_index0_guarded_by_and_short_circuit_issue_6643() {
        // unjs/automd `jsimport.ts`: `(importNames[0] && ` ${importNames[0]} `) || ""`.
        // The LEFT `importNames[0]` is evaluated for truthiness only — an empty array
        // yields `undefined`, the `&&` short-circuits, and the outer `|| ""` returns
        // `""`. The RIGHT `importNames[0]` runs only when the LEFT was truthy, so it is
        // present. Neither use is flagged.
        let src = "function f(importNames: string[]) { return (importNames[0] && ` ${importNames[0]} `) || \"\"; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_index0_as_left_of_and_guard_issue_6643() {
        // `arr[0]` as the LEFT operand of `&&` is a truthiness guard (the `&&` form of
        // `if (arr[0])`); the RIGHT `arr[0].id` runs only after that truthy test. Both
        // reads are exempt.
        let src = "function f(arr: number[]) { return arr[0] && arr[0].id; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_index0_in_right_of_and_with_different_array_left_issue_6643() {
        // The LEFT `&&` operand tests a DIFFERENT array (`other[0]`), so it does not
        // prove `arr` non-empty — the RIGHT `arr[0].id` stays flagged. (`other[0]` is
        // itself a truthiness guard and is exempt, so exactly one diagnostic remains.)
        let src = "function f(arr: number[], other: number[]) { return other[0] && arr[0].id; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_index0_in_right_of_or_issue_6643() {
        // `||` short-circuits on a FALSY left, so the RIGHT operand runs only when
        // `arr[0]` was absent/falsy — it is NOT guarded. The LEFT `arr[0]` is exempt
        // via its `|| fallback`, leaving exactly one diagnostic on the dereferenced
        // RIGHT `arr[0].id`.
        let src = "function f(arr: number[]) { return arr[0] || arr[0].id; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_index0_in_right_of_nullish_coalesce_issue_6643() {
        // `??` short-circuits on a nullish left, so the RIGHT operand is the fallback
        // path and is not guarded. The LEFT `arr[0]` is exempt via its `?? fallback`,
        // leaving exactly one diagnostic on the RIGHT `arr[0].id`.
        let src = "function f(arr: number[]) { return arr[0] ?? arr[0].id; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_bare_index0_member_access_issue_6643() {
        // No `&&` guard at all: a bare `arr[0].foo` dereferences the first element and
        // stays flagged.
        let src = "function f(arr: number[]) { const x = arr[0].foo; return x; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_ensure_nonempty_assignment_guard_issue_6648() {
        // The issue's exact shape: a `!base || base.length === 0` guard whose
        // consequent assigns a non-empty array literal makes `base[0]` in-bounds.
        let src = "function f(options) { if (!options.domains || options.domains.length === 0) { options.domains = [\"localhost.local\"]; } const x = options.domains[0]; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_ensure_nonempty_bang_identifier_guard_issue_6648() {
        // Bare `!arr` test on an identifier base with a non-empty literal assignment.
        let src = "function f(arr) { if (!arr) { arr = [1]; } const x = arr[0]; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_ensure_nonempty_length_only_guard_issue_6648() {
        // A `length === 0` test alone (no nullish disjunct) plus a non-empty literal.
        let src = "function f(arr) { if (arr.length === 0) arr = [1, 2]; const x = arr[arr.length - 1]; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_ensure_guard_assigns_empty_literal_issue_6648() {
        // The consequent assigns an EMPTY array literal, so the array is not
        // proven non-empty and the read stays flagged.
        let src = "function f(arr) { if (!arr || arr.length === 0) { arr = []; } const x = arr[0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_ensure_guard_different_base_issue_6648() {
        // The guard ensures `options.domains` is non-empty, but the read is on a
        // DIFFERENT base (`options.commonName`), so it stays flagged.
        let src = "function f(options) { if (!options.domains || options.domains.length === 0) { options.domains = [\"d\"]; } const x = options.commonName[0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_ensure_guard_assigns_non_literal_issue_6648() {
        // The consequent assigns an unknown non-literal value (a call result),
        // which cannot be proven non-empty, so the read stays flagged.
        let src = "function f(arr) { if (!arr || arr.length === 0) { arr = getDefaults(); } const x = arr[0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_index0_with_no_ensure_guard_issue_6648() {
        // No preceding ensure-non-empty guard at all.
        let src = "function f(options) { const x = options.domains[0]; return x; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_ensure_guard_with_and_test_issue_6648() {
        // An `&&` test is not a sound ensure-non-empty guard: on fall-through
        // (`!A || !B`) the empty-check arm may still be false (`cond` false,
        // `arr` empty), so the body did not run and `arr[0]` is out-of-bounds.
        let src = "function f(arr, cond) { if (cond && arr.length === 0) { arr = [1]; } const x = arr[0]; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_index0_strict_equality_string_issue_6231() {
        // nodejs/undici `isHttpOrHttpsPrefixed`: `value[0] === 'h'`. An empty
        // string makes `value[0]` `undefined`, and `undefined === 'h'` is `false`
        // (never throws) — the comparison is the guard. Issue #6231.
        let src = "function f(value) { return value[0] === 'h'; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_index0_strict_inequality_string_issue_6231() {
        // nodejs/undici `util.js`: `path[0] !== '/'`. `undefined !== '/'` is
        // `true`, the correct "not absolute" result. Issue #6231.
        let src = "function f(path) { return path[0] !== '/'; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_arguments_index0_inequality_sentinel_issue_6231() {
        // nodejs/undici cache constructor guard: `arguments[0] !== kConstruct`.
        // The sentinel `kConstruct` is a non-nullish identifier. Issue #6231.
        let src = "function f() { return arguments[0] !== kConstruct; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_parenthesized_index0_equality_issue_6231() {
        // A parenthesized operand is still the direct operand: `(value[0]) === 'h'`
        // is identical to `value[0] === 'h'`. Issue #6231.
        let src = "function f(value) { return (value[0]) === 'h'; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_bare_index0_outside_comparison_issue_6231() {
        // Negative space: a bare first-element read with no equality comparison
        // around it stays flagged. Issue #6231.
        let src = "function f(arr) { const x = arr[0]; return x; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_member_on_index0_in_comparison_issue_6231() {
        // Negative space: `arr[0].foo === 'h'` — the computed access is the OBJECT
        // of a member access, not the direct comparison operand. Reading
        // `undefined.foo` on an empty array throws, so it stays flagged. Issue #6231.
        let src = "function f(arr) { return arr[0].foo === 'h'; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_index0_equality_against_undefined_issue_6231() {
        // Negative space: `arr[0] === undefined` is a deliberate emptiness check,
        // not a comparison against a concrete value — the other operand is the
        // `undefined` identifier, so it is excluded and stays flagged. Issue #6231.
        let src = "function f(arr) { return arr[0] === undefined; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_index0_loose_equality_against_null_issue_6231() {
        // Negative space: `arr[0] == null` is a deliberate nullish check (the other
        // operand is a `NullLiteral`), so it is excluded and stays flagged. Issue #6231.
        let src = "function f(arr) { return arr[0] == null; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_index0_loose_equality_string_issue_6231() {
        // Loose `==` against a non-nullish literal is exempt for the same reason as
        // strict `===`: `undefined == 'h'` is `false` and never throws. Issue #6231.
        let src = "function f(value) { return value[0] == 'h'; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_typeof_index0_equality_guard_issue_7106() {
        // drizzle's overloaded-rest-param dispatch: `if (typeof params[0] === 'string')`
        // guards the body — an empty `params` makes `typeof undefined === 'string'`
        // false, so `params[0]` in the block is a defined string. The comparison
        // condition references `params[0]`, so the body read is in-bounds.
        let src = "function drizzle(...params) { if (typeof params[0] === 'string') { return new Pool({ connectionString: params[0] }); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_index0_equality_guard_body_issue_7106() {
        // `if (arr[0] === 'x')` narrows the branch to a present first element, so a
        // same-array `arr[0]` read in the body is in-bounds.
        let src = "function f(arr) { if (arr[0] === 'x') { use(arr[0]); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_index0_comparison_and_guard_issue_7106() {
        // A non-equality comparison operand also guards: `arr[0] > 5 && cond` reaches
        // `arr[0]` through the `&&` into the binary comparison, so the body read is
        // in-bounds.
        let src = "function f(arr, cond) { if (arr[0] > 5 && cond) { use(arr[0]); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_index0_binary_condition_not_referencing_it_issue_7106() {
        // A comparison whose operands do NOT reference `arr[0]` (`flag === 1`) is not
        // a guard for `arr[0]`, so the body read stays flagged.
        let src = "function f(arr, flag) { if (flag === 1) { use(arr[0]); } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_index0_inequality_guard_body_issue_7106() {
        // `if (arr[0] !== 'x')` does NOT prove presence: `undefined !== 'x'` is `true`,
        // so the branch runs for an absent element and the body read stays flagged.
        let src = "function f(arr) { if (arr[0] !== 'x') { use(arr[0]); } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_index0_equality_against_undefined_guard_body_issue_7106() {
        // `if (arr[0] === undefined)` is an emptiness check, not a presence guard, so
        // it does not exempt the body read. Both `arr[0]` accesses stay flagged: the
        // condition's own read (equality against `undefined` is not a value guard) and
        // the body read (the branch runs exactly when the element is absent).
        let src = "function f(arr) { if (arr[0] === undefined) { use(arr[0]); } }";
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn no_fp_index0_dominated_by_some_guard_issue_7479() {
        // fantastic-admin `getDeepestPath`: `menu.children[0]` in the `else` branch is
        // dominated by `if (menu.children?.some(...))`, which proves `menu.children`
        // non-empty (`some` is `false` for `[]`), so the first element exists.
        let src = "function getDeepestPath(menu, rootPath = '') { let retnPath = ''; if (menu.children?.some(item => item.meta?.menu !== false)) { const item = menu.children.find(item => item.meta?.menu !== false); if (item) { retnPath = getDeepestPath(item, resolveRoutePath(rootPath, menu.path)); } else { retnPath = getDeepestPath(menu.children[0], resolveRoutePath(rootPath, menu.path)); } } return retnPath; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_index0_plain_some_guard_issue_7479() {
        // `if (arr.some(...))` (no optional chaining) proves `arr` non-empty.
        let src = "function f(arr) { if (arr.some(x => x > 0)) { use(arr[0]); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_index0_optional_some_guard_issue_7479() {
        // `if (arr?.some(...))` — the `?.` guards `undefined`; a truthy result still
        // proves non-emptiness.
        let src = "function f(arr) { if (arr?.some(x => x > 0)) { use(arr[0]); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_last_index_dominated_by_some_guard_issue_7479() {
        // Non-emptiness proves the last element exists too: `arr[arr.length - 1]`.
        let src = "function f(arr) { if (arr.some(x => x > 0)) { use(arr[arr.length - 1]); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_while_some_guard_issue_7479() {
        // The same non-emptiness oracle applies to a `while (arr.some(...))` drain.
        let src = "function f(arr) { while (arr.some(x => x > 0)) { use(arr[0]); arr.shift(); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_index0_some_guard_on_different_array_issue_7479() {
        // Negative control: `some` is called on `other`, not the indexed `arr` — it
        // proves `other` non-empty, not `arr`, so `arr[0]` stays flagged.
        let src = "function f(arr, other) { if (other.some(x => x > 0)) { use(arr[0]); } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_index0_filter_guard_issue_7479() {
        // Negative control: only `.some` proves non-emptiness. `arr.filter(...)`
        // returns an array that is truthy even when empty, so it does not prove
        // `arr` non-empty and `arr[0]` stays flagged.
        let src = "function f(arr) { if (arr.filter(x => x > 0)) { use(arr[0]); } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_non_null_asserted_first_issue_7888() {
        // The `!` on `word[0]` is an explicit non-null assertion — the developer
        // has asserted the element is present (drizzle-orm casing.ts:20).
        let src = "const c = `${word[0]!.toUpperCase()}${word.slice(1)}`;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_non_null_asserted_member_issue_7888() {
        // drizzle-orm sqlite-core/foreign-keys.ts:45 — `foreignColumns[0]!.table`.
        let src = "const fk = { foreignTable: foreignColumns[0]!.table };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_non_null_asserted_minimal_repro_issue_7888() {
        // The issue body's minimal reproduction.
        let src = "export function g(cols) { return { t: cols[0]!.table }; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_non_null_asserted_last_issue_7888() {
        // The same assertion applies to a last-element read.
        let src = "const last = arr[arr.length - 1]!;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_non_null_asserted_parenthesized_issue_7888() {
        // The assertion is peeled through a parenthesized operand: `(arr[0])!`.
        let src = "const x = (arr[0])!;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_when_outer_member_asserted_issue_7888() {
        // Negative control: the `!` applies to the outer `.foo` member, so `arr[0]`
        // itself is a value read that is NOT non-null-asserted and stays flagged.
        let src = "const y = arr[0].foo!;";
        assert_eq!(run_on(src).len(), 1);
    }
}
