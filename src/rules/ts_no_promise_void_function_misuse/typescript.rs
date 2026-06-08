//! Flag `setTimeout(async`, `setInterval(async`, `queueMicrotask(async`,
//! `process.nextTick(async`, and `.forEach(async` patterns. The shared
//! property: callee discards the callback's return value, so a rejected
//! promise has nowhere to go.

use crate::diagnostic::{Diagnostic, Severity};

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

/// Returns `true` when `node` is an `arrow_function` or `function` with the
/// `async` keyword.
fn is_async_function(node: tree_sitter::Node) -> bool {
    match node.kind() {
        "arrow_function" | "function" | "function_expression" | "generator_function" => {
            // The first named child of an async function is usually the
            // body/params, but tree-sitter marks the `async` keyword as an
            // anonymous child with text "async".  Walk unnamed children.
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.is_named() {
                    break;
                }
                if child.kind() == "async" {
                    return true;
                }
            }
            false
        }
        _ => false,
    }
}

/// Extract the callee display name for the diagnostic message.
fn callee_display(callee: tree_sitter::Node, source: &[u8]) -> String {
    match callee.kind() {
        "identifier" => callee.utf8_text(source).unwrap_or("?").to_string(),
        "member_expression" => {
            if let Some(prop) = callee.child_by_field_name("property") {
                let obj = callee.child_by_field_name("object");
                let obj_text = obj
                    .map(|o| o.utf8_text(source).unwrap_or("?"))
                    .unwrap_or("?");
                let prop_text = prop.utf8_text(source).unwrap_or("?");
                format!("{obj_text}.{prop_text}")
            } else {
                callee.utf8_text(source).unwrap_or("?").to_string()
            }
        }
        _ => callee.utf8_text(source).unwrap_or("?").to_string(),
    }
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };

    let matches = match callee.kind() {
        "identifier" => {
            let name = callee.utf8_text(source).unwrap_or("");
            DIRECT_CALLEES.contains(&name)
        }
        "member_expression" => {
            if let Some(prop) = callee.child_by_field_name("property") {
                let prop_name = prop.utf8_text(source).unwrap_or("");
                MEMBER_METHODS.contains(&prop_name)
            } else {
                false
            }
        }
        _ => false,
    };
    if !matches { return; }

    // Check the first argument for async
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let Some(first_arg) = args.named_child(0) else { return };
    if !is_async_function(first_arg) { return; }

    let display = callee_display(callee, source);
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "`{display}(async ...)` ignores the returned promise. Wrap with \
             `() => {{ void asyncFn(); }}` or refactor `.forEach` into a `for ... of` with `await`."
        ),
        Severity::Warning,
    ));
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
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
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

    // --- new tests ---

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
