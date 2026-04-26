//! react-no-access-state-in-setstate AST backend.
//!
//! Flags `this.state` inside `this.setState(...)` calls. Reading
//! `this.state` inside `setState` may yield stale values because React
//! batches state updates.

use crate::diagnostic::{Diagnostic, Severity};

/// Check if a node or any descendant contains `this.state`.
fn contains_this_state(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() == "member_expression"
        && let Ok(text) = node.utf8_text(source)
        && text == "this.state"
    {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if contains_this_state(child, source) {
            return true;
        }
    }
    false
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    // Look for call_expression: `this.setState(...)`
    let Some(callee) = node.child_by_field_name("function") else { return };
    let Ok(callee_text) = callee.utf8_text(source) else { return };
    if callee_text != "this.setState" {
        return;
    }

    // Check the arguments for `this.state`
    let Some(args) = node.child_by_field_name("arguments") else { return };
    if contains_this_state(args, source) {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "react-no-access-state-in-setstate".into(),
            message: "`this.state` inside `setState()` reads stale \
                      state. Use the updater callback: \
                      `setState(prev => ...)`."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_this_state_in_setstate() {
        let src = "this.setState({ count: this.state.count + 1 });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_multiline_setstate() {
        let src = r#"
this.setState({
    count: this.state.count + 1,
});
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_updater_callback() {
        let src = "this.setState(prev => ({ count: prev.count + 1 }));";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_no_setstate() {
        let src = "const x = this.state.count;";
        assert!(run(src).is_empty());
    }
}
