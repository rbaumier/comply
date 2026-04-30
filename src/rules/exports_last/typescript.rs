//! exports-last backend — flag exports that precede non-export top-level
//! statements.
//!
//! Walks the `program` node once, collects its named children, and flags
//! every `export_statement` that is followed by a non-export statement.
//! Comments are not statement nodes (they're extras in tree-sitter), so
//! interspersed comments naturally don't break the "exports at the end"
//! requirement.

use crate::diagnostic::{Diagnostic, Severity};

fn is_reexport(node: tree_sitter::Node) -> bool {
    if node.kind() != "export_statement" { return false; }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "export_clause" {
            return true;
        }
    }
    false
}

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    let _ = source;
    let mut cursor = node.walk();
    let children: Vec<_> = node
        .named_children(&mut cursor)
        .filter(|c| c.kind() != "comment")
        .collect();

    let has_reexports = children.iter().any(|c| is_reexport(*c));
    if !has_reexports { return; }

    let last_non_export_idx = children
        .iter()
        .enumerate()
        .rev()
        .find(|(_, c)| c.kind() != "export_statement")
        .map(|(i, _)| i);

    let Some(last_non_export_idx) = last_non_export_idx else { return };

    for (i, child) in children.iter().enumerate() {
        if i >= last_non_export_idx { break; }
        if !is_reexport(*child) { continue; }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            child,
            super::META.id,
            "Re-export statement is not at the end of the file.".into(),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn allows_inline_export_before_code() {
        let src = "export const x = 1;\nconst y = 2;\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_reexport_before_code() {
        let src = "export { x };\nconst y = 2;\n";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_all_exports_at_end() {
        let src = "const y = 2;\nexport const x = 1;\nexport const z = 3;\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_inline_exports_only() {
        let src = "export function a() {}\nconst b = 1;\nexport function c() {}\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_file_with_no_exports() {
        assert!(run("const x = 1;\n").is_empty());
    }

    #[test]
    fn allows_comment_after_exports() {
        let src = "const y = 2;\nexport const x = 1;\n// tail comment\n";
        assert!(run(src).is_empty());
    }
}
