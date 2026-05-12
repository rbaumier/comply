//! elysia-after-response-mutation backend — flag response mutation in onAfterResponse.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "onAfterResponse" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");

    let has_header_mutation = args_text.contains("set.headers[")
        || args_text.contains("set.headers =");
    let has_status_mutation = has_assignment(args_text, "set.status");
    let has_return = args_text.contains("return ");

    if !has_header_mutation && !has_status_mutation && !has_return {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-after-response-mutation".into(),
        message: "`onAfterResponse` cannot change the response — it runs after bytes are flushed. Move mutations to `onBeforeHandle` or `mapResponse`.".into(),
        severity: Severity::Error,
        span: None,
    });
}

fn has_assignment(text: &str, target: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = text[start..].find(target) {
        let after = start + pos + target.len();
        let rest = &text[after..];
        let next = rest.trim_start();
        if next.starts_with('=') && !next.starts_with("==") {
            return true;
        }
        start = after;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_framework(source, &Check, "elysia")
    }

    #[test]
    fn flags_set_headers_in_after() {
        let src = "import { Elysia } from 'elysia';\napp.onAfterResponse(({ set }) => {\n  set.headers['x-trace'] = 'late';\n});";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_set_status_in_after() {
        let src = "import { Elysia } from 'elysia';\napp.onAfterResponse(({ set }) => {\n  set.status = 500;\n});";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_return_in_after() {
        let src = "import { Elysia } from 'elysia';\napp.onAfterResponse(() => {\n  return { rewritten: true };\n});";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_logging_in_after() {
        let src = "import { Elysia } from 'elysia';\napp.onAfterResponse(({ request }) => {\n  console.log(request.url);\n});";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_read_only_set_status() {
        let src = "import { Elysia } from 'elysia';\napp.onAfterResponse(({ set }) => {\n  const status = typeof set.status === 'number' ? set.status : 200;\n  counter.add(1, { status });\n});";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.onAfterResponse(({ set }) => set.status = 500);";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
