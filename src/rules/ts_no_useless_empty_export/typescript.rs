//! ts-no-useless-empty-export backend — flag `export {}` statements when
//! the file already has other export or import statements.
//!
//! Detection: walk the program root, collect empty export statements
//! (`export_statement` with an `export_clause` containing no specifiers)
//! and check for the presence of other export/import statements.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }

    let mut empty_export_positions: Vec<(usize, usize)> = Vec::new();
    let mut has_real_export = false;
    let mut cursor = node.walk();

    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "export_statement" => {
                // Check if this is `export {}`
                let mut inner_cursor = child.walk();
                let named_children: Vec<_> = child.named_children(&mut inner_cursor).collect();

                // `export {}` has one named child: an export_clause with no specifiers
                if named_children.len() == 1 && named_children[0].kind() == "export_clause" {
                    let clause = named_children[0];
                    let mut clause_cursor = clause.walk();
                    let specifier_count = clause.named_children(&mut clause_cursor).count();
                    if specifier_count == 0 {
                        let pos = child.start_position();
                        empty_export_positions.push((pos.row + 1, pos.column + 1));
                        continue;
                    }
                }
                has_real_export = true;
            }
            "import_statement" => {
                has_real_export = true;
            }
            _ => {}
        }
    }

    if !has_real_export {
        return;
    }

    for (line, col) in empty_export_positions {
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line,
            column: col,
            rule_id: "ts-no-useless-empty-export".into(),
            message: "`export {}` is unnecessary — the file already has other exports."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_empty_export_with_other_exports() {
        let diags = run_on("export const x = 1;\nexport {};");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_empty_export_as_only_export() {
        assert!(run_on("const x = 1;\nexport {};").is_empty());
    }

    #[test]
    fn flags_empty_export_with_import() {
        let diags = run_on("import { foo } from 'bar';\nexport {};");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_no_empty_export() {
        assert!(run_on("export const x = 1;").is_empty());
    }
}
