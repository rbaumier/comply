//! OxcCheck backend for ts-no-promise-void-function-misuse.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
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

        // `Promise.all(arr.map(async ...))` (and `allSettled`/`race`/`any`) fully
        // consumes the returned promises, so rejections are not swallowed — the
        // canonical concurrency idiom this rule's own remediation recommends.
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

/// True when `node` (a `.map()`/`.flatMap()` CallExpression) is itself an
/// argument of a `Promise.<all|allSettled|race|any>(...)` call. Those
/// combinators await every promise in the array, so the returned promises are
/// consumed rather than discarded.
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

    let AstKind::CallExpression(parent_call) = semantic.nodes().parent_node(node.id()).kind() else {
        return false;
    };
    let Expression::StaticMemberExpression(parent_member) = &parent_call.callee else {
        return false;
    };
    let Expression::Identifier(obj) = &parent_member.object else {
        return false;
    };
    if obj.name.as_str() != "Promise" {
        return false;
    }
    if !matches!(
        parent_member.property.name.as_str(),
        "all" | "allSettled" | "race" | "any"
    ) {
        return false;
    }

    parent_call
        .arguments
        .iter()
        .any(|arg| arg.span() == call.span)
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
}
