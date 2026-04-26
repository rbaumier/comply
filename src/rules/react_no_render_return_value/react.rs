//! react-no-render-return-value backend — detect `ReactDOM.render(...)`
//! whose result is captured (assigned, returned, awaited, etc.).

use crate::diagnostic::{Diagnostic, Severity};

/// True if `call` is `ReactDOM.render(...)`.
fn is_reactdom_render(call: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(func) = call.child_by_field_name("function") else { return false };
    if func.kind() != "member_expression" {
        return false;
    }
    let Some(object) = func.child_by_field_name("object") else { return false };
    let Some(property) = func.child_by_field_name("property") else { return false };
    &source[object.byte_range()] == b"ReactDOM" && &source[property.byte_range()] == b"render"
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !is_reactdom_render(node, source) {
        return;
    }
    let Some(parent) = node.parent() else { return };
    // A standalone statement is fine: `ReactDOM.render(...)` as a line.
    if parent.kind() == "expression_statement" {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "react-no-render-return-value".into(),
        message: "Do not use the return value of `ReactDOM.render()`.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_assignment() {
        let d = run_on("const app = ReactDOM.render(<App />, root);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_return() {
        let src = "function mount() { return ReactDOM.render(<App />, root); }";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_reassignment() {
        let src = "let app;\napp = ReactDOM.render(<App />, root);";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_expression_statement() {
        assert!(run_on("ReactDOM.render(<App />, root);").is_empty());
    }

    #[test]
    fn ignores_non_reactdom_render() {
        assert!(run_on("const x = other.render(<App />);").is_empty());
    }
}
