//! node-no-mixed-requires backend — don't mix require() with other declarations.

use crate::diagnostic::{Diagnostic, Severity};

/// Check if a declarator's init is a `require(...)` call.
fn is_require_init(declarator: tree_sitter::Node, source: &[u8]) -> bool {
    if let Some(init) = declarator.child_by_field_name("value")
        && init.kind() == "call_expression"
            && let Some(callee) = init.child_by_field_name("function") {
                return callee.kind() == "identifier"
                    && callee.utf8_text(source).unwrap_or("") == "require";
            }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    // Match `const`/`let`/`var` declarations with multiple declarators.
    if node.kind() != "lexical_declaration" && node.kind() != "variable_declaration" {
        return;
    }

    let mut cursor = node.walk();
    let declarators: Vec<_> = node.children(&mut cursor)
        .filter(|c| c.kind() == "variable_declarator")
        .collect();

    if declarators.len() < 2 {
        return;
    }

    let mut has_require = false;
    let mut has_other = false;

    for decl in &declarators {
        if is_require_init(*decl, source) {
            has_require = true;
        } else {
            has_other = true;
        }
    }

    if has_require && has_other {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "node-no-mixed-requires".into(),
            message: "Do not mix `require` and other declarations.".into(),
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
    fn flags_mixed_declarations() {
        let d = run_on("var fs = require('fs'), foo = 42;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("mix"));
    }

    #[test]
    fn allows_only_requires() {
        assert!(run_on("var fs = require('fs'), path = require('path');").is_empty());
    }

    #[test]
    fn allows_only_non_requires() {
        assert!(run_on("var a = 1, b = 2;").is_empty());
    }
}
