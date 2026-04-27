//! elysia-cron-timezone backend — flag non-IANA timezone strings in `cron(...)` config.

use crate::diagnostic::{Diagnostic, Severity};

fn extract_tz(args: &str) -> Option<&str> {
    let idx = args.find("timezone")?;
    let rest = &args[idx + "timezone".len()..];
    // skip whitespace and `:` or `=`.
    let rest = rest.trim_start();
    let rest = rest.strip_prefix(':').or_else(|| rest.strip_prefix('='))?.trim_start();
    let quote = rest.chars().next()?;
    if quote != '\'' && quote != '"' && quote != '`' {
        return None;
    }
    let after = &rest[1..];
    let end = after.find(quote)?;
    Some(&after[..end])
}

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
    let Some(tz) = extract_tz(args_text) else { return };

    // IANA tz identifiers always contain `/` (e.g. `America/Los_Angeles`) and are usually 5+ chars.
    if tz.contains('/') && tz.len() >= 5 {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-cron-timezone".into(),
        message: format!("`timezone: '{tz}'` is not an IANA identifier — use `America/Los_Angeles` style instead."),
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
    fn flags_pst_abbreviation() {
        let src = "import { cron } from '@elysiajs/cron';\napp.use(cron({ name: 'job', pattern: '0 * * * *', timezone: 'PST', run() {} }));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_est_abbreviation() {
        let src = "import { cron } from '@elysiajs/cron';\napp.use(cron({ pattern: '*/5 * * * *', timezone: 'EST' }));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_iana_timezone() {
        let src = "import { cron } from '@elysiajs/cron';\napp.use(cron({ pattern: '0 * * * *', timezone: 'America/Los_Angeles' }));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_cron_files() {
        let src = "cron({ timezone: 'PST' });";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
