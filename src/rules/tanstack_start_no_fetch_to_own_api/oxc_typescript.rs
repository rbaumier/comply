//! tanstack-start-no-fetch-to-own-api — OXC backend.
//! Flag `fetch('/api/...')` in files that import from `@tanstack/start`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["@tanstack/start", "@tanstack/react-start"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Must be a bare `fetch(...)` call.
        let Expression::Identifier(callee) = &call.callee else { return };
        if callee.name.as_str() != "fetch" {
            return;
        }

        // First argument must be a string starting with `/api/`.
        let Some(first_arg) = call.arguments.first() else { return };
        let literal = match first_arg.as_expression() {
            Some(Expression::StringLiteral(s)) => s.value.as_str(),
            Some(Expression::TemplateLiteral(t)) => {
                // Only plain template literals (no substitutions).
                if !t.expressions.is_empty() {
                    return;
                }
                if let Some(q) = t.quasis.first() {
                    q.value.raw.as_str()
                } else {
                    return;
                }
            }
            _ => return,
        };

        if !literal.starts_with("/api/") {
            return;
        }

        // File must use TanStack Start.
        if !ctx.source_contains("@tanstack/start")
            && !ctx.source_contains("@tanstack/react-start")
        {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Don't `fetch('{literal}')` your own app. Call a `createServerFn` \
                 instead for typed, round-trip-free access."
            ),
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    const TANSTACK_IMPORT: &str = "import { createServerFn } from '@tanstack/start';\n";

    #[test]
    fn flags_fetch_api_string() {
        let src = format!("{TANSTACK_IMPORT}fetch('/api/users');");
        assert_eq!(run(&src).len(), 1);
    }

    #[test]
    fn flags_fetch_api_template() {
        let src = format!("{TANSTACK_IMPORT}fetch(`/api/users`);");
        assert_eq!(run(&src).len(), 1);
    }

    #[test]
    fn allows_external_fetch() {
        let src = format!("{TANSTACK_IMPORT}fetch('https://example.com/data');");
        assert!(run(&src).is_empty());
    }

    #[test]
    fn allows_non_api_path() {
        let src = format!("{TANSTACK_IMPORT}fetch('/health');");
        assert!(run(&src).is_empty());
    }

    #[test]
    fn ignores_when_no_tanstack_start_import() {
        assert!(run("fetch('/api/users');").is_empty());
    }
}
