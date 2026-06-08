//! no-async-array-callback backend — flag `arr.forEach(async ...)`.
//!
//! Detects `call_expression` where the callee is a `member_expression` with
//! one of the iterator method names, and the first argument is an async
//! function/arrow. We deliberately skip `map` in contexts like
//! `Promise.all(arr.map(async ...))` — too hard to detect statically, and
//! the map pattern is the idiomatic way to parallelize.
//!
//! We flag: `forEach`, `filter`, `some`, `every`, `find`, `findIndex`,
//! `findLast`, `findLastIndex`. These methods do not await their
//! callbacks' returned promises, so async callbacks are almost always a
//! bug.

use crate::diagnostic::{Diagnostic, Severity};

const FLAGGED_METHODS: &[&str] = &[
    "forEach",
    "filter",
    "some",
    "every",
    "find",
    "findIndex",
    "findLast",
    "findLastIndex",
];

fn is_async_function(node: tree_sitter::Node, source: &[u8]) -> bool {
    match node.kind() {
        "arrow_function" | "function_expression" | "function" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.utf8_text(source).unwrap_or("") == "async" {
                    return true;
                }
            }
            false
        }
        _ => false,
    }
}

crate::ast_check! { on ["call_expression"] prefilter = ["forEach", "filter", "some", "every", "find", "findIndex", "findLast", "findLastIndex"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    let method = prop.utf8_text(source).unwrap_or("");
    if !FLAGGED_METHODS.contains(&method) {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    // First named child of arguments is the first argument.
    let Some(first) = args.named_child(0) else { return };

    if !is_async_function(first, source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-async-array-callback".into(),
        message: format!(
            "`.{method}` does not await its callback — the async work runs \
             unsupervised. Use a `for...of` loop with `await`, or \
             `Promise.all(arr.map(async ...))` for parallel awaited work."
        ),
        severity: Severity::Warning,
        span: None,
    });
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_foreach_async_arrow() {
        let d = run_on("arr.forEach(async (x) => { await f(x); });");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-async-array-callback");
    }

    #[test]
    fn flags_filter_async_fn() {
        let d = run_on("arr.filter(async function (x) { return await g(x); });");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_some() {
        let d = run_on("arr.some(async (x) => await g(x));");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_find() {
        let d = run_on("arr.find(async (x) => await g(x));");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_sync_foreach() {
        assert!(run_on("arr.forEach((x) => f(x));").is_empty());
    }

    #[test]
    fn allows_map_async() {
        // map with async is the idiomatic Promise.all pattern — don't flag it.
        assert!(run_on("Promise.all(arr.map(async (x) => await g(x)));").is_empty());
    }

    #[test]
    fn allows_non_array_method() {
        assert!(run_on("obj.handle(async () => {});").is_empty());
    }
}
