//! node-exports-style backend — enforce `module.exports` over bare `exports`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "assignment_expression" {
        return;
    }

    let Some(left) = node.child_by_field_name("left") else { return };

    // Flag direct `exports = ...` or `exports.foo = ...`.
    if left.kind() == "identifier" && left.utf8_text(source).unwrap_or("") == "exports" {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "node-exports-style".into(),
            message: "Unexpected assignment to `exports`. Use `module.exports` instead.".into(),
            severity: Severity::Warning,
        });
        return;
    }

    if left.kind() == "member_expression" {
        let Some(obj) = left.child_by_field_name("object") else { return };
        if obj.kind() == "identifier" && obj.utf8_text(source).unwrap_or("") == "exports" {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "node-exports-style".into(),
                message: "Unexpected access to `exports`. Use `module.exports` instead.".into(),
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
    fn flags_bare_exports_assignment() {
        let d = run_on("exports = { foo: 1 };");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("module.exports"));
    }

    #[test]
    fn flags_exports_property_assignment() {
        let d = run_on("exports.foo = 42;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_module_exports() {
        assert!(run_on("module.exports = { foo: 1 };").is_empty());
    }
}
