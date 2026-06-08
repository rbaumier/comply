//! node-no-top-level-await OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

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
    crate::oxc_helpers::source_contains(source, ".listen(") || crate::oxc_helpers::source_contains(source, "process.exit")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AwaitExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["await"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::AwaitExpression(await_expr) = node.kind() else {
            return;
        };

        if is_test_file(ctx.path)
            || is_script_file(ctx.path, ctx.source)
            || is_entrypoint(ctx.source)
        {
            return;
        }

        // Walk up: if inside any function scope, this is not top-level.
        for ancestor in semantic.nodes().ancestors(node.id()) {
            match ancestor.kind() {
                AstKind::Function(_)
                | AstKind::ArrowFunctionExpression(_) => {
                    return; // Inside a function — not top-level.
                }
                _ => {}
            }
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, await_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Top-level `await` is forbidden in published modules.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    fn run_at(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path(source, &Check, path)
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
