//! react-style-prop-object AST backend.
//!
//! Flags `style="..."` in JSX — React expects the `style` prop to be an
//! object, not a CSS string.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["jsx_attribute"] => |node, source, ctx, diagnostics|
    // Match jsx_attribute nodes named "style"
    let Some(name_node) = node.child(0) else { return };
    let Ok(name_text) = name_node.utf8_text(source) else { return };
    if name_text != "style" {
        return;
    }

    // Check if the value is a string literal (not an expression).
    let Some(value_node) = crate::rules::jsx::jsx_attribute_value(node) else { return };
    if value_node.kind() == "string" || value_node.kind() == "string_fragment" {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "react-style-prop-object".into(),
            message: "The `style` prop expects a JavaScript object, \
                      not a CSS string. Use `style={{ ... }}` instead."
                .into(),
            severity: Severity::Error,
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
    fn flags_string_style() {
        let src = r#"const x = <div style="color: red">hello</div>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_single_quote_style() {
        let src = "const x = <div style='color: red'>hello</div>;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_object_style() {
        let src = r#"const x = <div style={{ color: "red" }}>hello</div>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_variable_style() {
        let src = "const x = <div style={myStyles}>hello</div>;";
        assert!(run(src).is_empty());
    }
}
