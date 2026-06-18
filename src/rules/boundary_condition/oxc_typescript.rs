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
use crate::oxc_helpers::byte_offset_to_line_col;
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

        // Skip if inside an `if` whose condition guards this array — either a
        // `.length` check or, for a first-element read, a truthy `arr[0]` /
        // `arr?.[0]` check on the same array.
        if has_length_guard_ancestor(node, semantic, obj_text, is_first, source) {
            return;
        }

        // Skip if inside a `switch (<obj_text>.length)` case that proves the array
        // is non-empty — a `case N:` with `N >= 1`, or a `default:` when the
        // switch lists `case 0:` (so length 0 is handled elsewhere).
        if has_switch_length_guard_ancestor(node, semantic, obj_text, source) {
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
            severity: Severity::Warning,
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
///   1. any `.length` check in an enclosing `if` condition (covers first and last
///      reads);
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
                if cond_text.contains(".length") {
                    return true;
                }
                if is_first && condition_guards_index0(&if_stmt.test, obj_text, source) {
                    return true;
                }
            }
            AstKind::ConditionalExpression(cond) => {
                let in_consequent = cond.consequent.span().start <= node_span.start
                    && node_span.end <= cond.consequent.span().end;
                let in_test = cond.test.span().start <= node_span.start
                    && node_span.end <= cond.test.span().end;
                if in_consequent || in_test {
                    let cond_text = &source
                        [cond.test.span().start as usize..cond.test.span().end as usize];
                    // `.length` guard applies to the truthy consequent.
                    if in_consequent && cond_text.contains(&format!("{obj_text}.length")) {
                        return true;
                    }
                    // A truthy `arr[0]` / `arr?.[0]` test narrows the consequent AND
                    // exempts the test's own `[0]` access — the ternary equivalent of
                    // `if (arr[0])`. The alternate branch stays flagged.
                    if is_first && condition_guards_index0(&cond.test, obj_text, source) {
                        return true;
                    }
                }
            }
            _ => {}
        }
        current_id = parent_id;
    }
}

/// Returns true when `expr` (an `if` condition) contains a zero-index access
/// `obj_text[0]` / `obj_text?.[0]`, where `obj_text` is matched after stripping
/// optional-chaining `?.` to `.` on both sides. Recurses through the operators
/// that preserve a truthiness guard: `&&`, `||`, `!`, and parentheses.
fn condition_guards_index0(expr: &Expression, obj_text: &str, source: &str) -> bool {
    match expr {
        Expression::ComputedMemberExpression(member) => {
            if is_zero_index(&member.expression, source)
                && normalize_optional_chaining(expr_text(&member.object, source))
                    == normalize_optional_chaining(obj_text)
            {
                return true;
            }
            condition_guards_index0(&member.object, obj_text, source)
        }
        Expression::StaticMemberExpression(member) => {
            condition_guards_index0(&member.object, obj_text, source)
        }
        Expression::ChainExpression(chain) => match &chain.expression {
            ChainElement::ComputedMemberExpression(member) => {
                if is_zero_index(&member.expression, source)
                    && normalize_optional_chaining(expr_text(&member.object, source))
                        == normalize_optional_chaining(obj_text)
                {
                    return true;
                }
                condition_guards_index0(&member.object, obj_text, source)
            }
            ChainElement::StaticMemberExpression(member) => {
                condition_guards_index0(&member.object, obj_text, source)
            }
            _ => false,
        },
        Expression::LogicalExpression(logical) => {
            condition_guards_index0(&logical.left, obj_text, source)
                || condition_guards_index0(&logical.right, obj_text, source)
        }
        Expression::UnaryExpression(unary) => {
            condition_guards_index0(&unary.argument, obj_text, source)
        }
        Expression::ParenthesizedExpression(paren) => {
            condition_guards_index0(&paren.expression, obj_text, source)
        }
        _ => false,
    }
}

/// Strips optional-chaining tokens so `data?.choices` and `data.choices` compare
/// equal. The condition writes the access with `?.`, the flagged in-block read
/// without it; both denote the same array.
fn normalize_optional_chaining(text: &str) -> String {
    text.replace("?.", ".")
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

/// Returns true if `stmt` or a top-level statement within it is an early exit
/// (return, throw, or a bare `.exit()` call such as `process.exit(1)`).
fn body_has_early_exit(stmt: &Statement) -> bool {
    match stmt {
        Statement::ReturnStatement(_) | Statement::ThrowStatement(_) => true,
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
/// `const` is honored over an outer one). Only a direct `ArrayExpression`
/// literal qualifies: a call initializer (`getColors()`) or a `let` is unknown
/// and stays flagged. A spread element makes the length non-static, so an array
/// literal containing one does not qualify either.
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
                return matches!(
                    &declarator.init,
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

/// Returns true when `ident`'s binding has a literal tuple type annotation with
/// at least one element — making `ident[0]` provably in-bounds. Resolves the
/// reference to its declaration and reads the `type_annotation` on the enclosing
/// `FormalParameter` (`p: [A, B]`) or `VariableDeclarator` (`const p: [A, B]`).
/// A `readonly [...]` wrapper is unwrapped. An aliased tuple type
/// (`LineSegment<T>`) is a `TSTypeReference`, not a `TSTupleType`, so it does not
/// match: resolving the alias to its tuple definition needs type information this
/// native backend doesn't have.
fn resolves_to_nonempty_tuple_type(
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
            .is_some_and(|ann| ts_type_is_nonempty_tuple(&ann.type_annotation));
    }
    false
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
    let Expression::CallExpression(call) = expr else {
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

/// Returns true when `expr` is a `<recv>.split(...)` call. `<recv>` may itself be
/// a member chain (e.g. `this.name.split(...)`), so only the called property name
/// is checked. Any argument shape qualifies — `split()` with no argument returns
/// `[wholeString]`, still non-empty.
fn is_split_call(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr else {
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
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    member.property.name.as_str() == "matchAll"
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

/// Returns true when `expr` is a guard condition that is satisfied exactly when
/// `name` is nullish/falsy: `!name`, `name === null` / `name == null`, or
/// `name === undefined` / `name == undefined`.
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
        // Negative space (residual): an aliased tuple type cannot be resolved to
        // its tuple definition without type information, so it stays flagged. This
        // is the issue's own `LineSegment<GlobalPoint>` case — needs --type-aware.
        let src = "function f(seg: LineSegment<GlobalPoint>) { return seg[0]; }";
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
}
