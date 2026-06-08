//! no-primitive-wrappers oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const WRAPPER_TYPES: &[&str] = &["String", "Number", "Boolean"];

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

        let Expression::Identifier(ident) = &new_expr.callee else { return };
        let name = ident.name.as_str();
        if !WRAPPER_TYPES.contains(&name) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Primitive wrapper object detected — `new {name}(...)` creates an object, not a primitive. Use `{name}(...)` without `new`.",
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_new_string() {
        assert_eq!(run(r#"const s = new String("hello");"#).len(), 1);
    }


    #[test]
    fn flags_new_number() {
        assert_eq!(run("const n = new Number(42);").len(), 1);
    }


    #[test]
    fn flags_new_boolean() {
        assert_eq!(run("const b = new Boolean(true);").len(), 1);
    }


    #[test]
    fn allows_factory_calls() {
        assert!(run(r#"const s = String("hello");"#).is_empty());
        assert!(run("const n = Number(42);").is_empty());
        assert!(run("const b = Boolean(0);").is_empty());
    }


    #[test]
    fn allows_unrelated_new() {
        assert!(run("const m = new Map();").is_empty());
    }
}
