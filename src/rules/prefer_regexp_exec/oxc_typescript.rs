//! prefer-regexp-exec — OXC backend.

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

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".match"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "match" {
            return;
        }

        // Check that the first argument is a regex literal.
        let has_regex_arg = call.arguments.iter().any(|arg| matches!(arg, Argument::RegExpLiteral(_)));
        if !has_regex_arg {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.match(/regex/)` is slower — use `regex.exec(string)` instead.".into(),
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
    fn flags_match_with_regex() {
        let d = run_on("const m = str.match(/foo/);");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-regexp-exec");
    }


    #[test]
    fn flags_match_with_complex_regex() {
        let d = run_on("const m = input.match(/^[a-z]+$/i);");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_match_with_variable() {
        assert!(run_on("const m = str.match(pattern);").is_empty());
    }


    #[test]
    fn allows_exec() {
        assert!(run_on("const m = /foo/.exec(str);").is_empty());
    }
}
