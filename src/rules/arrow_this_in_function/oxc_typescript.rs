//! OxcCheck backend for arrow-this-in-function.
//!
//! Flags `this` inside an arrow function that has no enclosing regular
//! function/method to bind `this`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            let AstKind::ThisExpression(this_expr) = node.kind() else {
                continue;
            };

            if !is_in_unbound_arrow(node, semantic) {
                continue;
            }

            let (line, column) =
                byte_offset_to_line_col(ctx.source, this_expr.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`this` inside an arrow function with no enclosing regular \
                          function or method — arrow functions don't bind their own \
                          `this`."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }

        diagnostics
    }
}

fn is_in_unbound_arrow<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let mut saw_arrow = false;
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::ArrowFunctionExpression(_) => {
                saw_arrow = true;
            }
            AstKind::Function(_) | AstKind::MethodDefinition(_) => {
                return false;
            }
            _ => {}
        }
    }
    saw_arrow
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_top_level_arrow_with_this() {
        let diags = run_on("const f = () => { console.log(this); };");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_arrow_nested_only_in_arrow() {
        let diags = run_on("const f = () => () => this;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_arrow_inside_class_method() {
        assert!(run_on("class Foo { bar() { return () => this.x; } }").is_empty());
    }

    #[test]
    fn allows_arrow_inside_function_declaration() {
        assert!(run_on("function foo() { return () => this; }").is_empty());
    }

    #[test]
    fn allows_arrow_inside_function_expression() {
        assert!(run_on("const o = { m: function () { return () => this; } };").is_empty());
    }

    #[test]
    fn ignores_plain_this_without_arrow() {
        assert!(run_on("function foo() { return this; }").is_empty());
    }
}
