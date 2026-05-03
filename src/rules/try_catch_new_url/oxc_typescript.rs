//! OxcCheck backend for try-catch-new-url.
//!
//! Flags `new URL(...)` not wrapped in a try block.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["new URL"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        use oxc_ast::ast::Expression;

        let AstKind::NewExpression(new_expr) = node.kind() else {
            return;
        };

        // Constructor must be `URL`
        let Expression::Identifier(id) = &new_expr.callee else {
            return;
        };
        if id.name.as_str() != "URL" {
            return;
        }

        if is_inside_try_body(node, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`new URL(...)` throws on invalid input — wrap in try/catch \
                      or gate with `URL.canParse(s)` first."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_inside_try_body<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::TryStatement(_) => return true,
            // Stop at function boundary
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => return false,
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_bare_new_url() {
        let d = run_on("const u = new URL(input);");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "try-catch-new-url");
    }

    #[test]
    fn flags_new_url_in_fn() {
        let d = run_on("function f(s: string) { return new URL(s); }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_inside_try() {
        assert!(run_on("try { const u = new URL(input); } catch (e) { log(e); }").is_empty());
    }

    #[test]
    fn allows_other_constructors() {
        assert!(run_on("const u = new MyUrl(input);").is_empty());
    }
}
