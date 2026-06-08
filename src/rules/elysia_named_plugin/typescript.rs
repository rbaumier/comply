//! elysia-named-plugin backend — flag exported Elysia instances missing `name`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["new_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(constructor) = node.child_by_field_name("constructor") else { return };
    if constructor.utf8_text(source).unwrap_or("") != "Elysia" {
        return;
    }

    // Walk up to see if this `new Elysia(...)` is part of an exported declaration.
    let mut cur = node;
    let mut exported = false;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "export_statement" {
            exported = true;
            break;
        }
        cur = parent;
    }
    if !exported {
        return;
    }

    // Inspect arguments: missing entirely, or present but no `name:` field.
    let args_text = node
        .child_by_field_name("arguments")
        .map(|a| a.utf8_text(source).unwrap_or(""))
        .unwrap_or("");
    let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();
    if norm.contains("name:'") || norm.contains("name:\"") || norm.contains("name:`") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-named-plugin".into(),
        message: "Exported Elysia plugin has no `name` — pass `new Elysia({ name: '...' })` for deduplication and clearer error traces.".into(),
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
    fn flags_exported_unnamed_plugin() {
        let src = "import { Elysia } from 'elysia';\nexport const plugin = new Elysia();";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_exported_options_without_name() {
        let src = "import { Elysia } from 'elysia';\nexport const plugin = new Elysia({ prefix: '/v1' });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_named_plugin() {
        let src =
            "import { Elysia } from 'elysia';\nexport const plugin = new Elysia({ name: 'auth' });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_non_exported_app() {
        let src = "import { Elysia } from 'elysia';\nconst app = new Elysia();";
        assert!(run_on(src).is_empty());
    }
}
