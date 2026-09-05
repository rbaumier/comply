//! OxcCheck backend for js-index-maps — flag a bare-identifier
//! `.find()`/`.findIndex()`/`.filter()`/`.includes()`/`.indexOf()` that runs once
//! per iteration as a possible O(n*m) array scan.
//!
//! `.includes()`/`.indexOf()` also exist on `String.prototype`, where they are a
//! substring search with no collection to index — a `Map`/`Set` cannot answer a
//! substring query. The two method names alone therefore prove nothing, so they
//! are flagged only when the receiver is provably an array
//! ([`crate::oxc_helpers::expression_is_array`]: an array literal, an
//! array-producing call, or a binding whose declaration carries an array type
//! annotation or an array initializer). A receiver whose type cannot be resolved
//! — an imported binding, an untyped parameter, a call into another module — is
//! left alone: the rule is a performance suggestion, so a miss costs nothing
//! while a wrong hit asks for a rewrite that cannot be written.
//!
//! The other exceptions:
//! a `.includes()`/`.indexOf()` whose sole argument is a string literal is a
//! substring search over a receiver whose array-ness rests on a `String`/`Array`
//! shared method name (`.slice()`, `.concat()`), so it is not flagged;
//! a two-argument `.indexOf(value, fromIndex)` is a forward-scan cursor (a
//! positional string/array walk), never a membership lookup, so it is not flagged;
//! a property-access receiver (`product.correspondences.find(...)`) is typically a
//! bounded relation field, so it is not flagged;
//! a method-call chain rooted in an inline literal array (`["./", "/"].includes(x)`,
//! `[a, b].flat().filter(Boolean)`) has a fixed, hardcoded size independent of
//! input, so the scan is O(1), not flagged;
//! an identifier bound to a `const` NON-EMPTY inline array literal
//! (`const valid = ["yes", "no"]; valid.includes(x)`) is the same fixed-size
//! lookup table one binding removed, so the scan is O(1), not flagged (an
//! empty-array init like `const seen = []` is a growing accumulator and IS
//! still flagged);
//! a receiver that IS the element the enclosing iteration binds
//! (`for (const {rows} of groups) rows.filter(...)`) is a different, smaller
//! collection on every pass, so the total work is linear in the elements seen
//! rather than the O(n*m) rescan of one invariant collection, and it is not
//! flagged;
//! a `filter`/`find`/`findIndex` whose callback does no membership lookup against
//! a captured collection has nothing a `Map`/`Set` could replace — in particular
//! a `.has()` callback already performs the O(1) keyed-collection lookup this
//! rule asks for (`Array.prototype` has no `has`), so it is not flagged;
//! a lookup in the iterable expression of a `for..of`/`for..in`
//! (`for (const x of arr.filter(...))`) runs once before the loop, not per
//! iteration, so it is not an O(n*m) site for that loop (an enclosing outer loop
//! is still detected).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, expression_is_array};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Argument, BinaryOperator, CallExpression, ChainElement, Expression, IdentifierReference,
    LogicalOperator, VariableDeclarationKind,
};
use oxc_span::{GetSpan, Span};
use std::sync::Arc;

pub struct Check;

const LOOKUP_METHODS: &[&str] = &["find", "findIndex", "filter", "includes", "indexOf"];
/// Methods whose callback is invoked once per element of the receiver — a
/// per-iteration context. Covers the iterator methods (`forEach`/`map`/…) plus
/// the predicate-taking lookups (`filter`/`find`/`findIndex`): a lookup nested in
/// such a callback runs per element.
const CALLBACK_ITERATING_METHODS: &[&str] =
    &["forEach", "map", "flatMap", "reduce", "some", "every", "filter", "find", "findIndex"];
/// Methods whose presence in a `filter`/`find`/`findIndex` callback body marks a
/// membership scan of a collection — the O(n*m) work a `Map`/`Set` could replace.
/// `.has()` is deliberately absent: `Array.prototype` has no `has`, so a `.has()`
/// callback is a keyed-collection lookup (`Set`/`Map`/`WeakSet`/…) that is already
/// the O(1) index this rule asks the caller to build.
const MEMBERSHIP_METHODS: &[&str] = &["includes", "indexOf", "find"];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Match `.find(...)`, `.findIndex(...)`, etc.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method = member.property.name.as_str();
        if !LOOKUP_METHODS.contains(&method) {
            return;
        }

        // `String.prototype.includes`/`indexOf` take a search STRING. A sole
        // string-literal argument (`x.includes("figma")`, `x.indexOf("/")`) is a
        // substring search — there is no array to index into a Map/Set, so the
        // O(1)-lookup advice is a category error. Array-membership checks pass a
        // variable/element, not a literal substring. (`find`/`findIndex`/`filter`
        // take a callback, so this only affects `includes`/`indexOf`.)
        // This guard also covers the receivers whose array-ness rests on a method
        // name `String` and `Array` share (`s.slice(0, 3).includes("ab")`).
        if matches!(method, "includes" | "indexOf")
            && call.arguments.len() == 1
            && matches!(call.arguments.first(), Some(Argument::StringLiteral(_)))
        {
            return;
        }

        // A two-argument `indexOf(value, fromIndex)` is a forward-scan cursor: it
        // finds the next occurrence starting at an offset, walking a string/array
        // positionally (`s.indexOf('}', i + 3)`). There is no membership
        // collection to replace with a Map/Set, regardless of receiver type —
        // `Array`/`String.prototype.indexOf` both take exactly
        // `(searchValue, fromIndex)`.
        if method == "indexOf" && call.arguments.len() == 2 {
            return;
        }

        // `includes`/`indexOf` name a `String.prototype` substring search as much
        // as an `Array.prototype` membership test, and only the latter has a
        // `Map`/`Set` rewrite. Require positive evidence that the receiver is an
        // array; a receiver whose declaration is out of reach (an import, an
        // untyped parameter, a call into another module) stays unflagged.
        if matches!(method, "includes" | "indexOf")
            && !expression_is_array(&member.object, semantic)
        {
            return;
        }

        // Skip when the receiver is itself a property access (e.g. product.correspondences.find(...))
        // — relation fields are typically small and bounded; Map materialisation would be worse.
        // Transparent wrappers (a TS cast, `satisfies`, non-null `!`, parentheses, or an
        // optional-chain node) are peeled first, so a wrapped property access such as
        // `(curr?.extension as Extension[] | undefined)?.find(...)` is recognized too.
        if receiver_is_property_access(&member.object) {
            return;
        }

        // Skip when the method-call chain is rooted in an inline literal array
        // (`["./", "/"].includes(x)`, `[a, b].flat().filter(Boolean)`). The array
        // is spelled out at the call site rather than read from unbounded input,
        // so it is not the growing collection scanned per iteration that the rule
        // targets; building a Set/Map from it would only add allocation overhead
        // with no asymptotic gain.
        if root_receiver_is_literal_array(&member.object) {
            return;
        }

        // Skip when the receiver is an identifier bound to a `const` non-empty
        // inline array literal (`const valid = ["yes", "no"]; valid.includes(x)`).
        // The binding is immutable and the array's size is fixed at the
        // declaration site, so the scan is O(constant) — structurally the inline
        // `["yes", "no"].includes(x)` form one `const` binding removed.
        if receiver_is_const_bound_nonempty_array(&member.object, semantic) {
            return;
        }

        let Some(iteration) = enclosing_iteration(node, semantic) else {
            return;
        };

        // Scanning the element the iteration itself binds
        // (`for (const {rows} of groups) rows.filter(...)`) walks a different,
        // smaller collection on every pass: the total is linear in the elements
        // seen, not one invariant collection rescanned n times, so there is no
        // `Map` to hoist out of the loop.
        if let IterationBinding::Element(element) = iteration
            && receiver_is_iteration_element(&member.object, element, semantic)
        {
            return;
        }

        // `filter`/`find`/`findIndex` are O(n*m) only when their callback
        // actually scans a collection captured from the enclosing scope — the
        // work a `Map`/`Set` could replace. A bare named predicate, a
        // literal-only/side-effecting callback, or a plain property-truthiness
        // callback does no such scan. (`includes`/`indexOf` take a value, not a
        // callback, and are inherent membership lookups, so they skip this.)
        if matches!(method, "find" | "findIndex" | "filter")
            && !callback_does_captured_lookup(call, semantic)
        {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`.{method}()` inside a loop is O(n*m) — build a `Map` or `Set` for O(1) lookups."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// Peel transparent expression wrappers that do not change the structural
/// identity of the receiver: a TS cast (`x as T` / `<T>x`), `x satisfies T`, a
/// non-null assertion (`x!`), and parentheses. An optional-chain node
/// (`ChainExpression`) is left in place — its inner member access is matched
/// directly by [`receiver_is_property_access`] — because its `ChainElement`
/// payload is not an `Expression` to recurse into.
fn unwrap_expr<'a>(expr: &'a Expression<'a>) -> &'a Expression<'a> {
    match expr {
        Expression::ParenthesizedExpression(x) => unwrap_expr(&x.expression),
        Expression::TSAsExpression(x) => unwrap_expr(&x.expression),
        Expression::TSSatisfiesExpression(x) => unwrap_expr(&x.expression),
        Expression::TSNonNullExpression(x) => unwrap_expr(&x.expression),
        Expression::TSTypeAssertion(x) => unwrap_expr(&x.expression),
        _ => expr,
    }
}

/// True when `expr`, after peeling transparent wrappers ([`unwrap_expr`]), is a
/// property access — a `foo.bar` / `foo[bar]` member expression, or an
/// optional-chained one (`foo?.bar`). The wrappers do not change what the
/// receiver structurally is, so `(curr?.extension as Extension[])?.find(...)` has
/// the same property-access receiver as the bare `curr.extension.find(...)`.
///
/// A `??`/`||` default onto an empty array literal is seen through the same way:
/// `(foo.bar ?? []).find(...)` / `(foo.bar || []).find(...)` scan the relation
/// field `foo.bar` when it is present and an empty array otherwise, so the
/// receiver is still that same bounded field — the defensive `?? []` / `|| []`
/// adds nothing to index — and it keeps the exemption its bare form would get.
fn receiver_is_property_access(expr: &Expression<'_>) -> bool {
    match unwrap_expr(expr) {
        Expression::StaticMemberExpression(_) | Expression::ComputedMemberExpression(_) => true,
        Expression::ChainExpression(chain) => matches!(
            &chain.expression,
            ChainElement::StaticMemberExpression(_) | ChainElement::ComputedMemberExpression(_)
        ),
        Expression::LogicalExpression(logical)
            if matches!(logical.operator, LogicalOperator::Coalesce | LogicalOperator::Or)
                && matches!(
                    unwrap_expr(&logical.right),
                    Expression::ArrayExpression(arr) if arr.elements.is_empty()
                ) =>
        {
            receiver_is_property_access(&logical.left)
        }
        _ => false,
    }
}

/// True when the root receiver of a method-call chain is an inline array literal.
/// Walks `CallExpression` chains down through each call's member-expression callee
/// (`[a, b].flat().filter(...)` → `[a, b].flat()` → `[a, b]`) until it reaches the
/// ultimate receiver, returning true iff that root is an `Expression::ArrayExpression`.
/// Transparent wrappers ([`unwrap_expr`]) are peeled at each step, so a cast
/// literal array (`([a, b] as T[]).find(...)`) is still recognized. The base case
/// is the direct `[...].filter(...)` receiver; intermediate method calls
/// (`.flat()`, `.slice()`, …) are walked through to reach the root.
fn root_receiver_is_literal_array(expr: &Expression<'_>) -> bool {
    match unwrap_expr(expr) {
        Expression::ArrayExpression(_) => true,
        Expression::CallExpression(call) => match &call.callee {
            Expression::StaticMemberExpression(m) => root_receiver_is_literal_array(&m.object),
            Expression::ComputedMemberExpression(m) => root_receiver_is_literal_array(&m.object),
            _ => false,
        },
        _ => false,
    }
}

/// True when `expr` is an identifier bound to a `const` declaration whose
/// initializer is a NON-EMPTY inline array literal. Such a binding names a
/// fixed-size lookup table at the declaration site — structurally the inline
/// `["yes", "no"].includes(x)` form one binding removed, so a membership scan
/// over it is O(constant) and building a Set/Map would only add allocation
/// overhead. An empty-array init (`const seen = []`) is excluded: it is a
/// growing accumulator (`seen.push(x)`), the genuine O(n*m) collection the rule
/// targets. `let`/`var` bindings are excluded too: they could be reassigned to a
/// larger array, so the size is not statically bounded.
fn receiver_is_const_bound_nonempty_array<'a>(
    expr: &Expression<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let Expression::Identifier(id) = expr else {
        return false;
    };
    let Some(ref_id) = id.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let AstKind::VariableDeclarator(decl) =
        semantic.nodes().kind(scoping.symbol_declaration(sym_id))
    else {
        return false;
    };
    if decl.kind != VariableDeclarationKind::Const {
        return false;
    }
    matches!(&decl.init, Some(Expression::ArrayExpression(arr)) if !arr.elements.is_empty())
}

/// True for a `filter`/`find`/`findIndex` whose callback body performs a
/// membership/equality lookup against a value captured from the enclosing
/// scope — the O(n*m) work a `Map`/`Set` could replace. The signal is either a
/// nested membership call (`.includes()`/`.indexOf()`/`.find()`/`.has()`), or an
/// `===`/`==`/`in` comparison one of whose operands resolves to a free variable
/// (a binding declared OUTSIDE the callback, i.e. not one of its parameters or
/// locals). A bare named predicate (`arr.filter(isValid)`), a literal-only or
/// side-effecting callback (`(m) => m === 'x' ? fx() : true`), and a plain
/// property-truthiness callback (`(x) => x.active`) do no such scan.
fn callback_does_captured_lookup<'a>(
    call: &CallExpression<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let (callback_span, body_span) = match call.arguments.first() {
        Some(Argument::ArrowFunctionExpression(arrow)) => (arrow.span, arrow.body.span()),
        Some(Argument::FunctionExpression(func)) => {
            let Some(body) = &func.body else {
                return false;
            };
            (func.span, body.span())
        }
        // A bare identifier / member reference (`arr.filter(isValid)`) is an
        // opaque predicate — no visible lookup to key on.
        _ => return false,
    };

    semantic.nodes().iter().any(|descendant| {
        if !body_span.contains_inclusive(descendant.kind().span()) {
            return false;
        }
        match descendant.kind() {
            AstKind::CallExpression(inner) => matches!(
                &inner.callee,
                Expression::StaticMemberExpression(m)
                    if MEMBERSHIP_METHODS.contains(&m.property.name.as_str())
            ),
            AstKind::BinaryExpression(bin)
                if matches!(
                    bin.operator,
                    BinaryOperator::Equality | BinaryOperator::StrictEquality | BinaryOperator::In
                ) =>
            {
                operand_is_free_variable(&bin.left, callback_span, semantic)
                    || operand_is_free_variable(&bin.right, callback_span, semantic)
            }
            _ => false,
        }
    })
}

/// The head identifier of an expression: the identifier itself, or the object a
/// member chain is rooted in (`item` for `item.id`, `items` for `items[i].id`).
/// `None` when the expression is not rooted in an identifier (a literal, a call,
/// a `??` default).
fn root_identifier<'a, 'b>(expr: &'b Expression<'a>) -> Option<&'b IdentifierReference<'a>> {
    let mut cursor = expr;
    loop {
        match cursor {
            Expression::Identifier(id) => return Some(id),
            Expression::StaticMemberExpression(member) => cursor = &member.object,
            Expression::ComputedMemberExpression(member) => cursor = &member.object,
            _ => return None,
        }
    }
}

/// The span of the declaration `ident` resolves to, or `None` when the reference
/// resolves to no binding in this file — a global, or a name whose declaration
/// lives in another module.
fn binding_declaration_span(
    ident: &IdentifierReference<'_>,
    semantic: &oxc_semantic::Semantic<'_>,
) -> Option<Span> {
    let scoping = semantic.scoping();
    let symbol_id = scoping.get_reference(ident.reference_id.get()?).symbol_id()?;
    Some(
        semantic
            .nodes()
            .kind(scoping.symbol_declaration(symbol_id))
            .span(),
    )
}

/// True when `expr`'s root identifier resolves to a binding declared OUTSIDE
/// `callback_span` — a value captured from the enclosing scope (or an
/// unresolved/global name, which is never a callback-local binding). Literals
/// and other non-identifier-rooted operands are never free variables.
fn operand_is_free_variable<'a>(
    expr: &Expression<'a>,
    callback_span: Span,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let Some(ident) = root_identifier(expr) else {
        return false;
    };
    binding_declaration_span(ident, semantic)
        .is_none_or(|decl_span| !callback_span.contains_inclusive(decl_span))
}

/// True when the flagged receiver is the element the enclosing iteration binds,
/// or a value destructured from it — `element` is the span of that binding.
fn receiver_is_iteration_element<'a>(
    receiver: &Expression<'a>,
    element: Span,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    root_identifier(receiver)
        .and_then(|ident| binding_declaration_span(ident, semantic))
        .is_some_and(|decl_span| element.contains_inclusive(decl_span))
}

/// What the innermost enclosing per-iteration region binds for each pass.
#[derive(Clone, Copy)]
enum IterationBinding {
    /// A `for`/`while`/`do..while` head — the iteration binds no element.
    Anonymous,
    /// The span of the binding holding the current element: the `for..of` /
    /// `for..in` left-hand pattern, or the element parameter of an iterating
    /// callback.
    Element(Span),
}

/// The name of the method whose callback is invoked once per element of the
/// receiver (`.forEach`/`.map`/`.filter`/`.find`/…) — a per-iteration context the
/// rule treats as a loop body. `None` for any other callee.
fn iterating_method<'a>(call: &'a CallExpression<'_>) -> Option<&'a str> {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return None;
    };
    let method = member.property.name.as_str();
    CALLBACK_ITERATING_METHODS.contains(&method).then_some(method)
}

/// The binding an iterating callback gives the current element: `reduce` passes
/// the accumulator first and the element second, every other iterating method
/// passes the element first. `Anonymous` when the argument is not an inline
/// function or declares no such parameter — there is then no element binding to
/// compare a receiver against.
fn callback_element_binding(
    method: &str,
    callback: &oxc_semantic::AstNode<'_>,
) -> IterationBinding {
    let params = match callback.kind() {
        AstKind::ArrowFunctionExpression(arrow) => &arrow.params,
        AstKind::Function(func) => &func.params,
        _ => return IterationBinding::Anonymous,
    };
    let element_index = usize::from(method == "reduce");
    let Some(element) = params.items.get(element_index) else {
        return IterationBinding::Anonymous;
    };
    IterationBinding::Element(element.span)
}

/// The innermost per-iteration region enclosing `node`, or `None` when `node`
/// does not run per iteration.
fn enclosing_iteration<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<IterationBinding> {
    let nodes = semantic.nodes();
    // `child` is the node we ascended from on each step — the subtree of the
    // current ancestor that contains `node`. It distinguishes an iterator
    // method's per-iteration callback subtree from its receiver subtree.
    let mut child = nodes.get_node(node.id());
    for ancestor in nodes.ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::ForStatement(_)
            | AstKind::WhileStatement(_)
            | AstKind::DoWhileStatement(_) => return Some(IterationBinding::Anonymous),

            // `for..of` / `for..in`: a call in the ITERABLE expression
            // (`for (const x of <HERE>)`) runs once before the loop, not per
            // iteration, so it is not an O(n*m) site for THIS loop — only the
            // BODY repeats. When we ascended from the iterable subtree, keep
            // walking to catch an OUTER loop that would repeat the whole `for..of`.
            AstKind::ForOfStatement(for_of) => {
                if child.kind().span() != for_of.right.span() {
                    return Some(IterationBinding::Element(for_of.left.span()));
                }
            }
            AstKind::ForInStatement(for_in) => {
                if child.kind().span() != for_in.right.span() {
                    return Some(IterationBinding::Element(for_in.left.span()));
                }
            }

            // Named function/class/method boundaries — hoisted definitions
            // don't necessarily execute per iteration.
            AstKind::Function(f) if f.id.is_some() => return None,
            AstKind::Class(_) => return None,

            // Arrow / anonymous-function boundaries stop the walk: a callback
            // passed to an ordinary call (`bench(...)`/`group(...)`) does not run
            // per enclosing-loop iteration. The exception is a callback that
            // iterates (`.forEach`/`.map`/`.filter`/…), which IS a loop body —
            // leave the walk to the `CallExpression` arm below.
            AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
                if let AstKind::CallExpression(call) = nodes.parent_node(ancestor.id()).kind()
                    && iterating_method(call).is_some()
                {
                    child = ancestor;
                    continue;
                }
                return None;
            }

            // A callback-iterating method (`.forEach`/`.map`/`.filter`/…) is a
            // loop body only for its callback. When we arrived through the callee
            // (`X.map` member-expression receiver chain), `node` is a downstream
            // stage of a sequential pipeline (`a.filter(…).map(…)`) that runs
            // once, not per iteration — keep walking up.
            AstKind::CallExpression(call) => {
                if let Some(method) = iterating_method(call)
                    && !call.callee.span().contains_inclusive(child.kind().span())
                {
                    return Some(callback_element_binding(method, child));
                }
            }

            _ => {}
        }
        child = ancestor;
    }
    None
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
    fn flags_find_in_for_loop() {
        let diags = run(r#"
for (const item of items) {
    const match = others.find(o => o.id === item.id);
}
"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains(".find()"));
    }

    #[test]
    fn flags_find_in_for_statement() {
        let diags = run(r#"
for (let i = 0; i < items.length; i++) {
    const m = arr.findIndex(x => x.id === items[i].id);
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_filter_in_while() {
        let diags = run(r#"
while (hasMore) {
    const filtered = items.filter(i => i.id === target);
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_find_in_foreach() {
        let diags = run(r#"
items.forEach(item => {
    const match = others.find(o => o.id === item.id);
});
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_find_in_map() {
        let diags = run(r#"
const result = items.map(item => {
    return categories.find(c => c.id === item.categoryId);
});
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_find_outside_loop() {
        assert!(
            run(r#"
const user = users.find(u => u.id === targetId);
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_map_without_find() {
        assert!(
            run(r#"
const names = items.map(i => i.name);
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_find_on_non_loop_call() {
        assert!(
            run(r#"
function process() {
    const item = arr.find(x => x.id === id);
    return item;
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_find_in_anon_callback_to_ordinary_call_inside_loop() {
        // Regression for #3844: `bench(...)`/`group(...)` are ordinary calls, not
        // iterator methods — their callbacks are not run per loop iteration, and
        // `router.find()` here is a MedleyRouter method, not Array.prototype.find.
        assert!(
            run(r#"
for (const benchRoute of benchRoutes) {
    group(`${benchRoute.method} ${benchRoute.path}`, () => {
        bench('MedleyRouter', () => {
            const router = new MedleyRouter();
            const match = router.find(benchRoute.path);
            match.store[benchRoute.method];
        });
    });
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_find_in_named_function_inside_loop() {
        assert!(
            run(r#"
items.forEach(item => {
    function helper() { return others.find(o => o.id === id); }
    return helper;
});
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_relation_property_receiver() {
        // Regression for #757: product.correspondences is a bounded relation field.
        assert!(
            run(r#"
const fields = centrales.flatMap((centrale) => {
    const corr = product.correspondences.find((c) => c.centraleId === centrale.id) ?? null;
    return corr;
});
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_nested_member_chain() {
        // a.b.c is still a member expression — should not be flagged.
        assert!(
            run(r#"
items.forEach(item => {
    const x = a.b.c.find(v => v.id === item.id);
});
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_set_has_lookup_in_filter() {
        // Regression for #957: updatedGtins is a Set — `.has()` is already O(1).
        assert!(
            run(r#"
const updatedGtins = new Set(updatedRows.map((r) => r.gtin));
const unknownGtins = parsedRows
  .filter((r) => !updatedGtins.has(r.gtin))
  .map((r) => r.gtin);
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_map_has_lookup_in_find_inside_loop() {
        assert!(
            run(r#"
const byId = new Map(items.map((i) => [i.id, i]));
for (const row of rows) {
    const known = candidates.find((c) => byId.has(c.id));
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_direct_new_set_has_receiver() {
        assert!(
            run(r#"
const unknown = parsedRows
  .filter((r) => !new Set(updatedGtins).has(r.gtin))
  .map((r) => r.gtin);
"#)
            .is_empty()
        );
    }

    #[test]
    fn still_flags_includes_lookup_in_filter_chain() {
        // Plain-array `.includes()` is the genuine O(n*m) pattern.
        let diags = run(r#"
const updatedGtins = updatedRows.map((r) => r.gtin);
const unknownGtins = parsedRows
  .filter((r) => !updatedGtins.includes(r.gtin))
  .map((r) => r.gtin);
"#);
        assert!(!diags.is_empty());
    }

    #[test]
    fn no_fp_on_string_literal_includes_in_loop() {
        // Regression for #3730: `fullPath` is a string, `.includes("figma")` is a
        // substring search — there is no array to index into a Map/Set.
        assert!(
            run(r#"
for (const entry of entries) {
    const fullPath = join(dir, entry.name);
    if (fullPath.includes("figma")) continue;
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_string_literal_index_of_in_loop() {
        assert!(
            run(r#"
for (const s of strings) {
    const i = s.indexOf("/");
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_string_receiver_to_lower_case_includes_in_loop() {
        // Regression for #4566: `team.name.toLowerCase().includes(query)` is a
        // case-insensitive substring filter — the receiver is a string, not an
        // array, so there is no collection to index into a Map/Set.
        assert!(
            run(r#"
for (const team of teams) {
    if (team.name.toLowerCase().includes(normalizedQuery)) {}
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_string_literal_receiver_includes_variable_arg_in_loop() {
        // Regression for #4566: `"abc".includes(x)` is a substring search even
        // when the argument is a variable.
        assert!(
            run(r#"
for (const x of xs) {
    if ("abc".includes(x)) {}
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_template_literal_receiver_includes_in_loop() {
        assert!(
            run(r#"
for (const x of xs) {
    if (`prefix-${x}`.includes(needle)) {}
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn still_flags_array_slice_includes_in_loop() {
        // `slice`/`concat` exist on `Array.prototype` too — matching the method
        // name alone must not exempt a genuine array-membership scan.
        let diags = run(r#"
for (const r of rows) {
    if (bigArray.slice(0, 100).includes(r.gtin)) {}
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn still_flags_array_concat_includes_in_loop() {
        let diags = run(r#"
for (const r of rows) {
    if (bigArray.concat(extra).includes(r.gtin)) {}
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn still_flags_includes_with_variable_arg_in_loop() {
        // Array-membership with a variable argument is the genuine O(n*m) scan;
        // the `.map()` initializer is the evidence that the receiver is an array.
        let diags = run(r#"
const updatedGtins = updatedRows.map((r) => r.gtin);
for (const r of rows) {
    if (updatedGtins.includes(r.gtin)) {}
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_fp_on_unresolvable_receiver_includes_in_loop() {
        // `allowed` resolves to no declaration in this file, so nothing says
        // whether `.includes()` is `Array.prototype` membership or a
        // `String.prototype` substring search — the O(1)-lookup advice would be
        // unwritable if it is the latter.
        assert!(
            run(r#"
for (const k of keys) {
    if (allowed.includes(k)) {}
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_has_lookup_whatever_the_receiver_declaration() {
        // `Array.prototype` has no `has`, so a `.has()` callback is a
        // keyed-collection lookup — already the O(1) index this rule asks for —
        // no matter how the receiver is declared.
        assert!(
            run(r#"
const updatedGtins = getGtins();
for (const row of rows) {
    const known = candidates.find((c) => updatedGtins.has(c.id));
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn still_flags_find_callback_over_plain_array_inside_loop() {
        let diags = run(r#"
for (const item of items) {
    const match = others.find((o) => candidates.find((c) => c.id === o.id));
}
"#);
        assert!(!diags.is_empty());
    }

    #[test]
    fn still_flags_call_expression_receiver() {
        // getCategories() is a call result — unbounded, should still be flagged.
        let diags = run(r#"
items.map(item => {
    return getCategories().find(c => c.id === item.categoryId);
});
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_fp_on_filter_as_map_receiver() {
        // Regression for #3784: `.filter()` is the receiver of `.map()`, a
        // sequential pipeline stage that runs once — not a per-iteration body.
        assert!(
            run(r#"
const out = files.filter((f) => f.isDirectory()).map((f) => f.name);
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_longer_pipeline_chain() {
        assert!(
            run(r#"
const r = files.filter((a) => a.ok).map((b) => b.id).filter((c) => !c.hidden);
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_filter_then_foreach() {
        assert!(
            run(r#"
arr.filter((x) => x.ok).forEach((y) => use(y));
"#)
            .is_empty()
        );
    }

    #[test]
    fn still_flags_filter_in_map_callback() {
        // The inner `.filter` is nested in the `.map` callback — per-iteration.
        let diags = run(r#"
const r = items.map((i) => others.filter((o) => o.id === i.id));
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_fp_on_two_arg_index_of_cursor_in_loop() {
        // Regression for #4529: `indexOf('}', i + 3)` is a forward-scan cursor
        // (string positional walk), not an array-membership lookup.
        assert!(
            run(r#"
for (let i = 0; i < n; i++) {
    rawIndex = rawTemplate.indexOf('}', rawIndex + 3);
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_two_arg_index_of_variable_search_in_loop() {
        // A 2-arg `indexOf(value, fromIndex)` is a positional scan regardless of
        // whether the search value is a literal or a variable.
        assert!(
            run(r#"
for (const x of xs) {
    const j = arr.indexOf(x, 5);
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn still_flags_one_arg_index_of_membership_in_loop() {
        // A 1-arg `indexOf(value)` membership test is the genuine O(n*m) scan —
        // only the 2-arg `(value, fromIndex)` cursor form is exempt.
        let diags = run(r#"
const bigList: string[] = getList();
for (const item of list) {
    if (bigList.indexOf(item) !== -1) { found.push(item); }
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_fp_on_inline_literal_array_includes_in_loop() {
        // Regression for #4490: `["./", "/"].includes(slug)` — the receiver is an
        // inline literal array of fixed size 2, so the scan is O(constant) = O(1).
        assert!(
            run(r#"
for (const o of outputs) { if (["./", "/"].includes(o.slug)) {} }
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_inline_literal_array_find_in_loop() {
        // A literal array is fixed-size for every lookup method, not just includes.
        assert!(
            run(r#"
for (const x of items) { const m = [1, 2, 3].find(v => v === x); }
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_chain_rooted_in_literal_array_flat_filter_in_loop() {
        // Regression for #6612: `[lockFile, files].flat().filter(Boolean)` — the
        // root of the chain is an inline 2-element array literal, so the scan is
        // O(constant); intermediate `.flat()` stays bounded by that fixed size.
        assert!(
            run(r#"
for (const packageManager of packageManagers) {
    const detectionsFiles = [packageManager.lockFile, packageManager.files]
        .flat()
        .filter(Boolean);
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_chain_rooted_in_literal_array_slice_find_in_loop() {
        // `.slice(0, n)` between the literal and the lookup is also a bounded
        // transform of a fixed-size array.
        assert!(
            run(r#"
for (const x of items) {
    const m = [1, 2, 3, 4].slice(0, 2).find(v => v === x);
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_direct_literal_array_filter_in_loop() {
        // The base case of the chain walk: `.filter()` directly on a literal array.
        assert!(
            run(r#"
for (const x of items) {
    const m = [1, 2, 3].filter(v => v === x);
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn still_flags_chain_rooted_in_param_flat_filter_in_loop() {
        // The chain root is a parameter (unbounded), not a literal array — the
        // intermediate `.flat()` does not bound it, so still flagged.
        let diags = run(r#"
function process(arr) {
    for (const x of items) {
        const m = arr.flat().filter(v => v === x);
    }
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn still_flags_bare_unbounded_filter_in_loop() {
        let diags = run(r#"
for (const x of items) {
    const m = collection.filter(v => v === x);
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn still_flags_variable_receiver_includes_in_loop() {
        // An array-typed variable receiver is a collection that can grow with
        // input — flagged.
        let diags = run(r#"
const bigList: string[] = getList();
for (const o of outputs) { if (bigList.includes(o.slug)) {} }
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn still_flags_variable_receiver_find_in_loop() {
        let diags = run(r#"
for (const x of items) { const m = collection.find(v => v === x); }
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_fp_on_lookup_in_for_of_iterable() {
        // Regression for #4491: `.filter()` in the ITERABLE of `for..of` runs once
        // before the loop, not per iteration — not an O(n*m) site for this loop.
        assert!(
            run(r#"
for (const output of outputs.filter((o) => !o.type)) { const x = output.file; }
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_lookup_in_for_of_iterable_find() {
        // A lookup in the `for..of` iterable runs once for any lookup method.
        assert!(
            run(r#"
for (const x of arr.find(p => p.ok)) { use(x); }
"#)
            .is_empty()
        );
    }

    #[test]
    fn still_flags_lookup_in_for_of_body() {
        // A `.filter()` in the loop BODY runs per iteration — still flagged.
        let diags = run(r#"
for (const x of items) { const m = list.filter(v => v === x); }
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn still_flags_for_of_iterable_lookup_inside_outer_loop() {
        // The inner `for..of` iterable lookup runs once per OUTER-loop iteration —
        // the ascent must still reach the outer loop and flag it.
        let diags = run(r#"
for (const o of outer) { for (const x of inner.filter(p => p.id === o.id)) { use(x); } }
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_fp_on_const_bound_inline_array_includes_in_loop() {
        // Regression for #6623: `validValues` is a `const` bound to a fixed
        // 2-element array literal declared in the loop body — O(constant), the
        // inline `["yes", "no"].includes(x)` form one binding removed.
        assert!(
            run(r#"
for (const item of xmlItems) {
    const validValues = ['yes', 'no'];
    if (validValues.includes(item.family_friendly)) { keep(item); }
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_const_bound_four_element_array_includes_in_loop() {
        // A larger but still statically-fixed const array is equally bounded.
        assert!(
            run(r#"
for (const price of prices) {
    const validTypes = ['rent', 'purchase', 'package', 'subscription'];
    if (!validTypes.includes(price.type)) { reject(price); }
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn still_flags_const_bound_empty_array_accumulator_in_loop() {
        // `const seen = []` is a growing accumulator (`seen.push(x)`) — the
        // genuine O(n*m) membership scan the rule targets. The empty-array init
        // must NOT be exempted.
        let diags = run(r#"
const seen = [];
for (const x of xs) {
    if (seen.includes(x)) { continue; }
    seen.push(x);
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn still_flags_let_bound_inline_array_includes_in_loop() {
        // A `let` binding could be reassigned to a larger array — the size is not
        // statically bounded, so it is still flagged.
        let diags = run(r#"
for (const item of items) {
    let validValues = ['yes', 'no'];
    if (validValues.includes(item.flag)) { keep(item); }
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn still_flags_param_receiver_includes_in_loop() {
        // An array-typed function parameter is unbounded — not bound to a literal
        // array declaration — so it is still flagged.
        let diags = run(r#"
function check(validValues: string[]) {
    for (const item of items) {
        if (validValues.includes(item.flag)) { keep(item); }
    }
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_fp_on_typeof_string_narrowed_receiver_includes() {
        // Regression for #6357: `preset` holds a `string` here, and nothing in
        // this file makes it an array — `preset.includes(word)` is a substring
        // search with no collection to hash into a Map/Set.
        assert!(
            run(r#"
const matched = KEYWORDS_EDGE_TARGETS.some(
    word =>
        (typeof preset === "string" && preset.includes(word))
        || process.env.NITRO_PRESET?.includes(word),
);
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_bare_named_predicate_filter_in_loop() {
        // Regression for #7211: a bare named predicate is an opaque callback with
        // no visible membership lookup — there is no collection to pre-index.
        assert!(
            run(r#"
for (const item of items) {
    const valid = res.filter(filterValidExtends);
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_literal_only_side_effecting_filter_callback_in_loop() {
        // Regression for #7211: the callback only compares its parameter to string
        // literals and performs a side effect — nothing to hash into a Map/Set.
        assert!(
            run(r#"
for (const attr of attrs) {
    const kept = modifiers.filter((m) => {
        if (m === 'capture') { append(m); return false; }
        return true;
    });
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_property_truthiness_filter_callback_in_loop() {
        // Regression for #7211: a plain property-truthiness callback performs no
        // membership/equality scan of a captured collection.
        assert!(
            run(r#"
for (const x of xs) {
    const active = arr.filter((r) => r.active);
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn still_flags_filter_callback_with_inner_includes_against_captured_collection() {
        // The callback scans `rootVars` (captured) via `.includes()` — a genuine
        // O(n*m) site a Set could replace. Both the `.filter()` and the inner
        // `.includes()` (itself a per-iteration membership scan) are flagged.
        let diags = run(r#"
for (const decl of decls) {
    const free = scopes.filter((name) => !rootVars.includes(name));
}
"#);
        assert!(!diags.is_empty());
    }

    #[test]
    fn still_flags_find_callback_with_inner_includes_and_destructured_param() {
        // A destructured param (`{ type }`) with an inner `.includes()` against a
        // captured collection is still the O(n*m) pattern.
        let diags = run(r#"
for (const parent of parents) {
    const bad = nodes.find(({ type }) => !TS_NODE_TYPES.includes(type));
}
"#);
        assert!(!diags.is_empty());
    }

    #[test]
    fn still_flags_find_callback_with_equality_against_captured_key() {
        // `x.id === key` compares the element against a captured `key` — a Map
        // keyed by id could replace the linear scan.
        let diags = run(r#"
for (const item of items) {
    const hit = arr.find((x) => x.id === key);
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_fp_on_const_bound_string_call_receiver_includes_in_loop() {
        // Regression for #7420: `lc` is a `const` bound to `component.toLowerCase()`,
        // a string-returning call, so `lc.includes(keyword)` is a substring search —
        // the same category as the inline `s.toLowerCase().includes(x)` form, with no
        // collection to index into a Map/Set.
        assert!(
            run(r#"
function getMatchedPackage(component) {
    const lc = component.toLowerCase();
    for (const pkgConfig of lazyPackages) {
        const keyword = pkgConfig.name.split('/').pop();
        if (lc.includes(keyword)) { return pkgConfig; }
    }
    return null;
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_const_bound_string_literal_receiver_includes_in_loop() {
        // A `const` bound to a string literal is statically a string one binding
        // removed, so `.includes()` on it is a substring search.
        assert!(
            run(r#"
const s = 'prefix';
for (const x of xs) {
    if (s.includes(x)) {}
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_call_bound_receiver_of_unknown_type_includes_in_loop() {
        // Regression for #8229 shape 3: `getOutputLine` is declared in another
        // module, so `outputLine` may just as well be the `string` it is here —
        // `.includes()` on it has no Map/Set rewrite.
        assert!(
            run(r#"
const outputLine = getOutputLine(chunk);
for (const x of xs) {
    if (outputLine.includes(x)) {}
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn still_flags_array_param_membership_in_loop() {
        // #7420 control: `customKeywords` is an array-typed function parameter (an
        // unbounded collection) — the genuine array-membership scan the rule
        // targets, unaffected by the string-receiver exemptions.
        let diags = run(r#"
function scan(customKeywords: string[]) {
    for (const k of ks) {
        if (customKeywords.includes(k)) {}
    }
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_fp_on_string_typed_param_includes_in_find_callback() {
        // Regression for #7899: `providerName: string` — `.includes()` on a
        // `string`-typed parameter is a `String.prototype` substring search, not
        // array membership, so there is no collection to hash into a Map/Set.
        assert!(
            run(r#"
export const findFriendlyProviderName = (providerName: string) => {
  const provider = Object.keys(wellKnownProviders).find((provider) => providerName.includes(provider));
  return provider ? wellKnownProviders[provider] : null;
};
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_string_annotated_variable_includes_in_loop() {
        // A variable with an explicit `: string` annotation is a string regardless
        // of its initializer, so `.includes()` on it is a substring search.
        assert!(
            run(r#"
const s: string = getIt();
for (const x of xs) {
    if (s.includes(x)) {}
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_string_literal_union_typed_param_includes_in_loop() {
        // A union of string-literal types (`"a" | "b"`) is a `string` subtype —
        // `.includes()` on it is still `String.prototype`.
        assert!(
            run(r#"
function pick(kind: "a" | "b") {
    for (const x of xs) {
        if (kind.includes(x)) {}
    }
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn still_flags_array_annotated_variable_includes_in_loop() {
        // Negative space: a `string[]` annotation is an array, so `.includes()` is
        // the genuine O(n*m) membership scan — the `: string` exemption must not
        // extend to array-typed bindings.
        let diags = run(r#"
const arr: string[] = getIt();
for (const x of xs) {
    if (arr.includes(x)) {}
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_fp_on_optional_chained_has_typed_set_param_destructured() {
        // Regression for #7548: `excludedColumns` is a `Set<string>` destructured
        // from a typed params object and `?.has()` is O(1), so the `.filter()`
        // over the column names is not the O(n*m) scan the rule targets. Combines
        // both fixes — the typed-binding recognition and the ChainExpression unwrap.
        assert!(
            run(r#"
interface WriteRowsOptions {
    excludedColumns?: Set<string>;
}
function writeRows({ excludedColumns }: WriteRowsOptions) {
    for (const batch of batches) {
        const names = Object.keys(batch).filter(
            (columnName) => !excludedColumns?.has(columnName),
        );
    }
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_has_map_typed_param_via_type_alias() {
        // #7548: a `Map<…>` destructured from a `type` alias resolves the same way
        // as the interface case — `.has()` on it is O(1).
        assert!(
            run(r#"
type Ctx = { seen: Map<string, number> };
function scan({ seen }: Ctx) {
    for (const row of rows) {
        const hit = candidates.find((c) => seen.has(c.id));
    }
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_has_inline_object_type_destructured_set_param() {
        // #7548: a member destructured from an INLINE object type
        // (`{ s }: { s: Set<string> }`) is a known Set via the type-literal arm.
        assert!(
            run(r#"
function scan({ s }: { s: Set<string> }) {
    for (const row of rows) {
        const kept = candidates.filter((c) => s.has(c.id));
    }
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_has_variable_annotated_set_in_loop() {
        // #7548: a `const s: Set<string>` variable annotation identifies a known
        // Set even without a `new Set()` initializer, so `s.has()` is O(1).
        assert!(
            run(r#"
const s: Set<string> = getSet();
for (const row of rows) {
    const kept = candidates.filter((c) => s.has(c.id));
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_has_typed_set_param_non_optional() {
        // #7548 gap-1-only: a `Set<string>` parameter with a plain (non-optional)
        // `.has()` is O(1) — the typed-binding recognition alone must exempt it.
        assert!(
            run(r#"
function scan(excluded: Set<string>) {
    for (const row of rows) {
        const kept = candidates.filter((c) => !excluded.has(c.id));
    }
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_optional_chained_has_local_new_set() {
        // #7548 gap-2-only: an optional-chained `?.has()` on a local `new Set()`
        // binding — the ChainExpression unwrap alone must reach the known-Set
        // receiver behind the `?.`.
        assert!(
            run(r#"
const seen = new Set(getIds());
for (const row of rows) {
    const kept = candidates.filter((c) => !seen?.has(c.id));
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn still_flags_includes_on_typed_string_array_param_in_loop() {
        // #7548 negative space: `bigArray` is a `string[]` — `.includes()` is the
        // genuine O(n*m) scan a Set would replace, and a `string[]` annotation must
        // not be mistaken for a Set/Map.
        let diags = run(r#"
function scan(bigArray: string[]) {
    for (const row of rows) {
        const kept = candidates.filter((x) => bigArray.includes(x.id));
    }
}
"#);
        assert!(!diags.is_empty());
    }

    #[test]
    fn no_fp_on_has_readonly_set_typed_param_in_loop() {
        // Regression for #7622: `.has()` on a `ReadonlySet` is the same O(1)
        // keyed-collection lookup as on a `Set` — and so is `.has()` on any other
        // receiver, since `Array.prototype` has no `has` to scan.
        assert!(
            run(r#"
function scan(excluded: ReadonlySet<string>) {
    for (const row of rows) {
        const kept = candidates.filter((c) => excluded.has(c.id));
    }
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_cast_optional_chained_property_access_receiver_find_in_loop() {
        // Regression for #7505: `(curr?.extension as Extension[] | undefined)?.find(...)`
        // — the `.find()` receiver is a property access (`curr.extension`) wrapped in a
        // TS cast and an optional chain. Once the wrappers are peeled, the existing
        // property-access skip guard fires. (`curr` is also reassigned each iteration,
        // so there is no invariant array to hoist a Map from.)
        assert!(
            run(r#"
let curr = resource;
for (let i = 0; i < urls.length && curr; i++) {
    curr = (curr?.extension as Extension[] | undefined)?.find((e) => e.url === urls[i]);
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_cast_property_access_receiver_find_in_loop() {
        // A property access wrapped in a plain (non-optional) TS cast peels to the
        // `x.items` member access, so the skip guard fires.
        assert!(
            run(r#"
for (const x of xs) {
    const m = (x.items as T[]).find((v) => v.id === key);
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_nonnull_optional_chained_property_access_receiver_find_in_loop() {
        // A non-null-asserted property access behind an optional chain.
        assert!(
            run(r#"
for (const x of xs) {
    const m = (x.items!)?.find((v) => v.id === key);
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_paren_optional_chained_property_access_receiver_find_in_loop() {
        // A parenthesized property access behind an optional chain.
        assert!(
            run(r#"
for (const x of xs) {
    const m = (x.items)?.find((v) => v.id === key);
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn still_flags_bare_array_local_binding_find_in_loop() {
        // #7505 negative space: the unwrap must NOT exempt a genuine bare-identifier
        // array receiver — `arr` is a plain local collection scanned per iteration,
        // the true positive the rule targets.
        let diags = run(r#"
const arr = getItems();
for (const item of items) {
    const match = arr.find((x) => x.id === item.id);
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_fp_on_property_access_coalesce_array_default_includes_in_loop() {
        // Regression for #7910: `(c.extends ?? []).includes(x)` — the `?? []` default
        // does not change that `c.extends` is a bounded relation field, so it keeps
        // the property-access exemption the bare `c.extends.includes(x)` would get.
        assert!(
            run(r#"
const composerRootKeys = allConfigs.filter(
  (c) =>
    c.key !== config.key &&
    (c.extends ?? []).includes(config.key) &&
    context.permissions.canReadSingleProjectResource(c.project),
);
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_property_access_or_array_default_find_in_loop() {
        // #7910: the `|| []` default variant gets the same property-access exemption.
        assert!(
            run(r#"
for (const c of configs) {
    const m = (base.scopedOverrides || []).find((o) => o.config === c.key);
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_imported_const_array_includes_in_loop() {
        // Regression for #8229 shape 1a: the constant lives in a neighbouring
        // module, so this file holds no evidence of what `VERBOSE_VALUES` is —
        // the same verdict as the local declaration below.
        assert!(
            run(r#"
import { VERBOSE_VALUES } from './values.js';
export const check = (verbose) => {
    for (const fdVerbose of verbose) {
        if (!VERBOSE_VALUES.includes(fdVerbose)) { throw new TypeError('bad'); }
    }
};
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_local_const_array_includes_in_loop_matching_imported_verdict() {
        // #8229: the local twin of the fixture above — a fixed-size lookup table.
        // Moving the `const` across a module boundary must not change the verdict.
        assert!(
            run(r#"
const LOCAL_VALUES = ['none', 'short', 'full'];
export const check = (verbose) => {
    for (const fdVerbose of verbose) {
        if (!LOCAL_VALUES.includes(fdVerbose)) { throw new TypeError('bad'); }
    }
};
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_imported_set_has_in_filter_callback() {
        // Regression for #8229 shape 2a: the `.has()` predicate IS the O(1) index
        // the diagnostic asks for, whether the `Set` is declared here or imported.
        assert!(
            run(r#"
import { TRANSFORM_TYPES } from './values.js';
export const pick = (fileDescriptors) => {
    for (const { stdioItems } of fileDescriptors) {
        const transformItems = stdioItems.filter(({ type }) => TRANSFORM_TYPES.has(type));
        void transformItems;
    }
};
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_destructured_param_property_includes_in_loop() {
        // Regression for #8229 shape 4: `message` is destructured from a parameter
        // and carries no type — `.includes()` on it may well be the substring
        // search it is here.
        assert!(
            run(r#"
export const isSerializationError = ({ message }, patterns) =>
    patterns.some((pattern) => message.includes(pattern));
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_filter_over_iteration_element_in_flat_map() {
        // Regression for #8229 shape 5: each pass filters the element's OWN
        // `stdioItems`, a different collection every time — the total is linear in
        // the items seen, not one invariant collection rescanned.
        assert!(
            run(r#"
export const otherItems = (fileDescriptors, type) => fileDescriptors
    .flatMap(({ stdioItems }) => stdioItems
        .filter((stdioItem) => stdioItem.type === type));
"#)
            .is_empty()
        );
    }

    #[test]
    fn no_fp_on_filter_over_for_of_element_binding() {
        // #8229: the `for..of` spelling of the same shape — the receiver is the
        // element the loop binds.
        assert!(
            run(r#"
for (const { stdioItems } of fileDescriptors) {
    const kept = stdioItems.filter((item) => item.id === needle);
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn still_flags_invariant_collection_scanned_inside_element_binding_loop() {
        // #8229 negative space: the iteration-element exemption must not cover a
        // collection declared OUTSIDE the loop — that one is rescanned in full on
        // every pass, the O(n*m) the rule targets.
        let diags = run(r#"
const catalogue = getCatalogue();
for (const { stdioItems } of fileDescriptors) {
    const kept = catalogue.filter((item) => item.id === stdioItems.id);
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn still_flags_includes_on_reduce_accumulator_in_loop() {
        // #8229 negative space: `reduce` passes the accumulator first and the
        // element second, so the growing accumulator is NOT the iteration element
        // — scanning it per pass is the quadratic dedup the rule targets.
        let diags = run(r#"
const deduped = ids.reduce((acc: string[], id) => acc.includes(id) ? acc : [...acc, id], []);
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_fp_on_string_element_callback_param_includes() {
        // Regression for #8047: `v` is the element `values.some(...)` binds, and
        // `String.prototype.includes` on it is a substring search — flagging it
        // asks for a `Set` that cannot answer the question.
        assert!(
            run(r#"
export function getStableInterpolationReplacers(values: string[]): Record<string, string> {
    const has = (pattern: string) => values.some((v) => v.includes(pattern));
    return has('x') ? {} : {};
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn still_flags_plain_variable_receiver_array_default_find_in_loop() {
        // #7910 negative space: `(bigList ?? []).find(...)` where `bigList` is a
        // plain (unbounded) variable is NOT a relation field — the `?? []` default
        // must not extend the property-access exemption to it, so the genuine
        // O(n*m) scan stays flagged.
        let diags = run(r#"
for (const x of xs) {
    const m = (bigList ?? []).find((v) => v.id === x.id);
}
"#);
        assert_eq!(diags.len(), 1);
    }
}
