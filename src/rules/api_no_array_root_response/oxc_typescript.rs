//! api-no-array-root-response oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        let is_method_call = match &call.callee {
            Expression::StaticMemberExpression(member) => {
                let prop = member.property.name.as_str();
                if prop != "json" {
                    return;
                }
                match &member.object {
                    Expression::Identifier(id) => {
                        matches!(id.name.as_str(), "Response" | "res" | "c")
                    }
                    _ => false,
                }
            }
            Expression::Identifier(id) => {
                if id.name.as_str() != "json" {
                    return;
                }
                // Bare `json(...)` — only flag inside a return statement.
                let parent = semantic.nodes().parent_node(node.id());
                if !matches!(parent.kind(), AstKind::ReturnStatement(_)) {
                    return;
                }
                true
            }
            _ => return,
        };

        if !is_method_call {
            return;
        }

        // Check if the first argument is an array expression.
        let first_is_array = call
            .arguments
            .first()
            .is_some_and(|arg| matches!(arg, Argument::ArrayExpression(_)));
        if !first_is_array {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Return `{ data: [...] }` instead of a root-level array — arrays can't be extended without breaking clients.".into(),
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
mod gated_tests {
    use super::*;
    use crate::rules::test_helpers::run_rule_gated;

    /// #3392: `res.json([...])` in a `test/` middleware verifies framework
    /// serialization, not a production endpoint — the gate must suppress it.
    #[test]
    fn skips_array_response_in_test_dir() {
        let src = "app.use(function (req, res) { res.json(['foo', 'bar', 'baz']); });";
        assert!(run_rule_gated(&Check, src, "test/res.json.js").is_empty());
    }

    /// The same root-level array in a production handler must still fire.
    #[test]
    fn flags_array_response_in_production() {
        let src = "export async function GET() { return Response.json(['foo', 'bar']); }";
        assert_eq!(run_rule_gated(&Check, src, "src/app/route.ts").len(), 1);
    }
}
