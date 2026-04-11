//! prefer-structured-clone backend — flag `JSON.parse(JSON.stringify(…))`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    // Look for call_expression whose callee is `JSON.parse`
    if node.kind() != "call_expression" {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }

    let Some(obj) = callee.child_by_field_name("object") else { return };
    let Some(prop) = callee.child_by_field_name("property") else { return };
    let Ok(obj_text) = obj.utf8_text(source) else { return };
    let Ok(prop_text) = prop.utf8_text(source) else { return };

    if obj_text != "JSON" || prop_text != "parse" {
        return;
    }

    // Check the first argument is `JSON.stringify(…)`
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let first_arg = args.children(&mut cursor).find(|c| c.kind() == "call_expression");
    let Some(inner_call) = first_arg else { return };

    let Some(inner_callee) = inner_call.child_by_field_name("function") else { return };
    if inner_callee.kind() != "member_expression" {
        return;
    }

    let Some(inner_obj) = inner_callee.child_by_field_name("object") else { return };
    let Some(inner_prop) = inner_callee.child_by_field_name("property") else { return };
    let Ok(inner_obj_text) = inner_obj.utf8_text(source) else { return };
    let Ok(inner_prop_text) = inner_prop.utf8_text(source) else { return };

    if inner_obj_text != "JSON" || inner_prop_text != "stringify" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-structured-clone".into(),
        message: "Prefer `structuredClone(…)` over `JSON.parse(JSON.stringify(…))` to create a deep clone.".into(),
        severity: Severity::Warning,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_json_parse_stringify() {
        let d = run_on("const copy = JSON.parse(JSON.stringify(obj));");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("structuredClone"));
    }

    #[test]
    fn flags_nested_expression() {
        let d = run_on("return JSON.parse(JSON.stringify(this.state));");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_structured_clone() {
        assert!(run_on("const copy = structuredClone(obj);").is_empty());
    }

    #[test]
    fn allows_json_parse_alone() {
        assert!(run_on("const data = JSON.parse(text);").is_empty());
    }

    #[test]
    fn allows_json_stringify_alone() {
        assert!(run_on("const text = JSON.stringify(obj);").is_empty());
    }
}
