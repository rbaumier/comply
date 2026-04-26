//! react-no-string-refs AST backend.
//!
//! Flags `ref="stringValue"` on JSX elements (string refs are deprecated).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["jsx_self_closing_element", "jsx_opening_element"] => |node, source, ctx, diagnostics|
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" {
            continue;
        }
        let Some(attr_name) = child.child(0) else { continue };
        let Ok(name_text) = attr_name.utf8_text(source) else { continue };
        if name_text != "ref" {
            continue;
        }
        // Check if the value is a string literal.
        let Some(val_node) = child.child(2) else { continue };
        if val_node.kind() == "string" {
            let pos = child.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "react-no-string-refs".into(),
                message: "String refs are deprecated — use `useRef()` or a \
                          callback ref instead."
                    .into(),
                severity: Severity::Error,
                span: None,
            });
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
    fn flags_string_ref() {
        let src = r#"const x = <input ref="myInput" />;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_callback_ref() {
        let src = "const x = <input ref={el => { inputRef.current = el; }} />;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_ref_object() {
        let src = "const x = <input ref={myRef} />;";
        assert!(run(src).is_empty());
    }
}
