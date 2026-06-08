//! no-mutable-exports backend — flag `export let` / `export var`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["export_statement"] => |node, source, ctx, diagnostics|
    let Some(decl) = node.child_by_field_name("declaration") else {
        return;
    };
    let kind = match decl.kind() {
        "lexical_declaration" => {
            // Check if it's `let` (not `const`)
            let text = match decl.utf8_text(source) {
                Ok(t) => t,
                Err(_) => return,
            };
            if text.trim_start().starts_with("let ") {
                "let"
            } else {
                return;
            }
        }
        "variable_declaration" => "var",
        _ => return,
    };
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-mutable-exports".into(),
        message: format!(
            "Exporting mutable `{}` binding — use `export const` instead.",
            kind
        ),
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_export_let() {
        let d = run_on("export let count = 0;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`let`"));
    }

    #[test]
    fn flags_export_var() {
        let d = run_on("export var name = 'x';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`var`"));
    }

    #[test]
    fn allows_export_const() {
        assert!(run_on("export const MAX = 10;").is_empty());
    }
}
