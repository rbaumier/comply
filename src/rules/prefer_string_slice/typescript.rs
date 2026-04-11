//! prefer-string-slice backend — flag `.substring()` and `.substr()`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }

    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" {
        return;
    }

    let Some(prop) = func.child_by_field_name("property") else { return };
    let method = prop.utf8_text(source).unwrap_or("");

    if method != "substring" && method != "substr" {
        return;
    }

    let pos = prop.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-string-slice".into(),
        message: format!("Prefer `String#slice()` over `String#{method}()`."),
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
    fn flags_substring() {
        let d = run_on("str.substring(1, 3)");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("substring"));
    }

    #[test]
    fn flags_substr() {
        let d = run_on("str.substr(0, 5)");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("substr"));
    }

    #[test]
    fn allows_slice() {
        assert!(run_on("str.slice(1, 3)").is_empty());
    }

    #[test]
    fn flags_chained_call() {
        let d = run_on("foo().substring(0)");
        assert_eq!(d.len(), 1);
    }
}
