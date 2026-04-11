//! no-small-switch Rust backend — match with fewer than 3 arms.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, _source, ctx, diagnostics|
    if node.kind() != "match_expression" {
        return;
    }

    // Count match_arm children inside the match_body (match_block).
    let Some(body) = node.child_by_field_name("body") else { return };
    let mut arm_count = 0u32;
    let child_count = body.named_child_count();
    for i in 0..child_count {
        if let Some(child) = body.named_child(i)
            && child.kind() == "match_arm" {
                arm_count += 1;
        }
    }

    if arm_count < 3 {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-small-switch".into(),
            message: format!(
                "`match` has only {} arm(s) \u{2014} use `if/else` instead.",
                arm_count
            ),
            severity: Severity::Warning,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_match_with_two_arms() {
        let src = r#"
fn f(x: i32) {
    match x {
        1 => println!("one"),
        _ => println!("other"),
    }
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-small-switch");
    }

    #[test]
    fn flags_match_with_one_arm() {
        let src = r#"
fn f(x: i32) {
    match x {
        _ => println!("always"),
    }
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_match_with_three_arms() {
        let src = r#"
fn f(x: i32) {
    match x {
        1 => println!("one"),
        2 => println!("two"),
        _ => println!("other"),
    }
}
"#;
        assert!(run_on(src).is_empty());
    }
}
