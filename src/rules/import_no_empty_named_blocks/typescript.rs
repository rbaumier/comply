//! import-no-empty-named-blocks backend — forbid `import { } from '...'`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "import_statement" {
        return;
    }

    let text = node.utf8_text(source).unwrap_or("");

    // Detect `{ }` or `{}` pattern indicating empty named imports.
    // Must have braces but no identifiers between them.
    if let Some(open) = text.find('{')
        && let Some(close) = text[open..].find('}') {
            let between = &text[open + 1..open + close];
            let trimmed = between.trim();
            if trimmed.is_empty() {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "import-no-empty-named-blocks".into(),
                    message: "Unexpected empty named import block.".into(),
                    severity: Severity::Warning,
                    span: None,
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
    fn flags_empty_braces() {
        let d = run_on("import { } from 'foo';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("empty named import"));
    }

    #[test]
    fn flags_empty_braces_no_space() {
        let d = run_on("import {} from 'foo';");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_named_imports() {
        assert!(run_on("import { foo } from 'bar';").is_empty());
    }
}
