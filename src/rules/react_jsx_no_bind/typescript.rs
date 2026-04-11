//! react-jsx-no-bind AST backend.
//!
//! Flags `.bind()` calls and arrow functions used directly as JSX prop values.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "jsx_attribute" {
        return;
    }

    // Get the value expression.
    let Some(val_node) = node.child(2) else { return };

    // Value is usually wrapped in a jsx_expression: { expr }
    if val_node.kind() != "jsx_expression" {
        return;
    }

    let mut cursor = val_node.walk();
    for child in val_node.children(&mut cursor) {
        match child.kind() {
            "arrow_function" => {
                let pos = child.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "react-jsx-no-bind".into(),
                    message: "Arrow function in JSX prop creates a new function \
                              on every render — extract to a stable reference."
                        .into(),
                    severity: Severity::Warning,
                });
            }
            "call_expression" => {
                // Check if it's a .bind() call.
                let Some(callee) = child.child(0) else { continue };
                if callee.kind() == "member_expression"
                    && let Some(prop) = callee.child_by_field_name("property") {
                        let Ok(method_name) = prop.utf8_text(source) else { continue };
                        if method_name == "bind" {
                            let pos = child.start_position();
                            diagnostics.push(Diagnostic {
                                path: ctx.path.to_path_buf(),
                                line: pos.row + 1,
                                column: pos.column + 1,
                                rule_id: "react-jsx-no-bind".into(),
                                message: "`.bind()` in JSX prop creates a new \
                                          function on every render — extract to \
                                          a stable reference."
                                    .into(),
                                severity: Severity::Warning,
                            });
                        }
                    }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_bind_in_prop() {
        let src = "const x = <button onClick={handler.bind(this)} />;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_arrow_in_prop() {
        let src = "const x = <button onClick={() => doSomething()} />;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_reference() {
        let src = "const x = <button onClick={handleClick} />;";
        assert!(run(src).is_empty());
    }
}
