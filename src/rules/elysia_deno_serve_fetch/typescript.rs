//! elysia-deno-serve-fetch backend — flag `Deno.serve(app)` with a bare identifier instead of `app.fetch`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = callee.utf8_text(source).unwrap_or("");
    if callee_text != "Deno.serve" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let named: Vec<_> = args.named_children(&mut cursor).collect();
    if named.is_empty() {
        return;
    }
    // First arg may be options object, in which case look at second.
    // But typical pattern is `Deno.serve(handler)` or `Deno.serve(opts, handler)`.
    let handler = if named[0].kind() == "object" && named.len() >= 2 {
        named[1]
    } else {
        named[0]
    };
    if handler.kind() != "identifier" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-deno-serve-fetch".into(),
        message: "`Deno.serve(app)` does not call Elysia — pass `app.fetch` instead.".into(),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_framework(source, &Check, "elysia")
    }

    #[test]
    fn flags_serve_bare_identifier() {
        let src = "import { Elysia } from 'elysia';\nconst app = new Elysia();\nDeno.serve(app);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_serve_app_fetch() {
        let src = "import { Elysia } from 'elysia';\nconst app = new Elysia();\nDeno.serve(app.fetch);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "Deno.serve(handler);";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
