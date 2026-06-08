//! prefer-string-trim-start-end oxc backend — flag `.trimLeft()` / `.trimRight()`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["trimLeft", "trimRight"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };

        let method = member.property.name.as_str();
        let replacement = match method {
            "trimLeft" => "trimStart",
            "trimRight" => "trimEnd",
            _ => return,
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Prefer `String#{}()` over `String#{}()`.",
                replacement, method
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
    fn flags_trim_left() {
        let d = run_on("str.trimLeft()");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("trimStart"));
    }


    #[test]
    fn flags_trim_right() {
        let d = run_on("str.trimRight()");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("trimEnd"));
    }


    #[test]
    fn allows_trim_start() {
        assert!(run_on("str.trimStart()").is_empty());
    }


    #[test]
    fn allows_trim_end() {
        assert!(run_on("str.trimEnd()").is_empty());
    }


    #[test]
    fn allows_plain_trim() {
        assert!(run_on("str.trim()").is_empty());
    }


    #[test]
    fn ignores_standalone_function() {
        assert!(run_on("trimLeft()").is_empty());
    }
}
