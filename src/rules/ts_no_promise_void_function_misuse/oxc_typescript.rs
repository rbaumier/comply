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
        _semantic: &'a oxc_semantic::Semantic<'a>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;
    use super::Check;



    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_set_timeout_async() {
        let src = "setTimeout(async () => { await save(); }, 100);";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_set_interval_async() {
        let src = "setInterval(async () => { await tick(); }, 1000);";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_foreach_async() {
        let src = "items.forEach(async (i) => { await save(i); });";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_set_timeout_void_wrapper() {
        let src = "setTimeout(() => { void save(); }, 100);";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_set_timeout_sync_callback() {
        let src = "setTimeout(() => doStuff(), 100);";
        assert!(run(src).is_empty());
    }


    #[test]
    fn flags_set_timeout_with_spaces() {
        // Space between paren and async keyword
        let src = "setTimeout( async () => { await save(); }, 100);";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_process_next_tick() {
        let src = "process.nextTick(async () => { await cleanup(); });";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn ignores_comment_with_pattern() {
        let src = "// setTimeout(async () => bad())";
        assert!(run(src).is_empty());
    }


    #[test]
    fn flags_array_map_async() {
        let src = "arr.map(async (item) => { await save(item); });";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_array_filter_async() {
        let src = "arr.filter(async (item) => { return await check(item); });";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_array_reduce_async() {
        let src = "arr.reduce(async (acc, item) => { return acc; }, 0);";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_array_some_async() {
        let src = "arr.some(async (item) => { return await check(item); });";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_array_every_async() {
        let src = "arr.every(async (item) => { return await check(item); });";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_array_find_async() {
        let src = "arr.find(async (item) => { return await check(item); });";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_array_find_index_async() {
        let src = "arr.findIndex(async (item) => { return await check(item); });";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_foreach_sync() {
        let src = "items.forEach((i) => { save(i); });";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_unrelated_call() {
        let src = "doSomething(async () => { await work(); });";
        assert!(run(src).is_empty());
    }


    #[test]
    fn flags_set_immediate_async() {
        let src = "setImmediate(async () => { await work(); });";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_queue_microtask_async() {
        let src = "queueMicrotask(async () => { await work(); });";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn ignores_string_with_pattern() {
        let src = r#"const msg = "setTimeout(async () => bad())";"#;
        assert!(run(src).is_empty());
    }
}
