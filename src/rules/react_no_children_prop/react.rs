//! react-no-children-prop AST backend.
//!
//! Flags `<Foo children={...} />` or `<Foo children="..." />`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["jsx_self_closing_element", "jsx_opening_element"] => |node, source, ctx, diagnostics|
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" {
            continue;
        }
        let Some(attr_name) = child.child(0) else { continue };
        let Ok(name_text) = attr_name.utf8_text(source) else { continue };
        if name_text == "children" {
            let pos = child.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "react-no-children-prop".into(),
                message: "Pass children between tags instead of as a \
                          `children` prop."
                    .into(),
                severity: Severity::Warning,
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
    fn flags_children_prop() {
        let src = r#"const x = <Foo children="bar" />;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_children_prop_expression() {
        let src = "const x = <Foo children={<Bar />} />;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_nested_children() {
        let src = "const x = <Foo>bar</Foo>;";
        assert!(run(src).is_empty());
    }
}
