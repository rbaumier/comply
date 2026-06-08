//! elysia-cron-timezone OXC backend — flag non-IANA timezone strings in `cron(...)` config.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

fn extract_tz(args: &str) -> Option<&str> {
    let idx = args.find("timezone")?;
    let rest = &args[idx + "timezone".len()..];
    let rest = rest.trim_start();
    let rest = rest
        .strip_prefix(':')
        .or_else(|| rest.strip_prefix('='))?
        .trim_start();
    let quote = rest.chars().next()?;
    if quote != '\'' && quote != '"' && quote != '`' {
        return None;
    }
    let after = &rest[1..];
    let end = after.find(quote)?;
    Some(&after[..end])
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["timezone"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !ctx.project.has_framework("elysia") {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };

        let callee_name = match &call.callee {
            Expression::Identifier(id) => id.name.as_str(),
            _ => return,
        };
        if callee_name != "cron" {
            return;
        }

        // Extract the raw source text of the arguments.
        let args_start = call.span.start as usize;
        let args_end = call.span.end as usize;
        let args_text = &ctx.source[args_start..args_end];

        let Some(tz) = extract_tz(args_text) else { return };

        // IANA tz identifiers always contain `/` and are usually 5+ chars.
        if tz.contains('/') && tz.len() >= 5 {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("`timezone: '{tz}'` is not an IANA identifier — use `America/Los_Angeles` style instead."),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "elysia")
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
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
