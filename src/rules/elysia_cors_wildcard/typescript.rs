//! elysia-cors-wildcard backend — flag permissive CORS in Elysia apps.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = callee.utf8_text(source).unwrap_or("");
    if callee_text != "cors" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");
    let pos = node.start_position();

    if args_text == "()" {
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "elysia-cors-wildcard".into(),
            message: "`cors()` without arguments allows any origin to access the API.".into(),
            severity: Severity::Error,
            span: None,
        });
        return;
    }

    let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();
    if norm.contains("origin:'*'") || norm.contains("origin:\"*\"") {
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "elysia-cors-wildcard".into(),
            message: "`origin: '*'` allows any origin to access the API.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_framework(source, &Check, "elysia")
    }

    #[test]
    fn flags_bare_cors() {
        let src = "import { cors } from '@elysiajs/cors';\napp.use(cors());";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_wildcard_origin() {
        let src = "import { cors } from '@elysiajs/cors';\napp.use(cors({ origin: '*' }));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_specific_origin() {
        let src = "import { cors } from '@elysiajs/cors';\napp.use(cors({ origin: 'https://example.com' }));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.use(cors());";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
