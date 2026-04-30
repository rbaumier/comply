//! no-undefined-argument backend — flag `undefined` passed as a function argument.

use crate::diagnostic::{Diagnostic, Severity};

fn is_in_assertion_chain(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cur = node.parent();
    while let Some(n) = cur {
        if n.kind() == "call_expression" {
            if let Some(func) = n.child_by_field_name("function") {
                let text = func.utf8_text(source).unwrap_or("");
                if text.contains("expect") || text.contains("assert") {
                    return true;
                }
            }
        }
        cur = n.parent();
    }
    false
}

crate::ast_check! { on ["arguments"] prefilter = ["undefined"] => |node, source, ctx, diagnostics|
    if is_in_assertion_chain(node, source) { return; }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "undefined" {
            let pos = child.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-undefined-argument".into(),
                message: "Do not pass `undefined` as an argument \u{2014} omit the argument instead.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_sole_undefined_arg() {
        let d = crate::rules::test_helpers::run_ts("foo(undefined);", &Check);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-undefined-argument");
    }

    #[test]
    fn flags_undefined_among_args() {
        let d = crate::rules::test_helpers::run_ts("foo(x, undefined, y);", &Check);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_no_undefined() {
        let d = crate::rules::test_helpers::run_ts("foo(x, y);", &Check);
        assert!(d.is_empty());
    }

    #[test]
    fn allows_undefined_in_variable_name() {
        let d = crate::rules::test_helpers::run_ts("foo(undefinedValue);", &Check);
        assert!(d.is_empty());
    }

    #[test]
    fn allows_undefined_in_expect_matcher() {
        let d = crate::rules::test_helpers::run_ts(
            "expect(spy).toHaveBeenCalledWith(state, undefined);",
            &Check,
        );
        assert!(d.is_empty());
    }

    #[test]
    fn allows_undefined_in_to_equal() {
        let d = crate::rules::test_helpers::run_ts("expect(result).toEqual(undefined);", &Check);
        assert!(d.is_empty());
    }

    #[test]
    fn still_flags_outside_expect() {
        let d = crate::rules::test_helpers::run_ts("doStuff(undefined);", &Check);
        assert_eq!(d.len(), 1);
    }
}
