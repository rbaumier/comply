//! structured-api-error oxc backend — flag `new Error()` in route handler files.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const ROUTE_METHODS: &[&str] = &["get", "post", "put", "delete", "patch"];

fn is_route_file(source: &str) -> bool {
    source.lines().any(|line| {
        let t = line.trim();
        ROUTE_METHODS.iter().any(|m| {
            let pat = format!(".{m}(");
            t.contains(&pat)
        }) || t.contains("from 'hono'")
            || t.contains("from \"hono\"")
            || t.contains("@hono/")
    })
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::NewExpression(new_expr) = node.kind() else { return };

        if ctx.file.path_segments.in_test_dir {
            return;
        }

        let Expression::Identifier(ctor) = &new_expr.callee else { return };
        if ctor.name.as_str() != "Error" {
            return;
        }

        if !is_route_file(ctx.source) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Bare `new Error()` in route handler \u{2014} use a structured error with `{ type, code, status, detail }`.".into(),
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
    fn flags_bare_error_in_route_file() {
        let src = r#"
import { Hono } from "hono";
app.get("/foo", (c) => {
    throw new Error("not found");
});
"#;
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_error_in_non_route_file() {
        let src = r#"
function validate(x: string) {
    throw new Error("invalid input");
}
"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn flags_multiple_errors() {
        let src = r#"
app.post("/bar", (c) => {
    if (!x) throw new Error("missing x");
    if (!y) throw new Error("missing y");
});
"#;
        assert_eq!(run_on(src).len(), 2);
    }
}
