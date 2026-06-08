//! OxcCheck backend for prefer-string-slice — flag `.substring()` and `.substr()`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["substring", "substr"])
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

        let method = member.property.name.as_str();
        if method != "substring" && method != "substr" {
            return;
        }

        let prop_span = member.property.span();
        let (line, column) = byte_offset_to_line_col(ctx.source, prop_span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("Prefer `String#slice()` over `String#{method}()`."),
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
    fn flags_substring() {
        let d = run_on("str.substring(1, 3)");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("substring"));
    }


    #[test]
    fn flags_substr() {
        let d = run_on("str.substr(0, 5)");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("substr"));
    }


    #[test]
    fn allows_slice() {
        assert!(run_on("str.slice(1, 3)").is_empty());
    }


    #[test]
    fn flags_chained_call() {
        let d = run_on("foo().substring(0)");
        assert_eq!(d.len(), 1);
    }
}
