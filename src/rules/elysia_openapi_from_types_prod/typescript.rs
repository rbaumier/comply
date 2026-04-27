//! elysia-openapi-from-types-prod backend — flag unconditional `fromTypes('src/...')` calls.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.utf8_text(source).unwrap_or("") != "fromTypes" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");

    // Look for a hardcoded src/ path with no env conditional inside the call.
    let has_src_path = args_text.contains("'src/") || args_text.contains("\"src/");
    if !has_src_path {
        return;
    }
    if args_text.contains("process.env") || args_text.contains("NODE_ENV") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-openapi-from-types-prod".into(),
        message: "`fromTypes('src/...')` reads source at runtime — gate it behind a NODE_ENV check or pre-build the spec.".into(),
        severity: Severity::Warning,
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
    fn flags_hardcoded_src_path() {
        let src = "import { openapi, fromTypes } from '@elysiajs/openapi';\napp.use(openapi({ references: fromTypes('src/index.ts') }));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_double_quoted_path() {
        let src = "import { openapi, fromTypes } from '@elysiajs/openapi';\napp.use(openapi({ references: fromTypes(\"src/server.ts\") }));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_env_gated_path() {
        let src = "import { openapi, fromTypes } from '@elysiajs/openapi';\nconst refs = fromTypes(process.env.NODE_ENV === 'production' ? 'dist/index.js' : 'src/index.ts');";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_openapi_files() {
        let src = "fromTypes('src/index.ts');";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
