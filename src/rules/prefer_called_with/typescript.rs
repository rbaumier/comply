//! prefer-called-with — flag `toHaveBeenCalled()` with no arguments.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["toHaveBeenCalled"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }

    let Some(prop) = callee.child_by_field_name("property") else { return };
    let matcher = prop.utf8_text(source).unwrap_or("");
    if matcher != "toHaveBeenCalled" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    if args.named_child_count() > 0 {
        return;
    }

    // Skip negated assertions.
    let mut cursor = callee.child_by_field_name("object");
    while let Some(obj) = cursor {
        if obj.kind() != "member_expression" {
            break;
        }
        if let Some(p) = obj.child_by_field_name("property") {
            if p.utf8_text(source).unwrap_or("") == "not" {
                return;
            }
        }
        cursor = obj.child_by_field_name("object");
    }

    let pos = prop.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-called-with".into(),
        message: "Use `toHaveBeenCalledWith(...)` to assert specific arguments instead of bare `toHaveBeenCalled()`.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_ts;

    #[test]
    fn flags_bare_to_have_been_called() {
        let d = run_ts("expect(mock).toHaveBeenCalled();", &Check);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("toHaveBeenCalledWith"));
    }

    #[test]
    fn allows_to_have_been_called_with() {
        let d = run_ts("expect(mock).toHaveBeenCalledWith(1, 2);", &Check);
        assert!(d.is_empty());
    }

    #[test]
    fn allows_unrelated_matcher() {
        let d = run_ts("expect(x).toBe(1);", &Check);
        assert!(d.is_empty());
    }

    #[test]
    fn flags_chained_expect_to_have_been_called() {
        let d = run_ts("expect(fn).toHaveBeenCalled();", &Check);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn skips_negated_to_have_been_called() {
        let d = run_ts(
            "expect(CAPTURE_EXCEPTION_MOCK).not.toHaveBeenCalled();",
            &Check,
        );
        assert!(d.is_empty());
    }

    #[test]
    fn skips_resolves_not_to_have_been_called() {
        let d = run_ts("expect(mock).resolves.not.toHaveBeenCalled();", &Check);
        assert!(d.is_empty());
    }

    #[test]
    fn skips_rejects_not_to_have_been_called() {
        let d = run_ts("expect(mock).rejects.not.toHaveBeenCalled();", &Check);
        assert!(d.is_empty());
    }
}
