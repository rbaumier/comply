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
    let scan_start = pattern.start_byte();
    let scan_end = match pattern.child_by_field_name("condition") {
        Some(cond) => cond.start_byte(),
        None => pattern.end_byte(),
    };
    let Ok(pat_text) = std::str::from_utf8(&source[scan_start..scan_end]) else { return };

    if !pat_text.contains("||") {
        return;
    }

    // A `||` inside a string/char literal or a macro `token_tree` (e.g. the JS
    // operator token in the pattern `op!("||") | op!("&&")`) is those bytes of a
    // literal token, not a pattern-alternative typo. Collect such descendants'
    // byte ranges and only flag a `||` that falls outside all of them.
    let mut literal_ranges: Vec<(usize, usize)> = Vec::new();
    collect_literal_ranges(pattern, &mut literal_ranges);

    let has_pattern_level_or = pat_text
        .match_indices("||")
        .map(|(i, _)| scan_start + i)
        .any(|off| {
            !literal_ranges
                .iter()
                .any(|&(start, end)| off >= start && off + 2 <= end)
        });

    if !has_pattern_level_or {
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

/// Collect the byte ranges of every string/char literal and macro `token_tree`
/// descendant of `node`. A `||` falling inside one of these is a literal token,
/// not a pattern-alternative `||`. Matched nodes are not descended into: their
/// range already covers any nested literal.
fn collect_literal_ranges(node: tree_sitter::Node, ranges: &mut Vec<(usize, usize)>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "string_literal" | "raw_string_literal" | "char_literal" | "token_tree" => {
                ranges.push((child.start_byte(), child.end_byte()));
            }
            _ => collect_literal_ranges(child, ranges),
        }
    }
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
    fn allows_logical_or_inside_macro_token_tree() {
        // Regression for #7313: `||` lives inside the `op!("||")` macro
        // token-tree (a JS operator token), not as a pattern-level typo.
        let src = r#"
fn f(op: Op) {
    match op {
        op!("||") | op!("&&") => {}
        _ => {}
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_logical_or_inside_string_literal_pattern() {
        // Regression for #7313: `||` inside a string-literal pattern is a
        // matched value, not a pattern-alternative typo.
        let src = r#"
fn f(s: &str) {
    match s {
        "||" | "&&" => {}
        _ => {}
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_logical_or_inside_raw_string_literal_pattern() {
        // Regression for #7313: `||` inside a raw-string-literal pattern is a
        // matched value, not a pattern-alternative typo.
        let src = r##"
fn f(s: &str) {
    match s {
        r"||" | r"&&" => {}
        _ => {}
    }
}
"##;
        assert!(run_on(src).is_empty());
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
