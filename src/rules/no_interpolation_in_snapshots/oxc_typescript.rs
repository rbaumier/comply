//! no-interpolation-in-snapshots oxc backend — flag `toMatchSnapshot` /
//! `toMatchInlineSnapshot` calls receiving a template literal with interpolation.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const SNAPSHOT_MATCHERS: &[&str] = &["toMatchSnapshot", "toMatchInlineSnapshot"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["toMatchSnapshot", "toMatchInlineSnapshot"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Callee must be a member expression like `expect(x).toMatchSnapshot`.
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let name = member.property.name.as_str();
        if !SNAPSHOT_MATCHERS.contains(&name) {
            return;
        }

        for arg in &call.arguments {
            let Some(Expression::TemplateLiteral(tpl)) = arg.as_expression() else { continue };
            if tpl.expressions.is_empty() {
                continue;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, tpl.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Do not use template literal interpolation in snapshot matchers.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_interpolation_in_to_match_snapshot() {
        assert_eq!(
            run_on("expect(x).toMatchSnapshot(`hello ${name}`)").len(),
            1
        );
    }


    #[test]
    fn flags_interpolation_in_to_match_inline_snapshot() {
        assert_eq!(
            run_on("expect(x).toMatchInlineSnapshot(`value is ${v}`)").len(),
            1
        );
    }


    #[test]
    fn allows_plain_template_literal() {
        assert!(run_on("expect(x).toMatchSnapshot(`hello world`)").is_empty());
    }


    #[test]
    fn allows_plain_string_argument() {
        assert!(run_on("expect(x).toMatchSnapshot('hello')").is_empty());
    }


    #[test]
    fn allows_no_arguments() {
        assert!(run_on("expect(x).toMatchSnapshot()").is_empty());
    }


    #[test]
    fn ignores_unrelated_matcher() {
        assert!(run_on("expect(x).toEqual(`hello ${name}`)").is_empty());
    }
}
