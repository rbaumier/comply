//! prefer-set-size backend — flag `[...set].length` and `Array.from(set).length`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["member_expression"] => |node, source, ctx, diagnostics|
    // Look for member_expression with property `length`
    let Some(prop) = node.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "length" {
        return;
    }

    let Some(obj) = node.child_by_field_name("object") else { return };

    let is_spread_array = obj.kind() == "array" && {
        let mut cursor = obj.walk();
        let children: Vec<_> = obj.children(&mut cursor).collect();
        // `[...x]` — array with a single spread_element child
        children.iter().any(|c| c.kind() == "spread_element")
            && children.iter().filter(|c| c.kind() != "[" && c.kind() != "]" && c.kind() != ",").count() == 1
    };

    let is_array_from = obj.kind() == "call_expression" && {
        if let Some(func) = obj.child_by_field_name("function") {
            if func.kind() == "member_expression" {
                let o = func.child_by_field_name("object");
                let p = func.child_by_field_name("property");
                matches!(
                    (o.and_then(|n| n.utf8_text(source).ok()), p.and_then(|n| n.utf8_text(source).ok())),
                    (Some("Array"), Some("from"))
                )
            } else {
                false
            }
        } else {
            false
        }
    };

    if !is_spread_array && !is_array_from {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-set-size".into(),
        message: "Prefer `Set#size` instead of `[...set].length` or `Array.from(set).length`.".into(),
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
    fn flags_spread_length() {
        let d = run_on("const len = [...mySet].length;");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-set-size");
    }

    #[test]
    fn flags_array_from_length() {
        let d = run_on("const len = Array.from(mySet).length;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_set_size() {
        assert!(run_on("const len = mySet.size;").is_empty());
    }

    #[test]
    fn allows_array_spread_without_length() {
        assert!(run_on("const arr = [...mySet];").is_empty());
    }

    #[test]
    fn allows_regular_array_length() {
        assert!(run_on("const len = myArray.length;").is_empty());
    }
}
