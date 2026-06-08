//! elysia-listen-callback-info backend — flag .listen() with no callback.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "listen" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    // Count value arguments (skipping parens / commas).
    let mut value_args: Vec<tree_sitter::Node> = Vec::new();
    for i in 0..args.child_count() {
        let Some(child) = args.child(i) else { continue };
        let kind = child.kind();
        if kind == "(" || kind == ")" || kind == "," {
            continue;
        }
        value_args.push(child);
    }
    if value_args.len() != 1 {
        return;
    }
    let only = value_args[0];
    // Already has a callback as the first arg — uncommon but valid; don't flag.
    if matches!(only.kind(), "arrow_function" | "function" | "function_expression") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-listen-callback-info".into(),
        message: "`.listen(...)` has no callback — pass one and log the server info so deploys show where the server is bound.".into(),
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
    fn flags_listen_with_port_only() {
        let src = "import { Elysia } from 'elysia';\napp.listen(3000);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_listen_with_variable() {
        let src = "import { Elysia } from 'elysia';\napp.listen(PORT);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_listen_with_callback() {
        let src = "import { Elysia } from 'elysia';\napp.listen(3000, ({ hostname, port }) => console.log(hostname, port));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.listen(3000);";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }
}
