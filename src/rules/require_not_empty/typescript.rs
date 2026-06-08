//! require-not-empty backend — flag empty string as import/require path.
//!
//! Detects `import x from ''` and `require('')` where the module specifier is
//! an empty string literal (single or double quoted). Empty specifiers cannot
//! resolve to any module and are always programmer errors.

use crate::diagnostic::{Diagnostic, Severity};

/// Returns true when a string-literal node (including its quote characters)
/// is exactly `''` or `""`.
fn is_empty_string_literal(text: &str) -> bool {
    matches!(text, "''" | "\"\"")
}

crate::ast_check! { on ["import_statement", "call_expression"] => |node, source, ctx, diagnostics|
match node.kind() {
        "import_statement" => {
            let Some(src) = node.child_by_field_name("source") else { return };
            let text = src.utf8_text(source).unwrap_or("");
            if !is_empty_string_literal(text) { return; }

            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "require-not-empty".into(),
                message: "Import specifier must not be an empty string.".into(),
                severity: Severity::Error,
                span: None,
            });
        }
        "call_expression" => {
            let Some(callee) = node.child_by_field_name("function") else { return };
            if callee.kind() != "identifier" { return; }
            if callee.utf8_text(source).unwrap_or("") != "require" { return; }

            let Some(args) = node.child_by_field_name("arguments") else { return };
            let mut cursor = args.walk();
            let first_arg = args
                .children(&mut cursor)
                .find(|c| c.kind() != "(" && c.kind() != ")" && c.kind() != ",");
            let Some(arg) = first_arg else { return };
            if arg.kind() != "string" { return; }

            let text = arg.utf8_text(source).unwrap_or("");
            if !is_empty_string_literal(text) { return; }

            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "require-not-empty".into(),
                message: "require() specifier must not be an empty string.".into(),
                severity: Severity::Error,
                span: None,
            });
        }
        _ => {}
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
    fn flags_empty_import_single_quotes() {
        let d = run_on("import x from '';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Import specifier"));
    }

    #[test]
    fn flags_empty_import_double_quotes() {
        let d = run_on("import x from \"\";");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_empty_require() {
        let d = run_on("const x = require('');");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("require()"));
    }

    #[test]
    fn allows_valid_import() {
        assert!(run_on("import x from 'fs';").is_empty());
    }

    #[test]
    fn allows_valid_require() {
        assert!(run_on("const x = require('fs');").is_empty());
    }
}
