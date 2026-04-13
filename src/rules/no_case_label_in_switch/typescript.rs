//! no-case-label-in-switch backend — flag label statements inside switch blocks.

use crate::diagnostic::{Diagnostic, Severity};

/// Check whether a node is inside a switch_body (recursing through parents).
fn inside_switch(node: tree_sitter::Node) -> bool {
    let mut current = node.parent();
    while let Some(p) = current {
        if p.kind() == "switch_body" {
            return true;
        }
        current = p.parent();
    }
    false
}

crate::ast_check! { |node, _source, ctx, diagnostics|
    if node.kind() != "labeled_statement" {
        return;
    }

    if !inside_switch(node) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-case-label-in-switch".into(),
        message: "Label inside switch statement \u{2014} this is a JS label, not a case branch. Use `case <value>:` instead.".into(),
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
    fn flags_label_in_switch() {
        let src = r#"
switch (action) {
    case "run":
        break;
    stop:
        console.log("stopped");
        break;
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_multiple_labels() {
        let src = r#"
switch (x) {
    case 1:
        break;
    foo:
        break;
    bar:
        break;
}
"#;
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn allows_case_and_default() {
        let src = r#"
switch (x) {
    case "a":
        break;
    case "b":
        break;
    default:
        break;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_labels_outside_switch() {
        let src = r#"
myLabel:
for (let i = 0; i < 10; i++) {
    break myLabel;
}
"#;
        assert!(run_on(src).is_empty());
    }
}
