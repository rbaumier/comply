//! no-duplicated-branches Rust backend.
//!
//! Flag if/else branches with identical bodies. Also checks match arms.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    match node.kind() {
        "if_expression" => check_if_branches(node, source, ctx, diagnostics),
        "match_expression" => check_match_arms(node, source, ctx, diagnostics),
        _ => {}
    }
}

fn check_if_branches(
    node: tree_sitter::Node,
    source: &[u8],
    ctx: &crate::rules::backend::CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Only process the outermost if in a chain.
    if let Some(parent) = node.parent()
        && parent.kind() == "else_clause" {
            return;
        }

    let mut bodies: Vec<(usize, String)> = Vec::new();
    collect_if_bodies(node, source, &mut bodies);

    if bodies.len() < 2 {
        return;
    }

    for i in 0..bodies.len() {
        for j in (i + 1)..bodies.len() {
            if !bodies[i].1.is_empty() && bodies[i].1 == bodies[j].1 {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: bodies[j].0,
                    column: 1,
                    rule_id: "no-duplicated-branches".into(),
                    message: "This branch has the same body as another branch \u{2014} merge conditions or remove the duplicate.".into(),
                    severity: Severity::Warning,
                });
            }
        }
    }
}

fn collect_if_bodies(
    node: tree_sitter::Node,
    source: &[u8],
    bodies: &mut Vec<(usize, String)>,
) {
    // Get the consequence (body) of this if.
    if let Some(body) = node.child_by_field_name("consequence") {
        let line = body.start_position().row + 1;
        let text = body_text(&body, source);
        bodies.push((line, text));
    }

    // Check for alternative (else/else-if).
    if let Some(alt) = node.child_by_field_name("alternative") {
        let mut cursor = alt.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                match child.kind() {
                    "if_expression" => {
                        collect_if_bodies(child, source, bodies);
                        return;
                    }
                    "block" => {
                        let line = child.start_position().row + 1;
                        let text = body_text(&child, source);
                        bodies.push((line, text));
                        return;
                    }
                    _ => {}
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }
}

fn check_match_arms(
    node: tree_sitter::Node,
    source: &[u8],
    ctx: &crate::rules::backend::CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut arm_bodies: Vec<(usize, String)> = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "match_arm"
            && let Some(body) = child.child_by_field_name("value") {
                let line = body.start_position().row + 1;
                if let Ok(text) = body.utf8_text(source) {
                    let normalized = text.trim().to_string();
                    if !normalized.is_empty() {
                        arm_bodies.push((line, normalized));
                    }
                }
            }
    }

    if arm_bodies.len() < 2 {
        return;
    }

    for i in 0..arm_bodies.len() {
        for j in (i + 1)..arm_bodies.len() {
            if arm_bodies[i].1 == arm_bodies[j].1 {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: arm_bodies[j].0,
                    column: 1,
                    rule_id: "no-duplicated-branches".into(),
                    message: "This match arm has the same body as another arm \u{2014} merge patterns or remove the duplicate.".into(),
                    severity: Severity::Warning,
                });
            }
        }
    }
}

fn body_text(node: &tree_sitter::Node, source: &[u8]) -> String {
    let mut parts = Vec::new();
    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i)
            && let Ok(t) = child.utf8_text(source) {
                parts.push(t.trim().to_string());
            }
    }
    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_duplicate_if_else() {
        let src = r#"fn f() {
    if a {
        do_something();
    } else {
        do_something();
    }
}"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_different_branches() {
        let src = r#"fn f() {
    if a {
        foo();
    } else {
        bar();
    }
}"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_single_branch() {
        let src = r#"fn f() {
    if a {
        foo();
    }
}"#;
        assert!(run_on(src).is_empty());
    }
}
