//! no-large-snapshots — oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

const SNAPSHOT_MATCHERS: &[&str] = &[
    "toMatchInlineSnapshot",
    "toThrowErrorMatchingInlineSnapshot",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["toMatchInlineSnapshot", "toThrowErrorMatchingInlineSnapshot"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Callee must be a member expression like `expect(x).toMatchInlineSnapshot`.
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let name = member.property.name.as_str();
        if !SNAPSHOT_MATCHERS.contains(&name) {
            return;
        }

        // First argument must be a string/template literal.
        let Some(first_arg) = call.arguments.first() else { return };
        let arg_span = first_arg.span();

        let is_string_like = matches!(
            first_arg,
            oxc_ast::ast::Argument::TemplateLiteral(_) | oxc_ast::ast::Argument::StringLiteral(_)
        );
        if !is_string_like {
            return;
        }

        let max = ctx.config.threshold("no-large-snapshots", "max_lines", ctx.lang);
        let arg_start = arg_span.start as usize;
        let arg_end = arg_span.end as usize;
        let arg_text = ctx.source.get(arg_start..arg_end).unwrap_or("");
        let line_count = arg_text.lines().count().max(1);

        if line_count > max {
            let (line, column) = byte_offset_to_line_col(ctx.source, arg_start);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Inline snapshot spans {line_count} lines (max: {max}) \u{2014} narrow the assertion."
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



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_large_inline_snapshot() {
        let body = "\n".repeat(60);
        let src = format!("expect(x).toMatchInlineSnapshot(`{body}`)");
        assert_eq!(run_on(&src).len(), 1);
    }


    #[test]
    fn allows_small_inline_snapshot() {
        let src = "expect(x).toMatchInlineSnapshot(`hello\nworld`)";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_snapshot_matcher() {
        let body = "\n".repeat(60);
        let src = format!("expect(x).toEqual(`{body}`)");
        assert!(run_on(&src).is_empty());
    }


    #[test]
    fn ignores_empty_args() {
        assert!(run_on("expect(x).toMatchInlineSnapshot()").is_empty());
    }
}
