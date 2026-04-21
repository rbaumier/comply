//! Text-pass scan for GET handlers that don't mention any pagination
//! primitive anywhere in the file.
//!
//! Detection is deliberately coarse: if the file exports a `GET`
//! handler and *no* pagination term (`limit`, `cursor`, `page`,
//! `offset`, `pageSize`, `per_page`) appears in the source, the rule
//! fires once on the handler declaration line. False positives on
//! genuinely single-item routes (`GET /me`) are accepted in exchange
//! for catching silently-unbounded list endpoints.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const PAGINATION_TERMS: &[&str] = &["limit", "cursor", "page", "offset", "pageSize", "per_page"];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.contains("export async function GET") && !src.contains("export const GET") {
            return vec![];
        }
        if PAGINATION_TERMS.iter().any(|p| src.contains(p)) {
            return vec![];
        }
        let mut diags = Vec::new();
        for (i, line) in src.lines().enumerate() {
            let t = line.trim();
            if t.starts_with("export async function GET") || t.starts_with("export const GET") {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "GET handler has no pagination — add `limit`/`cursor` or `page`/`pageSize` to prevent unbounded queries.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("route.ts"), src))
    }

    #[test]
    fn flags_get_without_pagination() {
        assert_eq!(
            run("export async function GET() { return db.select().from(users) }").len(),
            1
        );
    }

    #[test]
    fn allows_get_with_limit() {
        assert!(run(
            "export async function GET(req: Request) { const { limit } = await req.json(); return db.select().from(users).limit(limit) }"
        )
        .is_empty());
    }
}
