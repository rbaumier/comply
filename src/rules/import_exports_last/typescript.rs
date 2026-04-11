//! import-exports-last backend — export statements must appear after all other statements.

use crate::diagnostic::{Diagnostic, Severity};

fn is_export_node(kind: &str) -> bool {
    kind == "export_statement"
        || kind == "export_default_declaration"
}

crate::ast_check! { |node, source, ctx, diagnostics|
    // Only inspect the top-level program node.
    if node.kind() != "program" {
        return;
    }

    // Collect children: track the index of the last non-export statement.
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();

    let mut last_non_export_idx: Option<usize> = None;
    for (i, child) in children.iter().enumerate() {
        if !is_export_node(child.kind()) && child.kind() != "comment" {
            last_non_export_idx = Some(i);
        }
    }

    let Some(last_ne) = last_non_export_idx else { return };

    // Any export statement before the last non-export statement is a violation.
    for (i, child) in children.iter().enumerate() {
        if i >= last_ne {
            break;
        }
        if is_export_node(child.kind()) {
            let pos = child.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "import-exports-last".into(),
                message: "Export statements should appear at the end of the file.".into(),
                severity: Severity::Warning,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_export_before_statement() {
        let src = "export const a = 1;\nconst b = 2;\n";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("end of the file"));
    }

    #[test]
    fn allows_exports_at_end() {
        let src = "const b = 2;\nexport const a = 1;\n";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_only_exports() {
        let src = "export const a = 1;\nexport const b = 2;\n";
        assert!(run_on(src).is_empty());
    }
}
