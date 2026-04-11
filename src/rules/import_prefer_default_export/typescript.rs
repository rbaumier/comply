//! import-prefer-default-export backend — prefer default export on single-export files.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    // Only inspect the top-level program.
    if node.kind() != "program" {
        return;
    }

    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();

    let mut named_export_count = 0u32;
    let mut has_default_export = false;
    let mut has_star_export = false;
    let mut has_type_export = false;
    let mut last_named_export_row = 0usize;
    let mut last_named_export_col = 0usize;

    for child in &children {
        let kind = child.kind();
        if kind == "export_statement" {
            let text = child.utf8_text(source).unwrap_or("");
            if text.starts_with("export default") {
                has_default_export = true;
            } else if text.starts_with("export *") {
                has_star_export = true;
            } else if text.starts_with("export type ") || text.starts_with("export interface ") {
                has_type_export = true;
                named_export_count += 1;
            } else {
                named_export_count += 1;
                let pos = child.start_position();
                last_named_export_row = pos.row;
                last_named_export_col = pos.column;
            }
        } else if kind == "export_default_declaration" {
            has_default_export = true;
        }
    }

    if has_default_export || has_star_export || has_type_export {
        return;
    }

    if named_export_count == 1 {
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: last_named_export_row + 1,
            column: last_named_export_col + 1,
            rule_id: "import-prefer-default-export".into(),
            message: "Prefer default export on a file with single export.".into(),
            severity: Severity::Warning,
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
    fn flags_single_named_export() {
        let d = run_on("export const foo = 1;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Prefer default export"));
    }

    #[test]
    fn allows_default_export() {
        assert!(run_on("export default function foo() {}").is_empty());
    }

    #[test]
    fn allows_multiple_named_exports() {
        assert!(run_on("export const a = 1;\nexport const b = 2;").is_empty());
    }

    #[test]
    fn allows_type_export() {
        assert!(run_on("export type Foo = string;").is_empty());
    }
}
