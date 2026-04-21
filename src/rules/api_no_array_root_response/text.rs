//! Text-pass scan for bare-array JSON responses.
//!
//! Flags lines that call `Response.json([`, `res.json([`, `c.json([`, or
//! `return json([` — all common signatures for handlers that ship a
//! naked array over the wire. The object form (`.json({ ... })`) is the
//! intended remediation.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const ARRAY_RESPONSE_PATTERNS: &[&str] = &[
    "Response.json([",
    "res.json([",
    "c.json([",
    "return json([",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if t.starts_with("//") {
                continue;
            }
            for pattern in ARRAY_RESPONSE_PATTERNS {
                if line.contains(pattern) {
                    diags.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: i + 1,
                        column: line.find(pattern).unwrap_or(0) + 1,
                        rule_id: super::META.id.into(),
                        message: "Return `{ data: [...] }` instead of a root-level array — arrays can't be extended without breaking clients.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                    break;
                }
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
    fn flags_response_json_array() {
        assert_eq!(
            run("export async function GET() { return Response.json([...users]) }").len(),
            1
        );
    }

    #[test]
    fn allows_object_response() {
        assert!(
            run("export async function GET() { return Response.json({ data: users }) }")
                .is_empty()
        );
    }
}
