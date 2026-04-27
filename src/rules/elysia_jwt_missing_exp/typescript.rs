//! elysia-jwt-missing-exp backend — flag jwt config without `exp`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = callee.utf8_text(source).unwrap_or("");
    if callee_text != "jwt" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");

    // Only flag if there is at least one option object (skip `jwt()` with no args).
    if args_text == "()" {
        return;
    }

    let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();
    if norm.contains("exp:") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-jwt-missing-exp".into(),
        message: "JWT config is missing `exp` — tokens will never expire.".into(),
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
    fn flags_missing_exp() {
        let src = "import { jwt } from '@elysiajs/jwt';\napp.use(jwt({ name: 'jwt', secret: process.env.S! }));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_with_exp() {
        let src = "import { jwt } from '@elysiajs/jwt';\napp.use(jwt({ name: 'jwt', secret: process.env.S!, exp: '7d' }));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "jwt({ name: 'jwt' });";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
