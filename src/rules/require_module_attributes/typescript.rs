//! require-module-attributes AST backend.
//!
//! Flags import/export statements with empty `with {}` attribute clauses.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["import_statement", "export_statement"] => |node, source, ctx, diagnostics|
    let kind = node.kind();
    // Look for a child that is an import attribute clause (`with { ... }`)
    // In tree-sitter-typescript this appears as `import_attribute` nodes
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        // The `with {}` clause is represented differently across TS grammar versions.
        // We look for the text pattern in the full node source.
        if child.kind() == "import_attribute" {
            // Check if the attribute block is empty — only whitespace between braces
            let Ok(text) = child.utf8_text(source) else { continue };
            let trimmed = text.trim();
            // An empty import attribute looks like `with {}` or `with {  }`
            if trimmed == "with {}" || (trimmed.starts_with("with") && trimmed.ends_with('}')) {
                // Verify the braces contain only whitespace
                if let Some(open) = trimmed.find('{') {
                    let inner = &trimmed[open + 1..trimmed.len() - 1];
                    if inner.trim().is_empty() {
                        let stmt_type = if kind == "import_statement" { "import" } else { "export" };
                        let pos = child.start_position();
                        diagnostics.push(Diagnostic {
                            path: std::sync::Arc::clone(&ctx.path_arc),
                            line: pos.row + 1,
                            column: pos.column + 1,
                            rule_id: "require-module-attributes".into(),
                            message: format!(
                                "{stmt_type} statement has an empty `with {{}}` clause \u{2014} \
                                 add the required attributes or remove the clause."
                            ),
                            severity: Severity::Warning,
                            span: None,
                        });
                        return;
                    }
                }
            }
        }
    }

    // Fallback: scan the node text for `with {}` / `with {  }` patterns
    // (handles grammar versions where the attribute isn't a named child)
    let Ok(node_text) = node.utf8_text(source) else { return };
    if let Some(with_pos) = node_text.find(" with ") {
        let after_with = node_text[with_pos + 6..].trim();
        if let Some(rest) = after_with.strip_prefix('{') {
            let after_brace = rest.trim_start();
            if after_brace.starts_with('}') {
                let stmt_type = if kind == "import_statement" { "import" } else { "export" };
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "require-module-attributes".into(),
                    message: format!(
                        "{stmt_type} statement has an empty `with {{}}` clause \u{2014} \
                         add the required attributes or remove the clause."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
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
    fn flags_import_with_empty_attributes() {
        let diags = run_on("import data from './data.json' with {};");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("import"));
    }

    #[test]
    fn flags_export_with_empty_attributes() {
        let diags = run_on("export { foo } from './bar' with {};");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("export"));
    }

    #[test]
    fn allows_import_with_attributes() {
        assert!(run_on("import data from './data.json' with { type: 'json' };").is_empty());
    }

    #[test]
    fn allows_import_without_with_clause() {
        assert!(run_on("import { foo } from './foo';").is_empty());
    }
}
