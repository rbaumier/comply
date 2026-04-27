//! elysia-cron-name-required backend — flag `cron({ ... })` calls missing a `name:` field.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.utf8_text(source).unwrap_or("") != "cron" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");
    let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();
    if norm.contains("name:") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-cron-name-required".into(),
        message: "`cron({ ... })` is missing `name:` — required for stop()/diagnostics.".into(),
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
    fn flags_cron_without_name() {
        let src = "import { cron } from '@elysiajs/cron';\napp.use(cron({ pattern: '* * * * *', run() {} }));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_cron_with_name() {
        let src = "import { cron } from '@elysiajs/cron';\napp.use(cron({ name: 'cleanup', pattern: '* * * * *', run() {} }));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_cron_files() {
        let src = "cron({ pattern: '* * * * *' });";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
