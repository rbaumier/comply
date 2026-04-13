//! node-global-require backend — require() must be at module top level.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "identifier" || callee.utf8_text(source).unwrap_or("") != "require" {
        return;
    }

    // Walk up the tree: require is OK if all ancestors are "acceptable"
    // (program, variable_declaration, variable_declarator, expression_statement,
    //  assignment_expression, member_expression, call_expression, arguments).
    let mut current = node.parent();
    let mut in_function = false;
    while let Some(ancestor) = current {
        let ak = ancestor.kind();
        if ak == "function_declaration"
            || ak == "function"
            || ak == "arrow_function"
            || ak == "method_definition"
            || ak == "if_statement"
            || ak == "for_statement"
            || ak == "for_in_statement"
            || ak == "while_statement"
            || ak == "try_statement"
            || ak == "catch_clause"
            || ak == "switch_case"
        {
            in_function = true;
            break;
        }
        current = ancestor.parent();
    }

    if !in_function {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "node-global-require".into(),
        message: "Unexpected `require()`. Move it to the top-level module scope.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_require_in_function() {
        let d = run_on("function init() { const x = require('fs'); }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("top-level"));
    }

    #[test]
    fn flags_require_in_if() {
        let d = run_on("if (true) { const x = require('fs'); }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_top_level_require() {
        assert!(run_on("const fs = require('fs');").is_empty());
    }
}
