//! no-undefined-argument backend — flag `undefined` passed as a function argument.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["arguments"] => |node, source, ctx, diagnostics|
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "undefined" {
            let pos = child.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
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
}
