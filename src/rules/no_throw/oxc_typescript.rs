use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ThrowStatement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["throw"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ThrowStatement(stmt) = node.kind() else {
            return;
        };
        let (line, column) = byte_offset_to_line_col(ctx.source, stmt.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "no-throw".into(),
            message: "Use Result<T, E> instead of throw — surface errors as values, \
                      not exceptions. Callers can't see thrown errors in the type signature."
                .into(),
            severity: Severity::Error,
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
    fn flags_throw_statement() {
        let diags = run_on("function f() { throw new Error('boom'); }");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "no-throw");
    }


    #[test]
    fn allows_code_without_throw() {
        assert!(run_on("function f() { return 42; }").is_empty());
    }


    #[test]
    fn flags_multiple_throws() {
        let diags = run_on("function f() { throw 1; } function g() { throw 2; }");
        assert_eq!(diags.len(), 2);
    }
}
