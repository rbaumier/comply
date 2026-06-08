use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["useSecureCookies"])
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

        let callee_span = call.callee.span();
        let callee_text = &ctx.source[callee_span.start as usize..callee_span.end as usize];
        if callee_text != "betterAuth" {
            return;
        }

        let Some(first_arg) = call.arguments.first() else { return };
        use oxc_ast::ast::Argument;
        let span = match first_arg {
            Argument::ObjectExpression(obj) => obj.span,
            _ => return,
        };

        let obj_text = &ctx.source[span.start as usize..span.end as usize];
        if obj_text.contains("useSecureCookies") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Better Auth config is missing `useSecureCookies: true` — add `advanced: { useSecureCookies: true }` so session cookies are only sent over HTTPS.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;
    use super::Check;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_missing_secure_cookies() {
        assert_eq!(
            run("export const auth = betterAuth({ database: db });").len(),
            1
        );
    }


    #[test]
    fn allows_with_secure_cookies() {
        assert!(
            run("betterAuth({ advanced: { useSecureCookies: true }, database: db })").is_empty()
        );
    }


    #[test]
    fn ignores_file_without_better_auth() {
        assert!(run("const x = doSomething()").is_empty());
    }


    #[test]
    fn ignores_unrelated_call() {
        assert!(run("configure({ database: db })").is_empty());
    }
}
