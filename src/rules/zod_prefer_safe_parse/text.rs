//! zod-prefer-safe-parse backend — flag `.parse()` calls inside route
//! handlers so that Zod validation failures cannot escape as unhandled
//! `ZodError` exceptions. Route handlers are detected heuristically by
//! filename (`route.ts`, `+server.ts`, etc.) or by the presence of an
//! exported HTTP verb handler.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const ROUTE_FILE_PATTERNS: &[&str] = &[
    "route.ts",
    "route.tsx",
    "handler.ts",
    "+server.ts",
    "page.server.ts",
    "controller.ts",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let file_name = ctx
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        let is_route = ROUTE_FILE_PATTERNS.iter().any(|p| file_name.ends_with(p))
            || ctx.source.contains("export async function GET")
            || ctx.source.contains("export async function POST")
            || ctx.source.contains("export async function PUT")
            || ctx.source.contains("export async function DELETE");
        if !is_route {
            return vec![];
        }
        if !ctx.source.contains(".parse(") {
            return vec![];
        }
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if t.starts_with("//") {
                continue;
            }
            if t.contains(".parse(") && !t.contains(".safeParse(") && !t.contains("JSON.parse(") {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: line.find(".parse(").unwrap_or(0) + 1,
                    rule_id: super::META.id.into(),
                    message: "Use `.safeParse()` in route handlers — `.parse()` throws `ZodError` which leaks schema internals to clients.".into(),
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

    fn run(path: &str, src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), src))
    }

    #[test]
    fn flags_parse_in_route() {
        assert_eq!(
            run(
                "route.ts",
                "export async function POST() { const body = schema.parse(data) }"
            )
            .len(),
            1
        );
    }

    #[test]
    fn allows_safe_parse() {
        assert!(run("route.ts", "const r = schema.safeParse(data)").is_empty());
    }

    #[test]
    fn allows_json_parse() {
        assert!(run(
            "route.ts",
            "export async function POST() { const body = JSON.parse(raw) }"
        )
        .is_empty());
    }

    #[test]
    fn ignores_non_route() {
        assert!(run("utils.ts", "const x = schema.parse(data)").is_empty());
    }
}
