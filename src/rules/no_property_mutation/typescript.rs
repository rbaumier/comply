use crate::diagnostic::{Diagnostic, Severity};

fn root_object_name<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    let mut cur = node;
    loop {
        match cur.kind() {
            "member_expression" | "subscript_expression" => {
                cur = cur.child_by_field_name("object")?;
            }
            "identifier" => return cur.utf8_text(source).ok(),
            _ => return None,
        }
    }
}

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

            // Allow: document.cookie = ... (only Web API for client-side cookies)
            if obj_text == "document" && prop_text == "cookie" { return; }

            // Allow: set.* = ... (Elysia response context)
            if root_object_name(left, source) == Some("set") { return; }

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
    fn run(code: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(code, &Check)
    }

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
    fn allows_elysia_set_status() {
        assert!(run("set.status = 404").is_empty());
    }

    #[test]
    fn allows_elysia_set_headers() {
        assert!(run(r#"set.headers["cache-control"] = "no-store""#).is_empty());
    }

    #[test]
    fn allows_elysia_set_nested() {
        assert!(run(r#"set.headers["x-content-type-options"] = "nosniff""#).is_empty());
    }

    #[test]
    fn allows_document_cookie() {
        assert!(run(r#"document.cookie = "name=value""#).is_empty());
    }

    #[test]
    fn still_flags_other_subscript_mutations() {
        assert_eq!(run(r#"obj.headers["key"] = "val""#).len(), 1);
    }

    #[test]
    fn still_flags_non_set_mutations() {
        assert_eq!(run("response.statusText = 'OK'").len(), 1);
    }
}
