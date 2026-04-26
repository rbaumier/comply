//! prefer-response-static-json backend — flag `new Response(JSON.stringify(...))`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["new_expression"] => |node, source, ctx, diagnostics|
    // Look for `new Response(JSON.stringify(...))`
    let Some(constructor) = node.child_by_field_name("constructor") else { return };
    if constructor.utf8_text(source).unwrap_or("") != "Response" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let first_arg = args.children(&mut cursor)
        .find(|c| c.kind() != "," && !c.is_extra() && c.kind() != "(" && c.kind() != ")");

    let Some(first) = first_arg else { return };

    // The first argument should be a call_expression: JSON.stringify(...)
    if first.kind() != "call_expression" {
        return;
    }

    let Some(func) = first.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" {
        return;
    }

    let Some(obj) = func.child_by_field_name("object") else { return };
    let Some(prop) = func.child_by_field_name("property") else { return };

    if obj.utf8_text(source).unwrap_or("") != "JSON"
        || prop.utf8_text(source).unwrap_or("") != "stringify"
    {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-response-static-json".into(),
        message: "Prefer `Response.json(data)` over `new Response(JSON.stringify(data))`.".into(),
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
    fn flags_new_response_json_stringify() {
        let d = run_on(
            r#"return new Response(JSON.stringify(data), { headers: { "Content-Type": "application/json" } });"#,
        );
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-response-static-json");
    }

    #[test]
    fn flags_bare_new_response_json_stringify() {
        let d = run_on("const res = new Response(JSON.stringify({ ok: true }));");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_response_json() {
        assert!(run_on("return Response.json(data);").is_empty());
    }

    #[test]
    fn allows_new_response_with_string() {
        assert!(run_on(r#"return new Response("hello");"#).is_empty());
    }
}
