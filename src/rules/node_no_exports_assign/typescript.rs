//! node-no-exports-assign backend — disallow `exports = ...`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["assignment_expression"] => |node, source, ctx, diagnostics|
    let Some(left) = node.child_by_field_name("left") else { return };

    // Only flag `exports = ...` (direct assignment to the exports variable).
    if left.kind() != "identifier" || left.utf8_text(source).unwrap_or("") != "exports" {
        return;
    }

    // Allow `module.exports = exports = {}` pattern:
    // if parent is also an assignment whose left is `module.exports`, skip.
    if let Some(parent) = node.parent()
        && parent.kind() == "assignment_expression"
            && let Some(pleft) = parent.child_by_field_name("left") {
                let pleft_text = pleft.utf8_text(source).unwrap_or("");
                if pleft_text == "module.exports" {
                    return;
                }
            }

    // Allow `exports = module.exports = {}` pattern.
    if let Some(right) = node.child_by_field_name("right")
        && right.kind() == "assignment_expression"
            && let Some(rleft) = right.child_by_field_name("left") {
                let rleft_text = rleft.utf8_text(source).unwrap_or("");
                if rleft_text == "module.exports" {
                    return;
                }
            }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "node-no-exports-assign".into(),
        message: "Unexpected assignment to `exports` variable. Use `module.exports` instead.".into(),
        severity: Severity::Error,
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
    fn flags_exports_assignment() {
        let d = run_on("exports = { foo: 1 };");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("module.exports"));
    }

    #[test]
    fn allows_module_exports() {
        assert!(run_on("module.exports = { foo: 1 };").is_empty());
    }

    #[test]
    fn allows_exports_property() {
        // `exports.foo = 1` is setting a property, not reassigning `exports` itself.
        assert!(run_on("exports.foo = 1;").is_empty());
    }
}
