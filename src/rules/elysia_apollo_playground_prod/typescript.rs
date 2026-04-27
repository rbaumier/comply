//! elysia-apollo-playground-prod backend — flag `apollo({ enablePlayground: true })` literal.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.utf8_text(source).unwrap_or("") != "apollo" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");
    let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();
    if !norm.contains("enablePlayground:true") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-apollo-playground-prod".into(),
        message: "`apollo({ enablePlayground: true })` is unconditional — gate it on a non-production env flag.".into(),
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
    fn flags_enable_playground_true() {
        let src = "import { apollo } from '@elysiajs/apollo';\napp.use(apollo({ enablePlayground: true }));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_env_gated_playground() {
        let src = "import { apollo } from '@elysiajs/apollo';\napp.use(apollo({ enablePlayground: process.env.NODE_ENV !== 'production' }));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_apollo_files() {
        let src = "server({ enablePlayground: true });";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
