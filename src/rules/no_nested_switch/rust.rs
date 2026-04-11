//! no-nested-switch Rust backend — flag `match` nested inside another `match`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, _source, ctx, diagnostics|
    if node.kind() != "match_expression" {
        return;
    }
    // Walk ancestors — if any parent is also a match_expression, flag it.
    let mut parent = node.parent();
    while let Some(p) = parent {
        if p.kind() == "match_expression" {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-nested-switch".into(),
                message: "Nested `match` \u{2014} extract the inner match into a separate function.".into(),
                severity: Severity::Error,
            });
            return;
        }
        parent = p.parent();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_nested_match() {
        let src = r#"
fn f() {
    match a {
        1 => {
            match b {
                2 => {},
                _ => {},
            }
        },
        _ => {},
    }
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_sequential_matches() {
        let src = r#"
fn f() {
    match a {
        1 => {},
        _ => {},
    }
    match b {
        2 => {},
        _ => {},
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_single_match() {
        let src = r#"
fn f() {
    match action {
        Action::Start => run(),
        Action::Stop => halt(),
    }
}
"#;
        assert!(run_on(src).is_empty());
    }
}
