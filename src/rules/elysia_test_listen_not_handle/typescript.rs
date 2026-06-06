//! elysia-test-listen-not-handle backend — flag `.listen()` + `fetch()` in test files.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    let _ = (node, source);
    if !ctx.project.has_framework("elysia") {
        return;
    }
    if !is_test_file(ctx.path) {
        return;
    }
    if !ctx.source_contains(".listen(") {
        return;
    }
    if !ctx.source_contains("fetch(") {
        return;
    }

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: 1,
        column: 1,
        rule_id: "elysia-test-listen-not-handle".into(),
        message: "Test boots a real server with `.listen()` and uses `fetch()` — prefer `app.handle(new Request(...))`.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on_test(source: &str) -> Vec<Diagnostic> {
        let project = crate::project::ProjectCtx::for_test_with_framework("elysia");
        crate::rules::test_helpers::run_ts_with_project_and_path(
            source,
            &Check,
            &project,
            std::path::Path::new("app.test.ts"),
        )
    }

    #[test]
    fn flags_listen_with_fetch_in_test() {
        let src = "import { Elysia } from 'elysia';\nconst app = new Elysia().listen(3000);\nconst res = await fetch('http://localhost:3000');";
        assert_eq!(run_on_test(src).len(), 1);
    }

    #[test]
    fn allows_handle_in_test() {
        let src = "import { Elysia } from 'elysia';\nconst app = new Elysia();\nconst res = await app.handle(new Request('http://x/'));";
        assert!(run_on_test(src).is_empty());
    }

    #[test]
    fn ignores_non_test_files() {
        let src = "import { Elysia } from 'elysia';\nconst app = new Elysia().listen(3000);\nfetch('http://localhost:3000');";
        let diags = crate::rules::test_helpers::run_ts_with_path(src, &Check, "app.ts");
        assert!(diags.is_empty());
    }
}
