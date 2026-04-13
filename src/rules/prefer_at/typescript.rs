//! prefer-at AST backend — flag `arr[arr.length - N]` and `.charAt()`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    match node.kind() {
        // Pattern 1: `arr[arr.length - N]`
        "subscript_expression" => {
            let Some(obj) = node.child_by_field_name("object") else { return };
            let Some(idx) = node.child_by_field_name("index") else { return };

            // Index should be a binary expression: `arr.length - N`.
            if idx.kind() != "binary_expression" {
                return;
            }

            // Check for `-` operator.
            let mut cursor = idx.walk();
            let has_minus = idx.children(&mut cursor)
                .any(|c| c.utf8_text(source).unwrap_or("") == "-");
            if !has_minus {
                return;
            }

            let Some(left) = idx.child_by_field_name("left") else { return };
            // Left side should be `<receiver>.length`.
            if left.kind() != "member_expression" {
                return;
            }
            let Some(length_prop) = left.child_by_field_name("property") else { return };
            if length_prop.utf8_text(source).unwrap_or("") != "length" {
                return;
            }

            // The receiver of `.length` should match the receiver of `[…]`.
            let Some(length_obj) = left.child_by_field_name("object") else { return };
            let obj_text = obj.utf8_text(source).unwrap_or("");
            let length_obj_text = length_obj.utf8_text(source).unwrap_or("");
            if obj_text != length_obj_text || obj_text.is_empty() {
                return;
            }

            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "prefer-at".into(),
                message: "Prefer `.at(…)` over `[….length - index]`.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        // Pattern 2: `.charAt(…)`
        "call_expression" => {
            let Some(callee) = node.child_by_field_name("function") else { return };
            if callee.kind() != "member_expression" {
                return;
            }
            let Some(prop) = callee.child_by_field_name("property") else { return };
            if prop.utf8_text(source).unwrap_or("") != "charAt" {
                return;
            }

            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "prefer-at".into(),
                message: "Prefer `String#at(…)` over `String#charAt(…)`.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_length_minus_bracket_access() {
        let d = run_on("const last = arr[arr.length - 1];");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains(".at("));
    }

    #[test]
    fn flags_char_at() {
        let d = run_on("const c = str.charAt(0);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("at("));
    }

    #[test]
    fn allows_at() {
        assert!(run_on("const last = arr.at(-1);").is_empty());
    }

    #[test]
    fn allows_normal_bracket_access() {
        assert!(run_on("const first = arr[0];").is_empty());
    }

    #[test]
    fn flags_nested_receiver() {
        let d = run_on("const x = foo.bar[foo.bar.length - 2];");
        assert_eq!(d.len(), 1);
    }
}
