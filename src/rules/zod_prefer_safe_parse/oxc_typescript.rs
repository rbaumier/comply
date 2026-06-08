//! zod-prefer-safe-parse OXC backend — flag `.parse()` calls inside route handler files.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const ROUTE_FILE_PATTERNS: &[&str] = &[
    "route.ts",
    "route.tsx",
    "handler.ts",
    "+server.ts",
    "page.server.ts",
    "controller.ts",
];

fn is_route_file(ctx: &CheckCtx) -> bool {
    let file_name = ctx.path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if ROUTE_FILE_PATTERNS.iter().any(|p| file_name.ends_with(p)) {
        return true;
    }
    ctx.source_contains("export async function GET")
        || ctx.source_contains("export async function POST")
        || ctx.source_contains("export async function PUT")
        || ctx.source_contains("export async function DELETE")
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !is_route_file(ctx) {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };

        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "parse" {
            return;
        }

        // Skip JSON.parse(...)
        if let Expression::Identifier(id) = &member.object
            && id.name.as_str() == "JSON" {
                return;
            }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `.safeParse()` in route handlers \u{2014} `.parse()` throws `ZodError` which leaks schema internals to clients.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(path: &str, src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, path)
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
        assert!(
            run(
                "route.ts",
                "export async function POST() { const body = JSON.parse(raw) }"
            )
            .is_empty()
        );
    }

    #[test]
    fn ignores_non_route() {
        assert!(run("utils.ts", "const x = schema.parse(data)").is_empty());
    }
}
