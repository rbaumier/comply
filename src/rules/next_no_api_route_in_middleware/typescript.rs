//! next-no-api-route-in-middleware backend.
//!
//! Restricted to `middleware.ts` / `middleware.tsx` / `src/middleware.*`
//! files. Flags `fetch("/api/...")` and `fetch(\`/api/...\`)` calls — these
//! create a same-origin loop that exhausts the edge runtime budget.

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::Framework;

fn callee_text<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    node.child_by_field_name("function")?.utf8_text(source).ok()
}

fn first_argument_text<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let args = node.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    let arg = args.named_children(&mut cursor).next()?;
    arg.utf8_text(source).ok()
}

fn is_middleware_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy().replace('\\', "/");
    s.ends_with("/middleware.ts")
        || s.ends_with("/middleware.tsx")
        || s.ends_with("/middleware.js")
        || s == "middleware.ts"
        || s == "middleware.tsx"
        || s == "middleware.js"
}

fn looks_like_internal_api_path(arg: &str) -> bool {
    let trimmed = arg.trim().trim_matches(|c| c == '"' || c == '\'' || c == '`');
    trimmed.starts_with("/api/") || trimmed == "/api"
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if ctx.project.framework != Framework::NextJs {
        return;
    }
    if !is_middleware_file(ctx.path) {
        return;
    }
    let Some(callee) = callee_text(node, source) else { return };
    if callee != "fetch" {
        return;
    }
    let Some(arg) = first_argument_text(node, source) else { return };
    if !looks_like_internal_api_path(arg) {
        return;
    }

    let pos = node.start_position();
    let range = node.byte_range();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf().into(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "next-no-api-route-in-middleware".into(),
        message: "Don't fetch `/api/*` from middleware — it triggers a same-origin loop. Inline the logic instead.".into(),
        severity: Severity::Error,
        span: Some((range.start, range.len())),
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
    use crate::project::ProjectCtx;
    use crate::rules::file_ctx::FileCtx;

    fn next_project() -> ProjectCtx {
        let mut project = ProjectCtx::empty();
        project.framework = Framework::NextJs;
        project
    }

    fn run(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, path, &next_project(), &FileCtx::default())
    }

    #[test]
    fn flags_fetch_api_in_middleware() {
        let src = "export async function middleware() { await fetch('/api/auth'); }";
        assert_eq!(run(src, "middleware.ts").len(), 1);
    }

    #[test]
    fn allows_fetch_external_in_middleware() {
        let src = "export async function middleware() { await fetch('https://example.com/x'); }";
        assert!(run(src, "middleware.ts").is_empty());
    }

    #[test]
    fn allows_fetch_api_outside_middleware() {
        let src = "async function load() { await fetch('/api/auth'); }";
        assert!(run(src, "src/lib/load.ts").is_empty());
    }

    #[test]
    fn flags_src_middleware_file() {
        let src = "export async function middleware() { await fetch('/api/x'); }";
        assert_eq!(run(src, "src/middleware.ts").len(), 1);
    }
}
