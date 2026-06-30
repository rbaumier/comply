//! OxcCheck backend for ts-no-promise-void-function-misuse.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Argument, AssignmentOperator, AssignmentTarget, BindingPattern, Expression,
};
use oxc_semantic::SymbolId;
use oxc_span::GetSpan;
use std::sync::Arc;

const DIRECT_CALLEES: &[&str] = &[
    "setTimeout",
    "setInterval",
    "setImmediate",
    "queueMicrotask",
];

const MEMBER_METHODS: &[&str] = &[
    "forEach",
    "map",
    "filter",
    "reduce",
    "some",
    "every",
    "find",
    "findIndex",
    "nextTick",
];

pub struct Check;

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

        let (matches, display) = match &call.callee {
            Expression::Identifier(id) => {
                let name = id.name.as_str();
                (DIRECT_CALLEES.contains(&name), name.to_string())
            }
            Expression::StaticMemberExpression(member) => {
                let prop = member.property.name.as_str();
                if MEMBER_METHODS.contains(&prop) {
                    let obj_text =
                        &ctx.source[member.object.span().start as usize..member.object.span().end as usize];
                    (true, format!("{obj_text}.{prop}"))
                } else {
                    (false, String::new())
                }
            }
            _ => (false, String::new()),
        };

        if !matches {
            return;
        }

        // The map results are awaited — inline (`Promise.all(arr.map(async ...))`)
        // or via a binding that later reaches an awaiting sink. Rejections are not
        // swallowed, so this is handled rather than a void misuse.
        if is_consumed_by_promise_combinator(node, semantic) {
            return;
        }

        // `await arr.reduce(async ...)` threads a promise through the accumulator
        // and the outer `await` consumes the final chain — not a void misuse.
        if is_awaited_reduce(node, semantic) {
            return;
        }

        // Check the first argument for async
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        if !is_async_arg(first_arg) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{display}(async ...)` ignores the returned promise. Wrap with \
                 `() => {{ void asyncFn(); }}` or refactor `.forEach` into a `for ... of` with `await`."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_async_arg(arg: &Argument) -> bool {
    match arg {
        Argument::ArrowFunctionExpression(arrow) => arrow.r#async,
        Argument::FunctionExpression(func) => func.r#async,
        _ => false,
    }
}

/// True when the call is `arr.reduce(async ...)` whose result is the operand of
/// an `await` (`await arr.reduce(...)` or `const x = await arr.reduce(...)`).
///
/// `reduce` returns its accumulator, which in the sequential-async idiom is the
/// threaded `Promise` chain (`(prev, item) => { await prev; ... }`,
/// `Promise.resolve()` seed); the outer `await` consumes that promise, so it is
/// not ignored. This is narrowly `reduce`-only: `forEach` returns `undefined`
/// and the other array methods coerce the callback's promise to a truthy
/// non-promise value, so awaiting the whole call does not consume the inner
/// promises — those remain genuine misuses.
fn is_awaited_reduce(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let AstKind::CallExpression(call) = node.kind() else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    if member.property.name.as_str() != "reduce" {
        return false;
    }
    matches!(
        semantic.nodes().parent_node(node.id()).kind(),
        AstKind::AwaitExpression(_)
    )
}

/// True when the promises produced by a `.map()`/`.flatMap()` CallExpression are
/// handled rather than discarded. Handled means either:
///
/// - the call is itself an argument of a `Promise.<all|allSettled|race|any>(...)`
///   combinator (the inline idiom `Promise.all(arr.map(async ...))`), or
/// - the call is the entire right-hand side of a binding (`const xs = arr.map(...)`
///   or `xs = arr.map(...)`) and the bound variable later reaches an awaiting sink
///   — passed to one of those combinators, `await`ed, `return`ed, or spread into a
///   `.push(...)`/`.unshift(...)` accumulator array that itself reaches such a sink.
///
/// The second case is resolved through the semantic model, so it holds whatever
/// the variable is named, whichever combinator awaits it, and regardless of the
/// binding sitting in a different (e.g. conditional) block from the sink.
fn is_consumed_by_promise_combinator(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let AstKind::CallExpression(call) = node.kind() else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    if !matches!(member.property.name.as_str(), "map" | "flatMap") {
        return false;
    }

    let nodes = semantic.nodes();
    let parent = nodes.parent_node(node.id());
    if is_awaiting_combinator_argument(parent.kind(), call.span) {
        return true;
    }

    // Spread form `Promise.all([...arr.map(async ...)])`: the map call is wrapped in
    // a SpreadElement inside an ArrayExpression that is the combinator's argument, so
    // its rejections still propagate to the awaiting site. Walk the bounded chain
    // SpreadElement -> ArrayExpression -> Promise.<all|allSettled|race|any>(...) call.
    if matches!(parent.kind(), AstKind::SpreadElement(_)) {
        let array = nodes.parent_node(parent.id());
        if let AstKind::ArrayExpression(array_expr) = array.kind() {
            let array_parent_kind = nodes.parent_node(array.id()).kind();
            if is_awaiting_combinator_argument(array_parent_kind, array_expr.span) {
                return true;
            }
        }
    }

    bound_variable(node, semantic)
        .is_some_and(|symbol_id| variable_reaches_awaiting_sink(symbol_id, semantic))
}

/// True when `parent_kind` is a `Promise.<all|allSettled|race|any>(...)` call
/// that receives the expression spanning `child_span` as one of its arguments.
fn is_awaiting_combinator_argument(parent_kind: AstKind, child_span: oxc_span::Span) -> bool {
    let AstKind::CallExpression(call) = parent_kind else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    let Expression::Identifier(obj) = &member.object else {
        return false;
    };
    if obj.name.as_str() != "Promise" {
        return false;
    }
    if !matches!(
        member.property.name.as_str(),
        "all" | "allSettled" | "race" | "any"
    ) {
        return false;
    }
    call.arguments.iter().any(|arg| arg.span() == child_span)
}

/// When the `.map`/`.flatMap` call at `node` is the entire right-hand side of a
/// binding — a `const`/`let` initializer (`const xs = arr.map(...)`) or a plain
/// `=` reassignment to an identifier (`xs = arr.map(...)`) — return the bound
/// variable's symbol. Returns `None` when the call is not the head of a binding
/// (e.g. it is an argument, chained off another call, or a compound assignment).
fn bound_variable(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> Option<SymbolId> {
    match semantic.nodes().parent_node(node.id()).kind() {
        AstKind::VariableDeclarator(declarator) => match &declarator.id {
            BindingPattern::BindingIdentifier(ident) => ident.symbol_id.get(),
            _ => None,
        },
        AstKind::AssignmentExpression(assign) => {
            if assign.operator != AssignmentOperator::Assign {
                return None;
            }
            let AssignmentTarget::AssignmentTargetIdentifier(target) = &assign.left else {
                return None;
            };
            semantic
                .scoping()
                .get_reference(target.reference_id())
                .symbol_id()
        }
        _ => None,
    }
}

/// True when any reference to `symbol_id` is consumed by an awaiting sink: it is
/// `await`ed, `return`ed, or passed to a `Promise.<all|allSettled|race|any>(...)`
/// combinator. A reference spread into a `.push(...)`/`.unshift(...)` accumulator
/// array also counts when that accumulator itself reaches such a sink — the
/// promises are collected into a shared array and later awaited via
/// `Promise.all(accumulator)`, so they are handled.
fn variable_reaches_awaiting_sink(
    symbol_id: SymbolId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    reaches_awaiting_sink(symbol_id, semantic, true)
}

/// Core of [`variable_reaches_awaiting_sink`]. When `follow_spread` is set, a
/// reference spread into a `.push(...)`/`.unshift(...)` accumulator counts as a
/// sink if the accumulator reaches a sink itself; the recursive accumulator check
/// runs with `follow_spread` cleared, bounding the transitivity to one extra hop
/// so mutually-spreading accumulators cannot recurse without end.
fn reaches_awaiting_sink(
    symbol_id: SymbolId,
    semantic: &oxc_semantic::Semantic,
    follow_spread: bool,
) -> bool {
    let nodes = semantic.nodes();
    semantic.symbol_references(symbol_id).any(|reference| {
        let ref_node_id = reference.node_id();
        let ref_span = nodes.kind(ref_node_id).span();
        let parent = nodes.parent_node(ref_node_id);
        if matches!(
            parent.kind(),
            AstKind::AwaitExpression(_) | AstKind::ReturnStatement(_)
        ) || is_awaiting_combinator_argument(parent.kind(), ref_span)
        {
            return true;
        }
        follow_spread && reference_spreads_into_awaited_accumulator(parent, semantic)
    })
}

/// True when `parent` is the `SpreadElement` of an `accumulator.push(...ref)` /
/// `accumulator.unshift(...ref)` call whose `accumulator` identifier resolves to a
/// symbol that reaches an awaiting sink. The accumulator is matched purely on AST
/// shape (member call, `push`/`unshift` property) and resolved by `SymbolId`, and
/// its own references are checked without following a further spread.
fn reference_spreads_into_awaited_accumulator(
    parent: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    if !matches!(parent.kind(), AstKind::SpreadElement(_)) {
        return false;
    }
    let AstKind::CallExpression(call) = semantic.nodes().parent_node(parent.id()).kind() else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    if !matches!(member.property.name.as_str(), "push" | "unshift") {
        return false;
    }
    let Expression::Identifier(accumulator) = &member.object else {
        return false;
    };
    let Some(accumulator_symbol) = semantic
        .scoping()
        .get_reference(accumulator.reference_id())
        .symbol_id()
    else {
        return false;
    };
    reaches_awaiting_sink(accumulator_symbol, semantic, false)
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
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_set_timeout_async() {
        assert_eq!(run("setTimeout(async () => { await save(); }, 100);").len(), 1);
    }

    #[test]
    fn flags_foreach_async() {
        assert_eq!(run("items.forEach(async (i) => { await save(i); });").len(), 1);
    }

    #[test]
    fn flags_bare_map_async() {
        // result discarded, not consumed by Promise.all
        assert_eq!(run("arr.map(async (x) => { await save(x); });").len(), 1);
    }

    // --- #2309: Promise.all(arr.map(async ...)) is the canonical idiom ---

    #[test]
    fn allows_promise_all_map_async() {
        let src = "Promise.all(dataSources.map(async (c) => { await c.save(); }));";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_promise_all_settled_map_async() {
        let src = "Promise.allSettled(arr.map(async (x) => { await save(x); }));";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_promise_race_map_async() {
        let src = "Promise.race(arr.map(async (x) => { await save(x); }));";
        assert!(run(src).is_empty());
    }

    // --- #6559: map result spread into the Promise.all argument array ---

    #[test]
    fn allows_promise_all_spread_map_async() {
        // The map call is spread into the array literal that is the awaited
        // `Promise.all` argument; its rejections propagate, so it is consumed.
        let src = "async function run() {\n\
                       await Promise.all([\n\
                           ...options.format.map(async (format, index) => {\n\
                               await build(format, index);\n\
                           }),\n\
                       ]);\n\
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_spread_map_async_into_plain_array() {
        // Spread into an array literal that is NOT a combinator argument — the
        // promises still float, so the diagnostic must fire.
        let src = "const xs = [...arr.map(async (x) => { await save(x); })];";
        assert_eq!(run(src).len(), 1);
    }

    // --- #3343: `.map(async ...)` bound to a variable then handled later ---

    #[test]
    fn allows_map_async_bound_then_promise_all() {
        // Exact issue example: the map results are assigned across conditional
        // branches and then awaited via `Promise.all(loadItems)`.
        let src = "export const load = async ({ params, data }) => {\n\
                       let loadItems;\n\
                       if (category === \"sidebar\") {\n\
                           loadItems = data.sidebars.map(async (block) => {\n\
                               const resp = await fetch(`/api/block/${block}`);\n\
                               return (await resp.json());\n\
                           });\n\
                       } else if (category === \"dashboard\") {\n\
                           loadItems = data.dashboards.map(async (block) => {\n\
                               const resp = await fetch(`/api/block/${block}`);\n\
                               return (await resp.json());\n\
                           });\n\
                       }\n\
                       const blocks = await Promise.all(loadItems);\n\
                       return { blocks };\n\
                   };";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_const_map_async_then_promise_allsettled() {
        // Different variable name + `Promise.allSettled` instead of `all`.
        let src = "const tasks = items.map(async (x) => fetchOne(x));\n\
                   const results = await Promise.allSettled(tasks);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_map_async_bound_then_returned() {
        // The bound array is returned rather than awaited inline.
        let src = "function run() {\n\
                       const ps = items.map(async (x) => fetchOne(x));\n\
                       return ps;\n\
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_map_async_bound_then_awaited_directly() {
        // The bound variable is `await`ed (not via a combinator).
        let src = "async function run() {\n\
                       const ps = items.map(async (x) => fetchOne(x));\n\
                       await ps;\n\
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_map_async_bound_then_dropped() {
        // The bound array is never awaited, returned, or passed to a combinator —
        // the promises genuinely float, so the diagnostic must still fire.
        let src = "function run() {\n\
                       const ps = items.map(async (x) => fetchOne(x));\n\
                       console.log(ps.length);\n\
                   }";
        assert_eq!(run(src).len(), 1);
    }

    // --- #6988: map result spread into an accumulator that is later awaited ---

    #[test]
    fn allows_map_async_spread_into_awaited_accumulator() {
        // Exact issue example: the map result is bound, spread into the `promises`
        // accumulator via `.push(...)`, and the accumulator is awaited via
        // `Promise.all` — the promises are collected, not dropped.
        let src = "async function run() {\n\
                       const promises = [];\n\
                       const coreInputPromises = inputList.core.map(async (schema) => {\n\
                           const response = await fetchInputSchema(schema);\n\
                           schemas[schema] = response;\n\
                       });\n\
                       promises.push(...coreInputPromises);\n\
                       await Promise.all(promises);\n\
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_map_async_unshift_into_awaited_accumulator() {
        // `.unshift(...)` collects into the accumulator the same way `.push(...)`
        // does.
        let src = "async function run() {\n\
                       const promises = [];\n\
                       const tasks = items.map(async (x) => { await save(x); });\n\
                       promises.unshift(...tasks);\n\
                       await Promise.all(promises);\n\
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_map_async_spread_into_unawaited_accumulator() {
        // The accumulator is never awaited, returned, or combined — spreading into
        // it does not launder the floating promises, so the diagnostic fires.
        let src = "function run() {\n\
                       const promises = [];\n\
                       const tasks = items.map(async (x) => { await save(x); });\n\
                       promises.push(...tasks);\n\
                       console.log(promises.length);\n\
                   }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_map_async_mutual_spread_no_sink() {
        // Two accumulators spread into each other but neither reaches a sink. The
        // one-hop bound must terminate (no infinite recursion) and still flag.
        let src = "function run() {\n\
                       const a = [];\n\
                       const b = [];\n\
                       const tasks = items.map(async (x) => { await save(x); });\n\
                       a.push(...tasks);\n\
                       b.push(...a);\n\
                       a.push(...b);\n\
                   }";
        assert_eq!(run(src).len(), 1);
    }

    // --- #6257: `await arr.reduce(async ...)` consumes the threaded promise ---

    #[test]
    fn allows_awaited_reduce_async() {
        // Sequential-async idiom: the outer `await` consumes the promise chain
        // returned by `reduce`.
        let src = "async function run() {\n\
                       await dump.reduce(async (prev, sql) => {\n\
                           await prev;\n\
                           await db.exec(sql);\n\
                       }, Promise.resolve());\n\
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_awaited_reduce_async_assigned() {
        // `const result = await arr.reduce(async ...)` — the await is still the
        // immediate parent of the call.
        let src = "async function run() {\n\
                       const result = await arr.reduce(async (prev, cur) => {\n\
                           await prev;\n\
                           return cur;\n\
                       }, Promise.resolve());\n\
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_reduce_async_not_awaited() {
        // No outer `await` — the promise produced by `reduce` floats.
        let src = "dump.reduce(async (prev, sql) => {\n\
                       await prev;\n\
                       await db.exec(sql);\n\
                   }, Promise.resolve());";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_awaited_foreach_async() {
        // `forEach` returns `undefined`; awaiting it does not consume the inner
        // async callbacks' promises — still a misuse.
        let src = "async function run() {\n\
                       await arr.forEach(async (x) => { await save(x); });\n\
                   }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_awaited_some_async() {
        // `some` returns a boolean; the callback's promise is coerced to truthy.
        // Awaiting the whole call does not consume it — still a misuse.
        let src = "async function run() {\n\
                       await arr.some(async (x) => { await check(x); });\n\
                   }";
        assert_eq!(run(src).len(), 1);
    }
}
