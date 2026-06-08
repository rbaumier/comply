//! hono-cors-permissive backend — flag permissive CORS configurations.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["hono/cors"] => |node, source, ctx, diagnostics|
    // Only check files that import from 'hono/cors'.
    if !ctx.source_contains("hono/cors") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = callee.utf8_text(source).unwrap_or("");
    if callee_text != "cors" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");
    let pos = node.start_position();

    // `cors()` with no arguments — defaults to `origin: '*'`.
    if args_text == "()" {
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "hono-cors-permissive".into(),
            message: "`cors()` without arguments defaults to `origin: '*'` — any origin can access the API.".into(),
            severity: Severity::Error,
            span: None,
        });
        return;
    }

    let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();

    // `origin: '*'` or `origin: "*"`.
    if norm.contains("origin:'*'") || norm.contains("origin:\"*\"") {
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "hono-cors-permissive".into(),
            message: "`origin: '*'` allows any origin to access the API.".into(),
            severity: Severity::Error,
            span: None,
        });
    }

    // `credentials: true` without a specific origin.
    if norm.contains("credentials:true") {
        let has_specific_origin = (norm.contains("origin:") || norm.contains("origin:"))
            && !norm.contains("origin:'*'")
            && !norm.contains("origin:\"*\"");
        if !has_specific_origin {
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "hono-cors-permissive".into(),
                message: "`credentials: true` without a specific origin — any origin can make credentialed requests.".into(),
                severity: Severity::Error,
                span: None,
            });
        }
    }
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
    fn flags_bare_cors() {
        let src = "import { cors } from 'hono/cors';\napp.use(cors());";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_wildcard_origin() {
        let src = "import { cors } from 'hono/cors';\napp.use(cors({ origin: '*' }));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_credentials_without_origin() {
        let src = "import { cors } from 'hono/cors';\napp.use(cors({\n  credentials: true\n}));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_specific_origin() {
        let src =
            "import { cors } from 'hono/cors';\napp.use(cors({ origin: 'https://example.com' }));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_credentials_with_specific_origin() {
        let src = "import { cors } from 'hono/cors';\napp.use(cors({\n  origin: 'https://example.com',\n  credentials: true\n}));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_hono_files() {
        let src = "app.use(cors());";
        assert!(run_on(src).is_empty());
    }
}
