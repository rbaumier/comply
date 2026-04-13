//! react-no-unescaped-entities AST backend.
//!
//! Flags `"`, `'`, `}` inside `jsx_text` nodes. Note: `>` in JSX text
//! is technically valid in tree-sitter (it does not close tags), but the
//! original eslint rule also flags it for consistency.

use crate::diagnostic::{Diagnostic, Severity};

const PROBLEMATIC: &[char] = &['"', '\'', '}'];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "jsx_text" {
        return;
    }

    let Ok(text) = node.utf8_text(source) else { return };

    for ch in PROBLEMATIC {
        if text.contains(*ch) {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "react-no-unescaped-entities".into(),
                message: format!(
                    "Unescaped `{ch}` in JSX text — use the HTML entity instead."
                ),
                severity: Severity::Warning,
                span: None,
            });
            // Report once per node, not once per character.
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_unescaped_quote() {
        let src = "const x = <div>She said &quot;hello&quot; and then \"bye\"</div>;";
        // The literal " in JSX text should be flagged.
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_unescaped_single_quote() {
        let src = "const x = <div>it's fine</div>;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_clean_text() {
        let src = "const x = <div>Hello world</div>;";
        assert!(run(src).is_empty());
    }
}
