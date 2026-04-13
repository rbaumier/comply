//! comma-or-logical-or-case Rust backend — flag `match` arms that use `|`
//! with `||` by mistake (logical OR instead of pattern alternative).
//!
//! In Rust, match arm alternatives use `|`, but developers coming from
//! other languages might accidentally write `||` which compiles but
//! means something different (logical OR, producing a boolean pattern).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "match_arm" {
        return;
    }

    // Get the pattern of the match arm.
    let Some(pattern) = node.child_by_field_name("pattern") else { return };

    // Check the full text of the pattern for `||`.
    let Ok(pat_text) = pattern.utf8_text(source) else { return };

    if !pat_text.contains("||") {
        return;
    }

    let pos = pattern.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "comma-or-logical-or-case".into(),
        message: "Match arm uses `||` (logical OR) \u{2014} use `|` for pattern alternatives.".into(),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_logical_or_in_match_arm() {
        let src = r#"
fn f(x: i32) {
    match x {
        1 || 2 => println!("one or two"),
        _ => println!("other"),
    }
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_pipe_pattern() {
        let src = r#"
fn f(x: i32) {
    match x {
        1 | 2 => println!("one or two"),
        _ => println!("other"),
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_simple_arm() {
        let src = r#"
fn f(x: i32) {
    match x {
        1 => println!("one"),
        _ => println!("other"),
    }
}
"#;
        assert!(run_on(src).is_empty());
    }
}
