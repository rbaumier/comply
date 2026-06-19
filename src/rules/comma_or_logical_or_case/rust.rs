//! comma-or-logical-or-case Rust backend — flag `match` arms that use `|`
//! with `||` by mistake (logical OR instead of pattern alternative).
//!
//! In Rust, match arm alternatives use `|`, but developers coming from
//! other languages might accidentally write `||` which compiles but
//! means something different (logical OR, producing a boolean pattern).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["match_arm"] => |node, source, ctx, diagnostics|
    // Get the pattern of the match arm.
    let Some(pattern) = node.child_by_field_name("pattern") else { return };

    // `match_pattern` = seq(_pattern, optional("if" condition)). A `||` in the
    // guard's condition is a boolean OR, not a pattern-alternative typo, so scan
    // only the pattern bytes before any `if` guard.
    let pat_text = match pattern.child_by_field_name("condition") {
        Some(cond) => match std::str::from_utf8(&source[pattern.start_byte()..cond.start_byte()]) {
            Ok(t) => t,
            Err(_) => return,
        },
        None => match pattern.utf8_text(source) {
            Ok(t) => t,
            Err(_) => return,
        },
    };

    if !pat_text.contains("||") {
        return;
    }

    let pos = pattern.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "comma-or-logical-or-case".into(),
        message: "Match arm uses `||` (logical OR) \u{2014} use `|` for pattern alternatives.".into(),
        severity: Severity::Error,
        span: None,
    });
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
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
    fn allows_logical_or_in_guard_char_literal() {
        // Regression for #3896: `||` lives in the match-arm guard, not the pattern.
        let src = r#"
fn f() {
    match c {
        b't' | b'f' if repr == "true" || repr == "false" => {}
        _ => {}
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_logical_or_in_guard_method_call() {
        // Regression for #3896 (syn src/stmt.rs shape).
        let src = r#"
fn f() {
    match e {
        E::M(x) if a.is_some() || b.is_brace() => {}
        _ => {}
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_logical_or_in_guard_range() {
        // Regression for #3896 (syn src/pat.rs shape).
        let src = r#"
fn f() {
    match p {
        P::Range(r) if r.start.is_none() || r.end.is_none() => {}
        _ => {}
    }
}
"#;
        assert!(run_on(src).is_empty());
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
