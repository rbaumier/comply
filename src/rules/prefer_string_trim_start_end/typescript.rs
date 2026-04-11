//! prefer-string-trim-start-end backend — flag `.trimLeft()` / `.trimRight()`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }

    let Some(prop) = callee.child_by_field_name("property") else { return };
    let Ok(method) = prop.utf8_text(source) else { return };

    let replacement = match method {
        "trimLeft" => "trimStart",
        "trimRight" => "trimEnd",
        _ => return,
    };

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-string-trim-start-end".into(),
        message: format!(
            "Prefer `String#{}()` over `String#{}()`.",
            replacement, method
        ),
        severity: Severity::Warning,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_trim_left() {
        let d = run_on("str.trimLeft()");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("trimStart"));
    }

    #[test]
    fn flags_trim_right() {
        let d = run_on("str.trimRight()");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("trimEnd"));
    }

    #[test]
    fn allows_trim_start() {
        assert!(run_on("str.trimStart()").is_empty());
    }

    #[test]
    fn allows_trim_end() {
        assert!(run_on("str.trimEnd()").is_empty());
    }

    #[test]
    fn allows_plain_trim() {
        assert!(run_on("str.trim()").is_empty());
    }

    #[test]
    fn ignores_standalone_function() {
        assert!(run_on("trimLeft()").is_empty());
    }
}
