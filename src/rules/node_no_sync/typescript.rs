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

    // Must end with "Sync" and have at least one char before it.
    if method_name.len() <= 4 || !method_name.ends_with("Sync") {
        return;
    }

    // Skip React hooks and event callbacks: `use[A-Z]…Sync` / `on[A-Z]…Sync`.
    if super::oxc_typescript::is_react_sync_name(method_name) {
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
    fn still_flags_lowercase_use_prefix() {
        // `users` is not a hook — must still be flagged if it ends in Sync.
        assert_eq!(run_on("const x = usersSync();").len(), 1);
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
    fn still_flags_lowercase_on_prefix() {
        // `once` is not a React callback — must still be flagged if it ends in Sync.
        assert_eq!(run_on("const x = onceSync();").len(), 1);
    }
}
