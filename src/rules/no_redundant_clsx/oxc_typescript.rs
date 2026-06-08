//! OXC backend for no-redundant-clsx — flag `clsx("foo")` / `cn("foo")` calls
//! with a single static string argument.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

const NAMES: &[&str] = &["clsx", "cn"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["clsx", "cn"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let oxc_ast::ast::Expression::Identifier(callee) = &call.callee else {
            return;
        };
        let name = callee.name.as_str();
        if !NAMES.contains(&name) {
            return;
        }

        // Exactly one argument, and it must be a string literal.
        if call.arguments.len() != 1 {
            return;
        }
        let Some(arg) = call.arguments.first() else { return };
        if !matches!(arg, oxc_ast::ast::Argument::StringLiteral(_)) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{}()` with a single static string is redundant — use the string directly.",
                name
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
    fn flags_clsx_single_string() {
        assert_eq!(run_on(r#"const c = clsx("foo");"#).len(), 1);
    }


    #[test]
    fn flags_cn_single_string() {
        assert_eq!(run_on(r#"const c = cn("foo bar");"#).len(), 1);
    }


    #[test]
    fn flags_clsx_single_quoted_string() {
        assert_eq!(run_on("const c = clsx('foo');").len(), 1);
    }


    #[test]
    fn allows_clsx_with_variable() {
        assert!(run_on(r#"const c = clsx(className);"#).is_empty());
    }


    #[test]
    fn allows_clsx_with_template_literal() {
        assert!(run_on("const c = clsx(`foo ${x}`);").is_empty());
    }


    #[test]
    fn allows_clsx_multiple_args() {
        assert!(run_on(r#"const c = clsx("foo", "bar");"#).is_empty());
    }


    #[test]
    fn allows_clsx_with_object() {
        assert!(run_on(r#"const c = clsx({ foo: true });"#).is_empty());
    }


    #[test]
    fn allows_clsx_no_args() {
        assert!(run_on("const c = clsx();").is_empty());
    }


    #[test]
    fn ignores_other_calls() {
        assert!(run_on(r#"const c = other("foo");"#).is_empty());
    }


    #[test]
    fn ignores_member_call() {
        assert!(run_on(r#"const c = utils.clsx("foo");"#).is_empty());
    }
}
