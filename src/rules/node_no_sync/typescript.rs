//! node-no-sync backend — flag synchronous Node.js method calls (`*Sync()`).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["Sync"] => |node, source, ctx, diagnostics|
    if super::allows_sync_node_api(ctx.path, ctx.source) {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };

    // Extract the method name from either `fooSync(...)` or `fs.readFileSync(...)`.
    let method_name = match callee.kind() {
        "identifier" => callee.utf8_text(source).unwrap_or(""),
        "member_expression" => {
            let Some(prop) = callee.child_by_field_name("property") else { return };
            prop.utf8_text(source).unwrap_or("")
        }
        _ => return,
    };

    // Only flag genuine Node.js core synchronous I/O methods, not arbitrary
    // identifiers ending in `Sync` (e.g. `flushSync`, `batchSync`).
    if !super::is_node_sync_io_method(method_name) {
        return;
    }

    // A sync call inside a function whose name ends in `Sync` is intentional:
    // the function's contract is to be synchronous (Node convention).
    if enclosing_function_is_sync(node, source) {
        return;
    }

    // Reconstruct the full callee text for the message.
    let full_name = callee.utf8_text(source).unwrap_or(method_name);

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "node-no-sync".into(),
        message: format!("Unexpected sync method: `{full_name}()`. Use the async variant instead."),
        severity: Severity::Warning,
        span: None,
    });
}

/// Walks the ancestors of the call node until the innermost enclosing
/// function-like node, returning true when that function's name ends in `Sync`.
///
/// Covers `function fooSync(){}`, `const fooSync = () => {}` / `= function(){}`,
/// and method/property forms `fooSync(){}` / `fooSync: () => {}`. The innermost
/// function defines the synchronous contract.
fn enclosing_function_is_sync(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cur = node.parent();
    while let Some(n) = cur {
        let name = match n.kind() {
            "function_declaration" | "generator_function_declaration" | "method_definition" => n
                .child_by_field_name("name")
                .and_then(|name| name.utf8_text(source).ok()),
            // Function expressions may be named (`function fooSync(){}`) or
            // anonymous and bound to a `*Sync` name (`const fooSync = function(){}`).
            "function_expression" | "arrow_function" => n
                .child_by_field_name("name")
                .and_then(|name| name.utf8_text(source).ok())
                .or_else(|| binding_name(n, source)),
            _ => {
                cur = n.parent();
                continue;
            }
        };
        return name.is_some_and(super::function_name_is_sync);
    }
    false
}

/// Resolves the name an arrow function is bound to via its parent:
/// a `*Sync` `variable_declarator` or an object `pair` whose key ends in `Sync`.
fn binding_name<'a>(arrow: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let parent = arrow.parent()?;
    let field = match parent.kind() {
        "variable_declarator" => "name",
        "pair" => "key",
        _ => return None,
    };
    parent
        .child_by_field_name(field)?
        .utf8_text(source)
        .ok()
        .map(|name| name.trim_matches(['\'', '"', '`']))
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

    fn run_on_path(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, path)
    }

    #[test]
    fn allows_sync_calls_in_scripts_dir() {
        assert!(run_on_path("const out = execSync('ls');", "scripts/check-dependencies.js").is_empty());
    }

    #[test]
    fn allows_sync_calls_in_config_files() {
        assert!(run_on_path("const data = fs.readFileSync('x');", "svelte.config.js").is_empty());
        assert!(run_on_path("const data = fs.readFileSync('x');", "vite.config.ts").is_empty());
    }

    #[test]
    fn allows_sync_calls_in_node_shebang_cli() {
        let source = "#!/usr/bin/env node\nconst out = execSync('ls');";
        assert!(run_on_path(source, "src/check.js").is_empty());
    }

    #[test]
    fn flags_read_file_sync() {
        let d = run_on("const data = fs.readFileSync('f.txt');");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("readFileSync"));
    }

    #[test]
    fn flags_exec_sync() {
        assert_eq!(run_on("const out = execSync('ls');").len(), 1);
    }

    #[test]
    fn flags_other_node_sync_io_methods() {
        assert_eq!(run_on("if (fs.existsSync('f')) {}").len(), 1);
        assert_eq!(run_on("const s = fs.statSync('f');").len(), 1);
        assert_eq!(run_on("const out = spawnSync('ls');").len(), 1);
    }

    #[test]
    fn allows_async_method() {
        assert!(run_on("fs.readFile('f.txt', cb);").is_empty());
    }

    #[test]
    fn allows_non_sync_identifier() {
        assert!(run_on("const isInSync = true;").is_empty());
    }

    #[test]
    fn allows_react_hook_sync_call() {
        // Issue #110 — `useListSearchSync` is a React hook synchronising state,
        // not Node sync I/O.
        let source = "const [state, onChange] = useListSearchSync(Route, { filterKeys: ['level'] });";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_other_react_hook_sync_variants() {
        assert!(run_on("const x = useStateSync();").is_empty());
        assert!(run_on("const x = useSearchParamsSync();").is_empty());
    }

    #[test]
    fn allows_react_on_callback_sync() {
        // Issue #359 — `onSearchSync` is a React event-callback prop (sync search
        // variant), not a Node sync I/O method.
        let source = r#"
            const { onSearchSync } = projection;
            if (onSearchSync !== null) {
                const results = onSearchSync(debouncedTerm);
            }
        "#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_other_on_callback_sync_variants() {
        assert!(run_on("const x = onChangeSync(val);").is_empty());
        assert!(run_on("const x = onStateSync(val);").is_empty());
    }

    #[test]
    fn allows_on_callback_as_method() {
        // Issue #359 — `obj.onSearchSync()` accessed via MemberExpression.
        assert!(run_on("hooks.onSearchSync(term);").is_empty());
    }

    #[test]
    fn allows_framework_flush_sync() {
        // Issue #1767 — Svelte's/React's `flushSync` is a DOM-batching helper,
        // not Node sync I/O, and must not be flagged.
        assert!(run_on("flushSync();").is_empty());
        assert!(run_on("ReactDOM.flushSync(() => setState(x));").is_empty());
    }

    #[test]
    fn allows_arbitrary_sync_suffixed_identifiers() {
        // Issue #1767 — application-level helpers ending in `Sync` are not Node
        // I/O methods.
        assert!(run_on("const x = batchSync();").is_empty());
        assert!(run_on("const x = renderSync();").is_empty());
        assert!(run_on("const x = usersSync();").is_empty());
        assert!(run_on("const x = onceSync();").is_empty());
    }

    #[test]
    fn allows_sync_io_in_sync_named_function() {
        // Issue #1344 — a `*Sync` function advertises its synchronous contract.
        assert!(run_on("function readConfigSync() { return fs.readFileSync(p); }").is_empty());
        assert!(run_on("const readConfigSync = () => fs.readFileSync(p);").is_empty());
        assert!(
            run_on("const readConfigSync = function() { return fs.readFileSync(p); };").is_empty()
        );
        assert!(run_on("class C { readConfigSync() { return fs.readFileSync(p); } }").is_empty());
        assert!(run_on("const o = { readConfigSync: () => fs.readFileSync(p) };").is_empty());
    }

    #[test]
    fn flags_sync_io_in_non_sync_named_function() {
        // Negative space: a function NOT ending in Sync is still flagged.
        assert_eq!(run_on("function readConfig() { return fs.readFileSync(p); }").len(), 1);
    }
}
