//! no-unnecessary-array-flat-depth backend — flag `.flat(1)`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }

    // Check that the callee is a member expression with property "flat".
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "flat" {
        return;
    }

    // Check arguments — must have exactly one argument that is `1`.
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let arg_nodes: Vec<_> = args.children(&mut cursor)
        .filter(|c| c.kind() != "(" && c.kind() != ")" && c.kind() != ",")
        .collect();

    if arg_nodes.len() != 1 {
        return;
    }

    let arg = arg_nodes[0];
    if arg.kind() == "number" && arg.utf8_text(source).unwrap_or("") == "1" {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-unnecessary-array-flat-depth".into(),
            message: "Passing `1` as the `depth` argument of `.flat()` is unnecessary \u{2014} it is the default.".into(),
            severity: Severity::Warning,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_flat_one() {
        let d = crate::rules::test_helpers::run_ts("arr.flat(1);", &Check);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-unnecessary-array-flat-depth");
    }

    #[test]
    fn allows_flat_no_args() {
        let d = crate::rules::test_helpers::run_ts("arr.flat();", &Check);
        assert!(d.is_empty());
    }

    #[test]
    fn allows_flat_other_depth() {
        let d = crate::rules::test_helpers::run_ts("arr.flat(2);", &Check);
        assert!(d.is_empty());
    }

    #[test]
    fn allows_flat_infinity() {
        let d = crate::rules::test_helpers::run_ts("arr.flat(Infinity);", &Check);
        assert!(d.is_empty());
    }
}
