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
        Some(&["join"])
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
        // Callee must be `*.join`
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "join" {
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
            message: "Missing the separator argument in `.join()` \u{2014} use `.join(',')` explicitly.".into(),
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
    fn flags_empty_join() {
        assert_eq!(run_on("const s = arr.join();").len(), 1);
    }


    #[test]
    fn flags_chained_join() {
        assert_eq!(run_on("foo.map(x => x.id).join()").len(), 1);
    }


    #[test]
    fn allows_join_with_separator() {
        assert!(run_on("const s = arr.join(',');").is_empty());
    }


    #[test]
    fn allows_join_with_variable() {
        assert!(run_on("const s = arr.join(sep);").is_empty());
    }
}
