//! prefer-spread backend — flag `Array.from()`, `.concat()`, and `.slice()` shallow copies.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };

    if func.kind() == "member_expression" {
        let Some(obj) = func.child_by_field_name("object") else { return };
        let Some(prop) = func.child_by_field_name("property") else { return };
        let prop_text = prop.utf8_text(source).unwrap_or("");
        let obj_text = obj.utf8_text(source).unwrap_or("");

        // Array.from(...)
        if obj_text == "Array" && prop_text == "from" {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "prefer-spread".into(),
                message: "Prefer the spread operator over `Array.from(...)`.".into(),
                severity: Severity::Warning,
                span: None,
            });
            return;
        }

        // .concat(...)
        if prop_text == "concat" {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "prefer-spread".into(),
                message: "Prefer the spread operator over `Array#concat(...)`.".into(),
                severity: Severity::Warning,
                span: None,
            });
            return;
        }

        // .slice() or .slice(0) — shallow copy pattern
        if prop_text == "slice" {
            let Some(args) = node.child_by_field_name("arguments") else { return };
            let mut cursor = args.walk();
            let arg_nodes: Vec<_> = args
                .children(&mut cursor)
                .filter(|c| c.is_named())
                .collect();

            let is_copy = arg_nodes.is_empty()
                || (arg_nodes.len() == 1 && arg_nodes[0].utf8_text(source).unwrap_or("") == "0");

            if is_copy {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "prefer-spread".into(),
                    message: "Prefer the spread operator over `Array#slice()`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_array_from() {
        let d = run_on("const arr = Array.from(iterable);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Array.from"));
    }

    #[test]
    fn flags_concat() {
        let d = run_on("const combined = arr.concat(other);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("concat"));
    }

    #[test]
    fn flags_slice_empty() {
        let d = run_on("const copy = arr.slice();");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("slice"));
    }

    #[test]
    fn flags_slice_zero() {
        let d = run_on("const copy = arr.slice(0);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_slice_with_args() {
        assert!(run_on("const sub = arr.slice(1, 3);").is_empty());
    }

    #[test]
    fn allows_spread() {
        assert!(run_on("const arr = [...iterable];").is_empty());
    }
}
