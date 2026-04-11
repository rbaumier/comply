//! react-jsx-no-useless-fragment AST backend.
//!
//! Flags `<><child/></>` or `<Fragment><child/></Fragment>` when there is
//! zero or one child (the fragment is unnecessary).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "jsx_element" {
        return;
    }

    let Some(opening) = node.child(0) else { return };
    if opening.kind() != "jsx_opening_element" && opening.kind() != "jsx_fragment" {
        // Not an element with open+close.
    }

    // Check if this is a Fragment: either <Fragment> or <> (jsx_fragment parent).
    let is_fragment = if opening.kind() == "jsx_opening_element" {
        if let Some(name_node) = opening.child_by_field_name("name") {
            let Ok(tag) = name_node.utf8_text(source) else { return };
            tag == "Fragment" || tag == "React.Fragment"
        } else {
            false
        }
    } else {
        false
    };

    // Also handle jsx_fragment directly.
    let is_jsx_fragment = node.kind() == "jsx_fragment";

    if !is_fragment && !is_jsx_fragment {
        return;
    }

    // Count meaningful children.
    let mut cursor = node.walk();
    let meaningful_children: usize = node
        .children(&mut cursor)
        .filter(|child| {
            match child.kind() {
                "jsx_opening_element" | "jsx_closing_element"
                | "jsx_opening_fragment" | "jsx_closing_fragment" => false,
                "jsx_text" => {
                    let Ok(text) = child.utf8_text(source) else { return false };
                    !text.trim().is_empty()
                }
                _ => true,
            }
        })
        .count();

    if meaningful_children <= 1 {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "react-jsx-no-useless-fragment".into(),
            message: "Unnecessary fragment — a fragment wrapping zero or one \
                      child adds no value."
                .into(),
            severity: Severity::Warning,
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
    fn flags_fragment_with_single_child() {
        let src = "const x = <Fragment><div>hi</div></Fragment>;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_fragment_with_multiple_children() {
        let src = "const x = <Fragment><div>a</div><div>b</div></Fragment>;";
        assert!(run(src).is_empty());
    }
}
