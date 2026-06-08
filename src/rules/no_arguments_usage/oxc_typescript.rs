use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_arguments_object(expr: &Expression) -> bool {
    matches!(expr, Expression::Identifier(id) if id.name.as_str() == "arguments")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::StaticMemberExpression,
            AstType::ComputedMemberExpression,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let span = match node.kind() {
            AstKind::StaticMemberExpression(member) => {
                if !is_arguments_object(&member.object) {
                    return;
                }
                member.object.span()
            }
            AstKind::ComputedMemberExpression(member) => {
                if !is_arguments_object(&member.object) {
                    return;
                }
                member.object.span()
            }
            _ => return,
        };
        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Avoid direct use of `arguments` — use rest parameters (`...args`) instead."
                .into(),
            severity: super::META.severity,
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_arguments_bracket() {
        assert_eq!(run_on("function f() { return arguments[0]; }").len(), 1);
    }

    #[test]
    fn flags_arguments_length() {
        assert_eq!(
            run_on("function f() { if (arguments.length > 0) {} }").len(),
            1
        );
    }

    #[test]
    fn flags_arguments_callee() {
        assert_eq!(run_on("function f() { return arguments.callee; }").len(), 1);
    }

    #[test]
    fn allows_rest_params() {
        assert!(run_on("function foo(...args: any[]) { return args[0]; }").is_empty());
    }

    #[test]
    fn allows_unrelated_identifier() {
        assert!(run_on("const arguments_list = [1, 2, 3];").is_empty());
    }
}
