//! testing-no-mocking-internal-modules OXC backend — detect `vi.mock`/`jest.mock`
//! calls whose first argument is a relative path (`./` or `../`).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn unquote(raw: &str) -> &str {
    raw.trim_start_matches(['\'', '"', '`'])
        .trim_end_matches(['\'', '"', '`'])
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["jest.mock", "vi.mock"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Callee must be vi.mock or jest.mock
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "mock" {
            return;
        }
        let Expression::Identifier(obj) = &member.object else { return };
        let obj_name = obj.name.as_str();
        if obj_name != "vi" && obj_name != "jest" {
            return;
        }

        // First argument must be a string literal starting with "./" or "../"
        let Some(first_arg) = call.arguments.first() else { return };
        let raw = &ctx.source[first_arg.span().start as usize..first_arg.span().end as usize];
        let path = unquote(raw);

        if path.starts_with("./") || path.starts_with("../") {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, first_arg.span().start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Mocking internal module '{path}' couples tests to implementation details — mock boundaries, not internals."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_vi_mock_relative_same_dir() {
        assert_eq!(run("vi.mock('./internal');").len(), 1);
    }


    #[test]
    fn flags_vi_mock_relative_parent() {
        assert_eq!(run("vi.mock('../utils/helpers');").len(), 1);
    }


    #[test]
    fn flags_jest_mock_relative() {
        assert_eq!(run("jest.mock('./service');").len(), 1);
    }


    #[test]
    fn allows_mocking_external_package() {
        assert!(run("vi.mock('axios');").is_empty());
    }


    #[test]
    fn allows_mocking_scoped_package() {
        assert!(run("jest.mock('@scope/pkg');").is_empty());
    }


    #[test]
    fn ignores_unrelated_call() {
        assert!(run("foo.mock('./internal');").is_empty());
    }
}
