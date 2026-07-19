//! no-ignored-return OXC backend — flag standalone calls to pure methods
//! whose return value is ignored. `Array.prototype.map`/`filter` are only
//! treated as pure when their first argument is a function literal, since a
//! non-function first argument means the receiver cannot be an Array.

use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, IdentifierReference};

pub struct Check;

const PURE_METHODS: &[&str] = &[
    "map",
    "filter",
    "slice",
    "concat",
    "trim",
    "replace",
    "toUpperCase",
    "toLowerCase",
    "split",
    "join",
];

// `Array.prototype.map`/`filter` always take a function as their first
// argument. A `.map`/`.filter` call whose first argument is not a function
// literal cannot be proven to operate on an Array — it is a look-alike method
// on some other receiver (e.g. a route registrar's `router.map(routes, ctrl)`)
// — so its discarded return is not a dead pure result.
const FUNCTION_FIRST_ARG_METHODS: &[&str] = &["map", "filter"];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ExpressionStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ExpressionStatement(expr_stmt) = node.kind() else {
            return;
        };
        let Expression::CallExpression(call) = &expr_stmt.expression else {
            return;
        };
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method_name = member.property.name.as_str();
        if !PURE_METHODS.contains(&method_name) {
            return;
        }
        // Skip when the (possibly chained) receiver resolves to a `new
        // <UserClass>()` instance: a user-defined type that merely has a
        // same-named method (e.g. a fluent `MappedCode.concat` that mutates in
        // place and returns `this`) is not the pure `Array`/`String` built-in,
        // so its ignored return is a real side effect, not dead code.
        if let Some(root) = receiver_root_ident(&member.object)
            && receiver_is_user_class_instance(root, semantic)
        {
            return;
        }
        // Skip look-alike `.map`/`.filter` on non-Array receivers (see
        // `FUNCTION_FIRST_ARG_METHODS`): the Array form mandates a function
        // literal first argument.
        if FUNCTION_FIRST_ARG_METHODS.contains(&method_name)
            && !first_arg_is_function_literal(call)
        {
            return;
        }
        // `String.prototype.replace`/`replaceAll` is only pure with a string
        // replacement. With a function replacer (`replace(re, (...m) => {...})`)
        // the callback carries the side effects and the discarded string
        // result is the canonical "iterate every match" idiom — not dead.
        if matches!(method_name, "replace" | "replaceAll")
            && let Some(
                Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_),
            ) = call.arguments.get(1).and_then(|arg| arg.as_expression())
        {
            return;
        }
        // Arrow concise body (`xs.map(fn)` is the implicit-return
        // expression of `() => xs.map(fn)`) wraps the call in an
        // ExpressionStatement under a FunctionBody, but the value
        // IS returned. Common JSX list pattern:
        // `{items.map(item => <Item />)}`
        let parent = semantic.nodes().parent_node(node.id());
        if let AstKind::FunctionBody(_) = parent.kind() {
            let grand = semantic.nodes().parent_node(parent.id());
            if let AstKind::ArrowFunctionExpression(arrow) = grand.kind()
                && arrow.expression
            {
                return;
            }
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, expr_stmt.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Return value of `.{}` is ignored — the call has no side effect.",
                method_name
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// `Array.prototype.map`/`filter` mandate a function callback as their first
/// argument. Returns `true` only when the call's first argument — after peeling
/// parentheses — is a syntactic function literal (arrow or function
/// expression); any other first argument (identifier, object/array literal,
/// member expression, literal, spread, or none) cannot prove an Array receiver.
fn first_arg_is_function_literal(call: &oxc_ast::ast::CallExpression<'_>) -> bool {
    matches!(
        call.arguments
            .first()
            .and_then(|arg| arg.as_expression())
            .map(crate::oxc_helpers::peel_parens),
        Some(Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_))
    )
}

/// Walk a pure-method call's receiver down to the identifier at the root of the
/// chain. Peels chained calls (`out.concat(a).concat(b)` → `out`) but never
/// property accesses (`m.items.concat(x)` stays unresolved so `m.items` is not
/// mistaken for `m`). Returns `None` for any receiver that does not bottom out
/// in a plain identifier.
fn receiver_root_ident<'a>(
    mut receiver: &'a Expression<'a>,
) -> Option<&'a IdentifierReference<'a>> {
    loop {
        match receiver {
            Expression::Identifier(id) => return Some(id),
            Expression::CallExpression(call) => match &call.callee {
                Expression::StaticMemberExpression(member) => receiver = &member.object,
                _ => return None,
            },
            _ => return None,
        }
    }
}

/// Whether `id` resolves to a binding whose initializer is `new <Ctor>()` where
/// `<Ctor>` is a user-defined constructor — not the `Array`/`String` built-in
/// whose pure methods `PURE_METHODS` targets. Such a receiver's same-named
/// method is a look-alike, so discarding its return is not a dead pure result.
fn receiver_is_user_class_instance(
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
        if let AstKind::VariableDeclarator(decl) = kind {
            let Some(Expression::NewExpression(new_expr)) = &decl.init else {
                return false;
            };
            return matches!(&new_expr.callee, Expression::Identifier(ctor)
                if !matches!(ctor.name.as_str(), "Array" | "String"));
        }
    }
    false
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
    fn flags_standalone_map_call() {
        let src = "function f(xs) { xs.map(x => x); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_arrow_concise_body_returning_map() {
        // Regression for rbaumier/comply#20 — `.map(...)` returning JSX
        // child as the implicit return of an arrow.
        let src = "const f = xs => xs.map(x => x);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_assigned_map_call() {
        let src = "const result = xs.map(x => x);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_replace_with_arrow_replacer() {
        // Regression for rbaumier/comply#3963 — `String.prototype.replace`
        // used as a side-effecting match iterator: the discarded return
        // value is legitimate because the replacer callback does the work.
        let src = "function f(source, re) { source.replace(re, (...m) => { push(m); return ''; }); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_replace_all_with_function_replacer() {
        // Regression for rbaumier/comply#3963 — function-expression replacer.
        let src = "function f(s, re) { s.replaceAll(re, function (m) { side(m); return ''; }); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_replace_with_string_replacement() {
        // A string replacement is genuinely pure — the discarded return
        // value is dead, so the call must still flag.
        let src = "function f(source, re) { source.replace(re, 'x'); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_router_map_with_identifier_first_arg() {
        // Regression for rbaumier/comply#6966 — `router.map(routes, controller)`
        // is a route-registration method, not `Array.prototype.map`: its first
        // argument is not a function literal, so the receiver cannot be an Array.
        let src = "function f(router, routes, controller) { router.map(routes, controller); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_router_map_with_object_second_arg() {
        // Regression for rbaumier/comply#6966 — fetch-router's
        // `router.map(routes, { actions: { ... } })` registration form.
        let src = "function f(router, routes) { router.map(routes, { actions: { async root() {} } }); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_array_map_with_arrow_first_arg() {
        // A real `Array.prototype.map` with a function-literal first argument
        // and an ignored return is still dead code.
        let src = "[1, 2, 3].map(x => x * 2);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_array_map_with_function_expression_first_arg() {
        let src = "function f(arr) { arr.map(function (x) { return x; }); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_array_map_with_named_callback_identifier() {
        // Accepted trade-off for rbaumier/comply#6966 — an identifier first
        // argument is indistinguishable from a non-Array `.map()` API, so a
        // real `arr.map(namedFn)` with an ignored return is no longer flagged.
        let src = "function f(arr, namedFn) { arr.map(namedFn); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_map_with_parenthesized_arrow_first_arg() {
        // Parentheses are preserved by the parser; a wrapped function literal
        // is still a real `Array.prototype.map` with a dead ignored return.
        let src = "function f(arr) { arr.map((x => x)); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_map_with_spread_first_arg() {
        // A spread first argument is not a function literal, so the receiver
        // cannot be proven to be an Array.
        let src = "function f(arr, fns) { arr.map(...fns); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_filter_with_arrow_first_arg() {
        let src = "function f(arr) { arr.filter(x => x > 0); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_filter_with_non_function_first_arg() {
        // `Array.prototype.filter` also mandates a function first argument; a
        // `router.filter(routes, opts)` look-alike must not be flagged.
        let src = "function f(router, routes, opts) { router.filter(routes, opts); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_chained_concat_on_user_class_instance() {
        // Regression for rbaumier/comply#7236 — `out` is a `MappedCode`
        // instance whose fluent `concat` mutates in place and returns `this`;
        // discarding the chained return is correct, not a dead pure call.
        let src = "class MappedCode { concat(o) { return this; } } const out = new MappedCode(); out.concat(a).concat(b);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_single_concat_on_user_class_instance() {
        // Regression for rbaumier/comply#7236 — the single-call form of a
        // user-defined fluent `concat` receiver.
        let src = "class MappedCode { concat(o) { return this; } } const out = new MappedCode(); out.concat(a);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_concat_on_array_literal() {
        // A genuine `Array.prototype.concat` with an ignored return is dead.
        let src = "[1, 2].concat([3]);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_concat_on_array_literal_binding() {
        // The receiver resolves to an array-literal binding, not a `new`
        // instance, so its ignored `concat` result is still dead.
        let src = "const arr = [1, 2]; arr.concat([3]);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_concat_on_string_literal() {
        // A genuine `String.prototype.concat` with an ignored return is dead.
        let src = "\"a\".concat(\"b\");";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_concat_on_builtin_array_ctor_instance() {
        // `new Array()` is the built-in Array constructor, so its `concat` is
        // the pure built-in and the ignored return is dead.
        let src = "const s = new Array(); s.concat([1]);";
        assert_eq!(run(src).len(), 1);
    }
}
