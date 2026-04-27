//! elysia-better-auth-basepath backend — flag empty/'/' basePath in betterAuth config.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.utf8_text(source).unwrap_or("") != "betterAuth" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");
    let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();
    let invalid = norm.contains("basePath:''")
        || norm.contains("basePath:\"\"")
        || norm.contains("basePath:'/'")
        || norm.contains("basePath:\"/\"");
    if !invalid {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-better-auth-basepath".into(),
        message: "`betterAuth` `basePath` cannot be empty or `'/'` — set a real prefix like `'/api/auth'`.".into(),
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
    fn flags_empty_basepath() {
        let src = "import { betterAuth } from 'better-auth';\nexport const auth = betterAuth({ basePath: '' });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_root_basepath() {
        let src = "import { betterAuth } from 'better-auth';\nexport const auth = betterAuth({ basePath: '/' });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_double_quoted_root() {
        let src = "import { betterAuth } from 'better-auth';\nexport const auth = betterAuth({ basePath: \"/\" });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_real_basepath() {
        let src = "import { betterAuth } from 'better-auth';\nexport const auth = betterAuth({ basePath: '/api/auth' });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_better_auth_files() {
        let src = "export const auth = betterAuth({ basePath: '' });";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
