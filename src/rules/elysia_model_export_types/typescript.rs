//! elysia-model-export-types backend — when a file exports a `t.Object(...)`
//! const, expect a corresponding `typeof X.static` type alias.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    let _ = source;
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let norm: String = ctx.source.chars().filter(|c| !c.is_whitespace()).collect();

    let exports_typebox_const = norm.contains("exportconst") && norm.contains("=t.Object(");
    if !exports_typebox_const { return; }

    if norm.contains(".static") { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-model-export-types".into(),
        message: "Module exports a `t.Object(...)` schema but no `typeof X.static` type — consumers cannot annotate variables with the model type.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_framework(source, &Check, "elysia")
    }

    #[test]
    fn flags_schema_without_static_type() {
        let src = "import { t } from 'elysia';\nexport const User = t.Object({ id: t.Number() });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_schema_with_static_type() {
        let src = "import { t } from 'elysia';\nexport const User = t.Object({ id: t.Number() });\nexport type User = typeof User.static;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_file_with_no_typebox_export() {
        let src = "import { Elysia } from 'elysia';\nexport const app = new Elysia();";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "export const User = t.Object({ id: t.Number() });";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
