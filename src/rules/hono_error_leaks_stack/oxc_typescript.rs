use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

fn inside_on_error<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    source: &str,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        if let AstKind::CallExpression(call) = ancestor.kind() {
            let callee_text =
                &source[call.callee.span().start as usize..call.callee.span().end as usize];
            if callee_text.ends_with(".onError") || callee_text == "onError" {
                return true;
            }
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::MemberExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["hono", "Hono"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !ctx.source_contains("hono") && !ctx.source_contains("Hono") {
            return;
        }

        let AstKind::MemberExpression(member) = node.kind() else {
            return;
        };

        let Some((prop_name, prop_span)) = member.static_property_info() else {
            return;
        };
        if prop_name != "stack" && prop_name != "message" {
            return;
        }

        // Object should be a simple identifier like err/error/e/exception
        let obj = member.object();
        let Expression::Identifier(obj_id) = obj else {
            return;
        };
        let obj_name = obj_id.name.as_str();
        if !matches!(obj_name, "err" | "error" | "e" | "exception") {
            return;
        }

        if !inside_on_error(node, semantic, ctx.source) {
            return;
        }

        let member_text =
            &ctx.source[member.span().start as usize..member.span().end as usize];
        let (line, column) =
            byte_offset_to_line_col(ctx.source, member.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Returning `{member_text}` from `onError` leaks internal error details to clients."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_err_stack() {
        let src = "import { Hono } from 'hono';\nconst app = new Hono();\napp.onError((err, c) => c.json({ stack: err.stack }));";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_err_message() {
        let src = "import { Hono } from 'hono';\nconst app = new Hono();\napp.onError((err, c) => c.json({ error: err.message }));";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_both() {
        let src = "import { Hono } from 'hono';\nconst app = new Hono();\napp.onError((err, c) => c.json({ error: err.message, stack: err.stack }));";
        assert_eq!(run(src).len(), 2);
    }


    #[test]
    fn allows_generic_message() {
        let src = "import { Hono } from 'hono';\nconst app = new Hono();\napp.onError((err, c) => c.json({ error: 'Internal Server Error' }, 500));";
        assert!(run(src).is_empty());
    }


    #[test]
    fn ignores_outside_on_error() {
        let src = "import { Hono } from 'hono';\nconst app = new Hono();\nfunction h(err: Error) { return err.message; }";
        assert!(run(src).is_empty());
    }
}
