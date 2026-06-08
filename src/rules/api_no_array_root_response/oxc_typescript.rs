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
mod tests {
    use super::*;



    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
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
            run("export async function GET() { return Response.json({ data: users }) }").is_empty()
        );
    }
}
