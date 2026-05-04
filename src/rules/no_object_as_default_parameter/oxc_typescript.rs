//! no-object-as-default-parameter — OXC backend.
//! Flags function parameters with non-empty object literal defaults.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression, FormalParameters};
use std::sync::Arc;

pub struct Check;

fn check_params(params: &FormalParameters, ctx: &CheckCtx, diagnostics: &mut Vec<Diagnostic>) {
    for param in &params.items {
        // The default value lives in `param.initializer` (OXC FormalParameter).
        let Some(init) = &param.initializer else { continue };

        // RHS must be a non-empty object expression.
        let Expression::ObjectExpression(obj) = init.as_ref() else { continue };
        if obj.properties.is_empty() {
            continue;
        }

        let param_name = match &param.pattern {
            BindingPattern::BindingIdentifier(id) => Some(id.name.as_str()),
            _ => None,
        };

        let message = match param_name {
            Some(name) => format!(
                "Do not use an object literal as default for parameter `{name}`. \
                 Use destructuring with individual defaults instead."
            ),
            None => "Do not use an object literal as default. \
                     Use destructuring with individual defaults instead."
                .to_string(),
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, param.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message,
            severity: Severity::Warning,
            span: None,
        });
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Function, AstType::ArrowFunctionExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::Function(f) => {
                check_params(&f.params, ctx, diagnostics);
            }
            AstKind::ArrowFunctionExpression(f) => {
                check_params(&f.params, ctx, diagnostics);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_object_default_in_function() {
        let d = run_on("function f(opts = { timeout: 1000 }) {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("opts"));
    }

    #[test]
    fn flags_object_default_in_arrow() {
        let d = run_on("const f = (opts = { retries: 3 }) => {};");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("opts"));
    }

    #[test]
    fn flags_object_default_in_method() {
        let d = run_on("class A { method(cfg = { debug: true }) {} }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_empty_object_default() {
        assert!(run_on("function f(opts = {}) {}").is_empty());
    }

    #[test]
    fn allows_destructured_default() {
        assert!(run_on("function f({ timeout = 1000 } = {}) {}").is_empty());
    }

    #[test]
    fn allows_primitive_default() {
        assert!(run_on("function f(x = 42) {}").is_empty());
    }

    #[test]
    fn allows_array_default() {
        assert!(run_on("function f(items = [1, 2]) {}").is_empty());
    }

    #[test]
    fn allows_assignment_in_body() {
        assert!(run_on("function f() { const x = { a: 1 }; }").is_empty());
    }
}
