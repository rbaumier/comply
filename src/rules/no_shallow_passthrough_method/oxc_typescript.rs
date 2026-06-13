//! no-shallow-passthrough-method oxc backend — flag methods whose body is a
//! single `return` forwarding the exact parameters to another callee.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, FormalParameters, Statement};
use std::sync::Arc;

pub struct Check;

fn param_names<'a>(params: &'a FormalParameters<'a>) -> Vec<&'a str> {
    let mut out = Vec::new();
    for item in &params.items {
        if let oxc_ast::ast::BindingPattern::BindingIdentifier(id) = &item.pattern {
            out.push(id.name.as_str());
        }
    }
    out
}

fn argument_names<'a>(args: &'a oxc_allocator::Vec<'a, oxc_ast::ast::Argument<'a>>) -> Option<Vec<&'a str>> {
    let mut out = Vec::new();
    for arg in args {
        match arg {
            oxc_ast::ast::Argument::Identifier(id) => {
                out.push(id.name.as_str());
            }
            _ => return None,
        }
    }
    Some(out)
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::MethodDefinition]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::MethodDefinition(method) = node.kind() else { return };

        // A decorated method carries external significance beyond its body: the
        // decorator binds it to a framework (e.g. NestJS `@MessagePattern` /
        // `@EventPattern` / `@Get`) that resolves the method via metadata
        // reflection at runtime. The forwarding body cannot be inlined or
        // removed without breaking that registration, so the passthrough is
        // intentional and required.
        if !method.decorators.is_empty() {
            return;
        }

        let Some(ref body) = method.value.body else { return };

        // Body must contain exactly one statement, a return statement.
        if body.statements.len() != 1 {
            return;
        }
        let Statement::ReturnStatement(ret) = &body.statements[0] else { return };
        let Some(ref expr) = ret.argument else { return };
        let Expression::CallExpression(call) = expr else { return };

        let Some(arg_names) = argument_names(&call.arguments) else { return };
        let params = param_names(&method.value.params);
        if params.is_empty() {
            return;
        }
        if params != arg_names {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, method.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Method is a pure pass-through — forwards the same arguments with no added logic. Inline the call or remove the indirection.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_passthrough() {
        let src = "class A { foo(a, b) { return this.bar(a, b); } }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_reordered_args() {
        let src = "class A { foo(a, b) { return this.bar(b, a); } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_decorated_message_handler() {
        // Regression for #2020: a NestJS `@MessagePattern` handler forwards its
        // parameter but the decorator registers it as an RPC entry point — it
        // cannot be inlined or removed.
        let src = "class NatsController { @MessagePattern('streaming.*') streaming(data) { return from(data); } }";
        assert!(run(src).is_empty(), "expected no diagnostics, got: {:?}", run(src));
    }
}
