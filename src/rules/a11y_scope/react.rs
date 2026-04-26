//! a11y-scope AST backend.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["jsx_opening_element", "jsx_self_closing_element"] => |node, source, ctx, diagnostics|
    let Some(tag) = crate::rules::jsx::jsx_element_tag_name(node, source) else { return };

    // scope is only valid on <th>
    if tag == "th" {
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" {
            continue;
        }
        let Some(attr_name) = child.child(0) else { continue };
        let Ok(name) = attr_name.utf8_text(source) else { continue };
        if name == "scope" {
            let pos = child.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "a11y-scope".into(),
                message: "`scope` attribute should only be used on `<th>` elements.".into(),
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
    fn flags_scope_on_td() {
        assert_eq!(run(r#"const x = <td scope="row">Name</td>;"#).len(), 1);
    }

    #[test]
    fn flags_scope_on_div() {
        assert_eq!(run(r#"const x = <div scope="col">Header</div>;"#).len(), 1);
    }

    #[test]
    fn allows_scope_on_th() {
        assert!(run(r#"const x = <th scope="col">Name</th>;"#).is_empty());
    }
}
