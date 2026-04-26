//! prefer-regexp-exec backend — flag `.match(/regex/)` calls.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    // Look for call_expression nodes like `str.match(/regex/)`
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" {
        return;
    }

    let Some(prop) = func.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "match" {
        return;
    }

    // Check that the first argument is a regex literal
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let has_regex_arg = args.children(&mut cursor).any(|c| c.kind() == "regex");

    if !has_regex_arg {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-regexp-exec".into(),
        message: "`.match(/regex/)` is slower — use `regex.exec(string)` instead.".into(),
        severity: Severity::Warning,
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
    fn flags_match_with_regex() {
        let d = run_on("const m = str.match(/foo/);");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-regexp-exec");
    }

    #[test]
    fn flags_match_with_complex_regex() {
        let d = run_on("const m = input.match(/^[a-z]+$/i);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_match_with_variable() {
        assert!(run_on("const m = str.match(pattern);").is_empty());
    }

    #[test]
    fn allows_exec() {
        assert!(run_on("const m = /foo/.exec(str);").is_empty());
    }
}
