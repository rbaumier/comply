//! node-no-top-level-await backend — disallow top-level `await`.
//!
//! Scope: only published library modules. The rule skips:
//!   - Test files (`*.test.*`, `*.spec.*`, `__tests__/`, `tests/`, `e2e/`).
//!   - Scripts (`scripts/` directory, or files starting with a `#!` shebang).
//!   - Entrypoints (sources containing `.listen(` or `process.exit`), which
//!     are bin scripts / server bootstraps, not consumed as imports.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    if TEST_MARKERS.iter().any(|m| s.contains(m)) {
        return true;
    }
    path.components()
        .any(|c| c.as_os_str() == "tests" || c.as_os_str() == "e2e")
}

fn is_script_file(path: &std::path::Path, source: &str) -> bool {
    if path.components().any(|c| c.as_os_str() == "scripts") {
        return true;
    }
    source.starts_with("#!")
}

fn is_entrypoint(source: &str) -> bool {
    source.contains(".listen(") || source.contains("process.exit")
}

crate::ast_check! { on ["await_expression"] => |node, source, ctx, diagnostics|
    let _ = source;
    if is_test_file(ctx.path)
        || is_script_file(ctx.path, ctx.source)
        || is_entrypoint(ctx.source)
    {
        return;
    }

    // Walk up: if we're inside any function scope, this is not top-level.
    let mut current = node.parent();
    while let Some(ancestor) = current {
        let ak = ancestor.kind();
        if ak == "function_declaration"
            || ak == "function"
            || ak == "arrow_function"
            || ak == "method_definition"
            || ak == "generator_function"
            || ak == "generator_function_declaration"
        {
            return; // Not top-level — inside a function.
        }
        current = ancestor.parent();
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "node-no-top-level-await".into(),
        message: "Top-level `await` is forbidden in published modules.".into(),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    fn run_at(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_path(source, &Check, path)
    }

    #[test]
    fn flags_top_level_await() {
        let d = run_on("const data = await fetch('/api');");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Top-level"));
    }

    #[test]
    fn allows_await_in_async_function() {
        assert!(run_on("async function load() { const data = await fetch('/api'); }").is_empty());
    }

    #[test]
    fn allows_await_in_arrow() {
        assert!(run_on("const load = async () => { await fetch('/api'); };").is_empty());
    }

    #[test]
    fn allows_top_level_await_in_test_file() {
        let src = "const data = await fetch('/api');";
        assert!(run_at(src, "src/foo.test.ts").is_empty());
        assert!(run_at(src, "src/foo.spec.ts").is_empty());
        assert!(run_at(src, "src/__tests__/foo.ts").is_empty());
        assert!(run_at(src, "tests/foo.ts").is_empty());
        assert!(run_at(src, "e2e/foo.ts").is_empty());
    }

    #[test]
    fn allows_top_level_await_in_scripts_dir() {
        let src = "const data = await fetch('/api');";
        assert!(run_at(src, "scripts/seed.ts").is_empty());
    }

    #[test]
    fn allows_top_level_await_in_shebang_script() {
        let src = "#!/usr/bin/env tsx\nconst data = await fetch('/api');";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_top_level_await_in_listen_entrypoint() {
        let src = r#"
const port = await getPort();
app.listen(port);
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_top_level_await_with_process_exit() {
        let src = r#"
const ok = await runMigration();
if (!ok) process.exit(1);
"#;
        assert!(run_on(src).is_empty());
    }
}
