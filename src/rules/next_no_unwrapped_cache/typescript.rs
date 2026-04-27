//! next-no-unwrapped-cache backend.
//!
//! Flags `unstable_cache(fn, ...)` and `cache(fn)` calls when the first
//! argument is an inline arrow/function whose body does not contain
//! `try`. A throw inside a cached function poisons subsequent reads, so
//! the inner body should always catch.

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::Framework;

fn callee_text<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    node.child_by_field_name("function")?.utf8_text(source).ok()
}

fn first_argument(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let args = node.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    args.named_children(&mut cursor).next()
}

fn body_contains_try(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Ok(text) = node.utf8_text(source) else { return false };
    text.contains("try")
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if ctx.project.framework != Framework::NextJs {
        return;
    }
    let Some(callee) = callee_text(node, source) else { return };
    if callee != "unstable_cache" && callee != "cache" {
        return;
    }
    let Some(arg) = first_argument(node) else { return };
    let kind = arg.kind();
    if kind != "arrow_function" && kind != "function_expression" && kind != "function" {
        return;
    }
    if body_contains_try(arg, source) {
        return;
    }

    let pos = node.start_position();
    let range = node.byte_range();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf().into(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "next-no-unwrapped-cache".into(),
        message: format!(
            "`{callee}` callback has no try/catch — an unhandled throw will poison the cache."
        ),
        severity: Severity::Warning,
        span: Some((range.start, range.len())),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::ProjectCtx;
    use crate::rules::file_ctx::FileCtx;

    fn next_project() -> ProjectCtx {
        let mut project = ProjectCtx::empty();
        project.framework = Framework::NextJs;
        project
    }

    fn run(source: &str, project: &ProjectCtx) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx_with_project_and_file(
            source,
            &Check,
            project,
            &FileCtx::default(),
        )
    }

    #[test]
    fn flags_unstable_cache_without_try() {
        let src = "const get = unstable_cache(async () => { return await fetch('/x'); }, ['k']);";
        assert_eq!(run(src, &next_project()).len(), 1);
    }

    #[test]
    fn allows_unstable_cache_with_try() {
        let src = "const get = unstable_cache(async () => { try { return await fetch('/x'); } catch { return null; } }, ['k']);";
        assert!(run(src, &next_project()).is_empty());
    }

    #[test]
    fn ignores_non_nextjs_project() {
        let src = "const get = unstable_cache(async () => { return 1; }, ['k']);";
        assert!(run(src, &ProjectCtx::empty()).is_empty());
    }
}
