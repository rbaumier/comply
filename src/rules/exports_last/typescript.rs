//! exports-last backend — flag exports that precede non-export top-level
//! statements.
//!
//! Walks the `program` node once, collects its named children, and flags
//! every `export_statement` that is followed by a non-export statement.
//! Comments are not statement nodes (they're extras in tree-sitter), so
//! interspersed comments naturally don't break the "exports at the end"
//! requirement.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    let _ = source;
    if node.kind() != "program" { return; }

    // Collect named top-level children, dropping `comment` nodes — they
    // are statement-shaped in tree-sitter (named children of `program`)
    // but conceptually trailing/leading commentary should not break the
    // "exports at the end" requirement.
    let mut cursor = node.walk();
    let children: Vec<_> = node
        .named_children(&mut cursor)
        .filter(|c| c.kind() != "comment")
        .collect();

    // Find the index of the first non-export statement that appears AFTER any
    // export. Any export whose index is less than the last non-export index
    // is misplaced.
    let last_non_export_idx = children
        .iter()
        .enumerate()
        .rev()
        .find(|(_, c)| c.kind() != "export_statement")
        .map(|(i, _)| i);

    let Some(last_non_export_idx) = last_non_export_idx else { return };

    for (i, child) in children.iter().enumerate() {
        if i >= last_non_export_idx { break; }
        if child.kind() != "export_statement" { continue; }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            child,
            super::META.id,
            "Export statement is not at the end of the file.".into(),
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
    fn flags_export_before_code() {
        let src = "export const x = 1;\nconst y = 2;\n";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 1);
    }

    #[test]
    fn allows_all_exports_at_end() {
        let src = "const y = 2;\nexport const x = 1;\nexport const z = 3;\n";
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
