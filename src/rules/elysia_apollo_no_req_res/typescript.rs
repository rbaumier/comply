//! elysia-apollo-no-req-res backend — flag `context: ({ req, res }) => ...` in Elysia + Apollo files.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["pair"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(key) = node.child_by_field_name("key") else { return };
    if key.utf8_text(source).unwrap_or("") != "context" {
        return;
    }

    let Some(value) = node.child_by_field_name("value") else { return };
    if value.kind() != "arrow_function" && value.kind() != "function_expression" {
        return;
    }

    let value_text = value.utf8_text(source).unwrap_or("");
    // Only inspect the parameter list, not the body.
    let Some(arrow_idx) = value_text.find("=>").or_else(|| value_text.find('{')) else { return };
    let params = &value_text[..arrow_idx];
    let norm: String = params.chars().filter(|c| !c.is_whitespace()).collect();
    if !(norm.contains("{req,") || norm.contains(",req,") || norm.contains(",req}") || norm == "{req}"
        || norm.contains("{req,res}") || norm.contains(",res}") || norm.contains("{res,")) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-apollo-no-req-res".into(),
        message: "Apollo + Elysia context exposes `{ request }`, not `{ req, res }`.".into(),
        severity: Severity::Error,
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
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, "t.ts", &crate::project::ProjectCtx::for_test_with_framework("elysia"), crate::rules::file_ctx::default_static_file_ctx())
    }

    #[test]
    fn flags_req_res_context() {
        let src = "import { apollo } from '@elysiajs/apollo';\napp.use(apollo({ context: ({ req, res }) => ({ token: req.headers.authorization }) }));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_request_context() {
        let src = "import { apollo } from '@elysiajs/apollo';\napp.use(apollo({ context: ({ request }) => ({ token: request.headers.get('authorization') }) }));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_apollo_files() {
        let src = "app.use(({ context: ({ req, res }) => ({}) }));";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }
}
