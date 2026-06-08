use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["toFixed"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        // Callee must be `*.toFixed`
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "toFixed" {
            return;
        }
        // Must have zero arguments.
        if !call.arguments.is_empty() {
            return;
        }
        let (line, column) =
            byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Missing the digits argument in `.toFixed()` \u{2014} use `.toFixed(0)` explicitly.".into(),
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
    fn flags_empty_to_fixed() {
        assert_eq!(run_on("const s = num.toFixed();").len(), 1);
    }


    #[test]
    fn flags_chained_to_fixed() {
        assert_eq!(run_on("price.toFixed().padStart(5)").len(), 1);
    }


    #[test]
    fn allows_to_fixed_with_digits() {
        assert!(run_on("const s = num.toFixed(2);").is_empty());
    }


    #[test]
    fn allows_to_fixed_with_zero() {
        assert!(run_on("const s = num.toFixed(0);").is_empty());
    }
}
