//! newline-after-import backend — flag the last import statement when the
//! next top-level statement is on the immediately following line, with no
//! blank line in between.
//!
//! Walk the `program` root, find the last `import_statement` child, then
//! locate the next non-import named child. Compare row positions: if the
//! next statement starts on `last_import_end_row + 1` we flag.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    let _ = source;
    if node.kind() != "program" {
        return;
    }

    let mut cursor = node.walk();
    let children: Vec<_> = node.named_children(&mut cursor).collect();

    // Locate the index of the last `import_statement` among top-level children.
    let Some(last_import_idx) = children
        .iter()
        .enumerate()
        .rev()
        .find(|(_, c)| c.kind() == "import_statement")
        .map(|(i, _)| i)
    else {
        return;
    };

    let last_import = children[last_import_idx];

    // Find the next named non-comment child after the last import.
    let next = children
        .iter()
        .skip(last_import_idx + 1)
        .find(|c| c.kind() != "comment");
    let Some(&next) = next else {
        return;
    };

    // The last import ends at `end_position().row`; a blank line means the
    // next statement starts at least two rows after that. If it starts on
    // the row immediately following, there is no blank line separator.
    let import_end_row = last_import.end_position().row;
    let next_start_row = next.start_position().row;
    if next_start_row == import_end_row + 1 {
        let pos = last_import.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: 1,
            rule_id: "newline-after-import".into(),
            message: "Expected a blank line after the last import statement.".into(),
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
    fn flags_missing_newline() {
        let src = "import { a } from 'a';\nconst x = 1;\n";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 1);
    }

    #[test]
    fn allows_blank_line_after_import() {
        let src = "import { a } from 'a';\n\nconst x = 1;\n";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_consecutive_imports_without_blank() {
        let src = "import { a } from 'a';\nimport { b } from 'b';\n\nconst x = 1;\n";
        assert!(run_on(src).is_empty());
    }
}
