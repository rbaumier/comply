use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["assignment_expression", "augmented_assignment_expression", "update_expression", "unary_expression"] => |node, source, ctx, diagnostics|
match node.kind() {
        // obj.prop = value or obj['prop'] = value
        "assignment_expression" | "augmented_assignment_expression" => {
            let Some(left) = node.child_by_field_name("left") else { return; };
            if !matches!(left.kind(), "member_expression" | "subscript_expression") { return; }

            // Allow: module.exports = ...
            let obj_text = left.child_by_field_name("object")
                .and_then(|o| o.utf8_text(source).ok())
                .unwrap_or("");
            if obj_text == "module" || obj_text == "exports" { return; }

            // Allow: ref.current = ... (React useRef pattern)
            let prop_text = left.child_by_field_name("property")
                .and_then(|p| p.utf8_text(source).ok())
                .unwrap_or("");
            if prop_text == "current" { return; }

            // Allow: set.headers["x"] = ... (Elysia response context)
            if left.kind() == "subscript_expression" {
                if let Some(sub_obj) = left.child_by_field_name("object") {
                    if sub_obj.kind() == "member_expression" {
                        let sub_obj_text = sub_obj.child_by_field_name("object")
                            .and_then(|o| o.utf8_text(source).ok())
                            .unwrap_or("");
                        let sub_prop_text = sub_obj.child_by_field_name("property")
                            .and_then(|p| p.utf8_text(source).ok())
                            .unwrap_or("");
                        if sub_obj_text == "set" && sub_prop_text == "headers" { return; }
                    }
                }
            }

            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-property-mutation".into(),
                message: "Property mutation — use spread or immutable patterns.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        // ++obj.prop or obj.prop++
        "update_expression" => {
            let Some(arg) = node.named_child(0) else { return; };
            if !matches!(arg.kind(), "member_expression" | "subscript_expression") { return; }

            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-property-mutation".into(),
                message: "Property mutation (increment/decrement) — use immutable patterns.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        // delete obj.prop
        "unary_expression" => {
            let op = node.child_by_field_name("operator")
                .and_then(|o| o.utf8_text(source).ok())
                .unwrap_or("");
            if op != "delete" { return; }

            let Some(arg) = node.child_by_field_name("argument") else { return; };
            if !matches!(arg.kind(), "member_expression" | "subscript_expression") { return; }

            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-property-mutation".into(),
                message: "Property deletion — use destructuring or immutable patterns.".into(),
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
    fn run(code: &str) -> Vec<Diagnostic> { crate::rules::test_helpers::run_ts(code, &Check) }

    #[test]
    fn flags_property_assignment() {
        assert_eq!(run("obj.prop = 1").len(), 1);
        assert_eq!(run("obj['prop'] = 1").len(), 1);
    }

    #[test]
    fn flags_compound_assignment() {
        assert_eq!(run("obj.count += 1").len(), 1);
        assert_eq!(run("obj.str += 'x'").len(), 1);
    }

    #[test]
    fn flags_increment() {
        assert_eq!(run("obj.count++").len(), 1);
        assert_eq!(run("++obj.count").len(), 1);
    }

    #[test]
    fn flags_delete() {
        assert_eq!(run("delete obj.prop").len(), 1);
    }

    #[test]
    fn allows_variable_assignment() {
        assert!(run("let x = 1").is_empty());
        assert!(run("x = 1").is_empty());
    }

    #[test]
    fn allows_module_exports() {
        assert!(run("module.exports = {}").is_empty());
        assert!(run("exports.foo = bar").is_empty());
    }

    #[test]
    fn allows_ref_current() {
        assert!(run("timerRef.current = setTimeout(() => {}, 100)").is_empty());
        assert!(run("newKeyRef.current = null").is_empty());
    }

    #[test]
    fn allows_elysia_set_headers() {
        assert!(run(r#"set.headers["cache-control"] = "no-store""#).is_empty());
        assert!(run(r#"set.headers["x-content-type-options"] = "nosniff""#).is_empty());
    }

    #[test]
    fn still_flags_other_set_mutations() {
        assert_eq!(run("set.status = 404").len(), 1);
    }

    #[test]
    fn still_flags_other_subscript_mutations() {
        assert_eq!(run(r#"obj.headers["key"] = "val""#).len(), 1);
    }
}
