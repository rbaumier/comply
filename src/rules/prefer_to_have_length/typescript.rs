//! prefer-to-have-length — flag `expect(x.length).toBe(n)` / `.toEqual(n)`.

use crate::diagnostic::{Diagnostic, Severity};

const LENGTH_MATCHERS: &[&str] = &["toBe", "toEqual"];

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    // Outer: expect(x.length).<matcher>(n)
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }

    let Some(prop) = callee.child_by_field_name("property") else { return };
    let matcher = prop.utf8_text(source).unwrap_or("");
    if !LENGTH_MATCHERS.contains(&matcher) {
        return;
    }

    // The object of the member_expression should be `expect(x.length)`.
    let Some(expect_call) = callee.child_by_field_name("object") else { return };
    if expect_call.kind() != "call_expression" {
        return;
    }

    let Some(expect_fn) = expect_call.child_by_field_name("function") else { return };
    if expect_fn.kind() != "identifier"
        || expect_fn.utf8_text(source).unwrap_or("") != "expect"
    {
        return;
    }

    // Argument to expect(...) should be `<something>.length`.
    let Some(expect_args) = expect_call.child_by_field_name("arguments") else { return };
    let Some(arg) = expect_args.named_child(0) else { return };
    if arg.kind() != "member_expression" {
        return;
    }
    let Some(arg_prop) = arg.child_by_field_name("property") else { return };
    if arg_prop.utf8_text(source).unwrap_or("") != "length" {
        return;
    }

    let pos = prop.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-to-have-length".into(),
        message: format!(
            "Use `toHaveLength(n)` instead of `expect(x.length).{matcher}(n)`."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_ts;

    #[test]
    fn flags_to_be_on_length() {
        let d = run_ts("expect(arr.length).toBe(3);", &Check);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("toHaveLength"));
    }

    #[test]
    fn flags_to_equal_on_length() {
        let d = run_ts("expect(items.length).toEqual(0);", &Check);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("toHaveLength"));
    }

    #[test]
    fn allows_to_have_length() {
        let d = run_ts("expect(arr).toHaveLength(3);", &Check);
        assert!(d.is_empty());
    }

    #[test]
    fn allows_non_length_property() {
        let d = run_ts("expect(user.name).toBe('alice');", &Check);
        assert!(d.is_empty());
    }

    #[test]
    fn allows_to_be_on_plain_value() {
        let d = run_ts("expect(x).toBe(3);", &Check);
        assert!(d.is_empty());
    }
}
