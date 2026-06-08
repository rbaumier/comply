//! elysia-no-server-assertion oxc backend — flag `server!` non-null assertions.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSNonNullExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["server!"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSNonNullExpression(expr) = node.kind() else {
            return;
        };
        if !ctx.project.has_framework("elysia") {
            return;
        }

        let text = &ctx.source[expr.span.start as usize..expr.span.end as usize];
        // text looks like `something!` — check that it ends with `server!` or `.server!`.
        if !(text.ends_with(".server!") || text == "server!") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`server!` non-null assertion is unsafe — `app.server` is undefined until `.listen()` resolves.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "elysia")
    }


    #[test]
    fn flags_app_server_bang() {
        let src = "import { Elysia } from 'elysia';\nconst port = app.server!.port;";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_bare_server_bang() {
        let src = "import { Elysia } from 'elysia';\nconst s = server!;";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_server_without_bang() {
        let src = "import { Elysia } from 'elysia';\napp.listen(3000, () => { console.log(app.server?.port); });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "const port = app.server!.port;";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
