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

/// True when the promises produced by a `.map()`/`.flatMap()` CallExpression are
/// handled rather than discarded. Handled means either:
///
/// - the call is itself an argument of a `Promise.<all|allSettled|race|any>(...)`
///   combinator (the inline idiom `Promise.all(arr.map(async ...))`), or
/// - the call is the entire right-hand side of a binding (`const xs = arr.map(...)`
///   or `xs = arr.map(...)`) and the bound variable later reaches an awaiting sink
///   — passed to one of those combinators, `await`ed, or `return`ed.
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

    let parent_kind = semantic.nodes().parent_node(node.id()).kind();
    if is_awaiting_combinator_argument(parent_kind, call.span) {
        return true;
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
/// combinator. Such a sink awaits the collected promises, so they are handled.
fn variable_reaches_awaiting_sink(
    symbol_id: SymbolId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    semantic.symbol_references(symbol_id).any(|reference| {
        let ref_span = nodes.kind(reference.node_id()).span();
        let parent_kind = nodes.parent_node(reference.node_id()).kind();
        matches!(
            parent_kind,
            AstKind::AwaitExpression(_) | AstKind::ReturnStatement(_)
        ) || is_awaiting_combinator_argument(parent_kind, ref_span)
    })
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
}
