//! prefer-string-replace-all OXC backend — flag `.replace(/pattern/g, ...)`.

use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, RegExpFlags};

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".replace"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
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
        if member.property.name.as_str() != "replace" {
            return;
        }

        // First argument must be a regex literal with the `g` flag.
        let Some(first_arg) = call.arguments.first() else { return };
        let Argument::RegExpLiteral(regex) = first_arg else { return };

        if !regex.regex.flags.contains(RegExpFlags::G) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Prefer `String#replaceAll()` over `String#replace()` with a global regex."
                .into(),
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
    fn flags_replace_with_global_regex() {
        let d = run_on(r#"str.replace(/foo/g, 'bar')"#);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-string-replace-all");
    }


    #[test]
    fn flags_replace_with_gu_flags() {
        let d = run_on(r#"str.replace(/foo/gu, 'bar')"#);
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_replace_without_global() {
        assert!(run_on(r#"str.replace(/foo/, 'bar')"#).is_empty());
    }


    #[test]
    fn allows_replace_with_string_arg() {
        assert!(run_on(r#"str.replace('foo', 'bar')"#).is_empty());
    }


    #[test]
    fn allows_replace_all_already() {
        assert!(run_on(r#"str.replaceAll('foo', 'bar')"#).is_empty());
    }


    #[test]
    fn flags_replace_with_case_insensitive_global() {
        let d = run_on(r#"str.replace(/foo/gi, 'bar')"#);
        assert_eq!(d.len(), 1);
    }
}
