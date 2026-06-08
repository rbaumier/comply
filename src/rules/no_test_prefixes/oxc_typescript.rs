use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const FLAGGED: &[&str] = &["ftest", "fdescribe", "fit", "xtest", "xdescribe", "xit"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["fdescribe", "fit", "ftest", "xdescribe", "xit", "xtest"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };
        let Expression::Identifier(id) = &call.callee else { return };
        let name = id.name.as_str();
        if !FLAGGED.contains(&name) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "no-test-prefixes".into(),
            message: format!(
                "`{name}` uses a Jasmine-style f/x prefix to focus or skip a test. \
                 Use .only or .skip modifiers instead."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_ftest() {
        assert_eq!(run_on("ftest('x', () => {});").len(), 1);
    }


    #[test]
    fn flags_fdescribe() {
        assert_eq!(run_on("fdescribe('x', () => {});").len(), 1);
    }


    #[test]
    fn flags_fit() {
        assert_eq!(run_on("fit('x', () => {});").len(), 1);
    }


    #[test]
    fn flags_xtest() {
        assert_eq!(run_on("xtest('x', () => {});").len(), 1);
    }


    #[test]
    fn flags_xdescribe() {
        assert_eq!(run_on("xdescribe('x', () => {});").len(), 1);
    }


    #[test]
    fn flags_xit() {
        assert_eq!(run_on("xit('x', () => {});").len(), 1);
    }


    #[test]
    fn allows_regular_test() {
        assert!(run_on("test('x', () => {});").is_empty());
    }


    #[test]
    fn allows_test_only() {
        assert!(run_on("test.only('x', () => {});").is_empty());
    }


    #[test]
    fn allows_describe_skip() {
        assert!(run_on("describe.skip('x', () => {});").is_empty());
    }


    #[test]
    fn allows_similarly_named_identifier() {
        assert!(run_on("fitness('x');").is_empty());
    }
}
