//! import-no-dynamic-require backend — forbid non-literal require() arguments.

use crate::diagnostic::{Diagnostic, Severity};

/// Returns true if the node is a static string value (string literal or
/// template literal with no expressions).
fn is_static_value(kind: &str) -> bool {
    kind == "string" || kind == "template_string"
}

crate::ast_check! { on ["call_expression"] prefilter = ["require"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_name = callee.utf8_text(source).unwrap_or("");

    if callee.kind() != "identifier" || callee_name != "require" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let first_arg = args.children(&mut cursor)
        .find(|c| c.kind() != "(" && c.kind() != ")" && c.kind() != ",");

    let Some(arg) = first_arg else { return };

    // Template strings with expressions (template_substitution children) are dynamic.
    if arg.kind() == "template_string" {
        let mut sub_cursor = arg.walk();
        let has_expr = arg.children(&mut sub_cursor)
            .any(|c| c.kind() == "template_substitution");
        if !has_expr {
            return; // static template string
        }
    } else if is_static_value(arg.kind()) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "import-no-dynamic-require".into(),
        message: "Calls to `require()` should use string literals.".into(),
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
    fn flags_dynamic_require() {
        let d = run_on("const x = require(getPath());");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("string literals"));
    }

    #[test]
    fn flags_variable_require() {
        let d = run_on("const x = require(moduleName);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_static_require() {
        assert!(run_on("const x = require('fs');").is_empty());
    }
}
