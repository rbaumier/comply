//! drizzle-no-push-in-production text backend.
//!
//! `drizzle-kit push` applies schema changes directly without a
//! migration file — fine for prototyping, catastrophic in production
//! because there is no audit trail and diffs cannot be reviewed.
//! This rule flags any line referencing `drizzle-kit push` (including
//! dialect-suffixed variants like `push:pg`) inside scripts, config,
//! or embedded template literals. Commented lines are ignored.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with('#') || trimmed.starts_with("//") {
                continue;
            }
            let Some(pos) = line.find("drizzle-kit push") else {
                continue;
            };
            // Accept `push`, `push:pg`, `push --flag`, etc. Reject
            // accidental substrings like `drizzle-kit pusher` by
            // requiring the next char (if any) to be one of `:`, space,
            // tab, quote, or end-of-string.
            let after = pos + "drizzle-kit push".len();
            let next = line.as_bytes().get(after).copied();
            let valid_boundary = matches!(
                next,
                None | Some(b':') | Some(b' ') | Some(b'\t') | Some(b'"') | Some(b'\'') | Some(b'`')
            );
            if !valid_boundary {
                continue;
            }
            diags.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: i + 1,
                column: pos + 1,
                rule_id: super::META.id.into(),
                message:
                    "`drizzle-kit push` bypasses migrations. Use `drizzle-kit generate` + \
                     `drizzle-kit migrate` in CI and production deployments."
                        .into(),
                severity: Severity::Error,
                span: None,
            });
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("deploy.ts"), src))
    }

    #[test]
    fn flags_plain_push() {
        assert_eq!(run("drizzle-kit push").len(), 1);
    }

    #[test]
    fn flags_dialect_suffixed_push() {
        assert_eq!(run("drizzle-kit push:pg --config=drizzle.config.ts").len(), 1);
    }

    #[test]
    fn flags_push_in_template_literal() {
        assert_eq!(
            run(r#"const cmd = `drizzle-kit push`;"#).len(),
            1
        );
    }

    #[test]
    fn allows_migrate() {
        assert!(run("drizzle-kit migrate").is_empty());
    }

    #[test]
    fn allows_shell_comment() {
        assert!(run("# drizzle-kit push").is_empty());
    }

    #[test]
    fn allows_js_comment() {
        assert!(run("// run drizzle-kit push in dev only").is_empty());
    }
}
