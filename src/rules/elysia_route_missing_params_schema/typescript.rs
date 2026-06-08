//! elysia-route-missing-params-schema backend — flag routes with `:param`
//! placeholders but no `params:` schema in options.

use crate::diagnostic::{Diagnostic, Severity};

const ROUTE_METHODS: &[&str] = &["get", "post", "put", "patch", "delete", "head", "options"];

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" { return; }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    let prop_text = prop.utf8_text(source).unwrap_or("");
    if !ROUTE_METHODS.contains(&prop_text) { return; }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");

    // First named arg should be a string literal path. Find it.
    let mut path_str: Option<&str> = None;
    let mut cursor = args.walk();
    for child in args.children(&mut cursor) {
        if !child.is_named() { continue; }
        if child.kind() == "string" {
            path_str = Some(child.utf8_text(source).unwrap_or(""));
            break;
        }
        // not a string -> bail.
        break;
    }
    let Some(path) = path_str else { return };

    // Strip outer quotes and look for `:param` segments.
    let inner = path.trim_matches(|c| c == '\'' || c == '"' || c == '`');
    let has_param = inner.split('/').any(|seg| seg.starts_with(':'));
    if !has_param { return; }

    let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();
    if norm.contains("params:") { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-route-missing-params-schema".into(),
        message: "Route path declares `:param` but options have no `params:` schema — path params are unvalidated.".into(),
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
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, "t.ts", &crate::project::ProjectCtx::for_test_with_framework("elysia"), crate::rules::file_ctx::default_static_file_ctx())
    }

    #[test]
    fn flags_get_with_param_no_schema() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().get('/users/:id', ({ params }) => params);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_get_with_params_schema() {
        let src = "import { Elysia, t } from 'elysia';\nnew Elysia().get('/users/:id', ({ params }) => params, { params: t.Object({ id: t.Numeric() }) });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_route_without_param_placeholder() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().get('/users', () => 'ok');";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.get('/users/:id', () => 'ok');";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }
}
