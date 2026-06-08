//! prefer-structured-clone backend — flag `JSON.parse(JSON.stringify(…))`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["JSON.parse"] => |node, source, ctx, diagnostics|
    // Look for call_expression whose callee is `JSON.parse`
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
    if args.named_child_count() != 1 {
        return;
    }
    let Some(inner_call) = args.named_child(0) else { return };
    if inner_call.kind() != "call_expression" {
        return;
    }

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

    let Some(inner_args) = inner_call.child_by_field_name("arguments") else { return };
    if inner_args.named_child_count() != 1 {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-structured-clone".into(),
        message: "Prefer `structuredClone(…)` over `JSON.parse(JSON.stringify(…))` to create a deep clone.".into(),
        severity: Severity::Warning,
        span: None,
    });
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
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

    #[test]
    fn allows_stringify_replacer() {
        assert!(run_on("const copy = JSON.parse(JSON.stringify(obj, replacer));").is_empty());
    }

    #[test]
    fn allows_parse_reviver() {
        assert!(run_on("const copy = JSON.parse(JSON.stringify(obj), reviver);").is_empty());
    }
}
