//! OxcCheck backend — flag `.listen()` + `fetch()` in test files.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [crate::rules::backend::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !ctx.project.has_framework("elysia") {
            return Vec::new();
        }
        if !is_test_file(ctx.path) {
            return Vec::new();
        }
        if !ctx.source_contains(".listen(") {
            return Vec::new();
        }
        if !ctx.source_contains("fetch(") {
            return Vec::new();
        }

        vec![Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: super::META.id.into(),
            message: "Test boots a real server with `.listen()` and uses `fetch()` — prefer `app.handle(new Request(...))`.".into(),
            severity: Severity::Warning,
            span: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on_test(source: &str) -> Vec<Diagnostic> {
        let project = crate::project::ProjectCtx::for_test_with_framework("elysia");
        crate::rules::test_helpers::run_oxc_ts_with_project(
            source,
            &Check,
            &project)
    }


    #[test]
    fn allows_handle_in_test() {
        let src = "import { Elysia } from 'elysia';\nconst app = new Elysia();\nconst res = await app.handle(new Request('http://x/'));";
        assert!(run_on_test(src).is_empty());
    }


    #[test]
    fn ignores_non_test_files() {
        let src = "import { Elysia } from 'elysia';\nconst app = new Elysia().listen(3000);\nfetch('http://localhost:3000');";
        let diags = crate::rules::test_helpers::run_oxc_ts_with_path(src, &Check, "app.ts");
        assert!(diags.is_empty());
    }
}
