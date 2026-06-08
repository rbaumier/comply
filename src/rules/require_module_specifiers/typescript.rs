//! require-module-specifiers AST backend.
//!
//! Flags import/export statements with empty specifier lists `{}`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["import_statement", "export_statement"] => |node, source, ctx, diagnostics|
    let kind = node.kind();
    let stmt_type = if kind == "import_statement" { "import" } else { "export" };

    // For import statements: look for an import clause with an empty named_imports
    // For export statements: look for an export clause with empty named specifiers
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        // import_clause → named_imports → `{ }`
        if child.kind() == "import_clause" {
            let mut inner_cursor = child.walk();
            for ic_child in child.named_children(&mut inner_cursor) {
                if ic_child.kind() == "named_imports" && ic_child.named_child_count() == 0 {
                    let pos = node.start_position();
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: "require-module-specifiers".into(),
                        message: format!(
                            "{stmt_type} statement with empty specifiers `{{}}` is not \
                             allowed \u{2014} add specifiers, use a side-effect import, or \
                             remove the statement."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                    return;
                }
            }
        }

        // For export: export_clause → `{ }`
        if child.kind() == "export_clause" && child.named_child_count() == 0 {
            // Must have a `from` source to flag — bare `export {}` is weird but
            // we care about re-export patterns like `export {} from './mod'`
            let has_source = node.named_children(&mut node.walk())
                .any(|c| c.kind() == "string");
            if has_source {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "require-module-specifiers".into(),
                    message: format!(
                        "{stmt_type} statement with empty specifiers `{{}}` is not \
                         allowed \u{2014} add specifiers, use a side-effect import, or \
                         remove the statement."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
                return;
            }
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
    fn flags_import_with_empty_specifiers() {
        let diags = run_on("import {} from './module';");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("import"));
    }

    #[test]
    fn flags_export_with_empty_specifiers() {
        let diags = run_on("export {} from './module';");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("export"));
    }

    #[test]
    fn allows_import_with_specifiers() {
        assert!(run_on("import { foo } from './module';").is_empty());
    }

    #[test]
    fn allows_side_effect_import() {
        assert!(run_on("import './module';").is_empty());
    }

    #[test]
    fn allows_default_import() {
        assert!(run_on("import foo from './module';").is_empty());
    }
}
