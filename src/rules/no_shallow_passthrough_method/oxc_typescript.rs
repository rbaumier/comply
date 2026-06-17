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
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::MethodDefinition(method) = node.kind() else { return };

        // A method in a class with a heritage clause (`extends` or `implements`)
        // may be a polymorphic override that the superclass invokes by name, so
        // it cannot be inlined (the call sites live in the base class) or removed
        // (that reverts to base behaviour). This backend cannot resolve the base
        // class to prove the method is not such an override, so it stays
        // conservative and skips the whole class.
        for ancestor in semantic.nodes().ancestors(node.id()) {
            if let AstKind::Class(class) = ancestor.kind() {
                if class.super_class.is_some() || !class.implements.is_empty() {
                    return;
                }
                break;
            }
        }

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

        // The delegation must target a bare `this.<other>(...)`. When the
        // receiver is itself a call or member chain (e.g. knex's
        // `this._bool('or').whereRaw(...)`), the intervening call mutates state
        // before delegating, so the method is behaviourally distinct from a
        // direct call — not a shallow pass-through.
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if !matches!(&member.object, Expression::ThisExpression(_)) {
            return;
        }

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

    #[test]
    fn allows_mutate_then_delegate_chain() {
        // Regression for #2337: knex query builder `or*` / `not*` variants first
        // mutate boolean state via `this._bool('or')` / `this._not(true)`, then
        // delegate. The receiver of the delegation is the result of that state-
        // mutating call, not a bare `this`, so it is not a shallow pass-through.
        let bool_chain = "class QB { orWhereRaw(sql, bindings) { return this._bool('or').whereRaw(sql, bindings); } }";
        assert!(run(bool_chain).is_empty(), "expected no diagnostics, got: {:?}", run(bool_chain));

        let not_chain = "class QB { whereNotExists(callback) { return this._not(true).whereExists(callback); } }";
        assert!(run(not_chain).is_empty(), "expected no diagnostics, got: {:?}", run(not_chain));
    }

    #[test]
    fn allows_override_in_class_with_heritage_clause() {
        // Regression for #3921: a method in a class that `extends` or
        // `implements` may be a polymorphic override the superclass invokes by
        // name. It cannot be inlined (call sites live in the base) or removed
        // (reverts to base behaviour), so the class is skipped entirely.
        let extends = "class C extends Base { foo(x) { return this.bar(x); } }";
        assert!(run(extends).is_empty(), "expected no diagnostics, got: {:?}", run(extends));

        let implements = "class C implements I { foo(x) { return this.bar(x); } }";
        assert!(run(implements).is_empty(), "expected no diagnostics, got: {:?}", run(implements));

        // The babel ESTree mixin shape: the override name deliberately differs
        // from the callee because it is an override hook, not a rename alias.
        let babel_mixin = "class M extends superClass implements Parser { parseStringLiteral(v) { return this.estreeParseLiteral(v); } }";
        assert!(run(babel_mixin).is_empty(), "expected no diagnostics, got: {:?}", run(babel_mixin));
    }

    #[test]
    fn flags_passthrough_in_standalone_class() {
        // A standalone class (no `extends`, no `implements`) has no base class to
        // invoke the method polymorphically, so a shallow pass-through is still a
        // deletable wrapper and must flag.
        let src = "class C { foo(x) { return this.bar(x); } }";
        assert_eq!(run(src).len(), 1, "expected one diagnostic, got: {:?}", run(src));
    }

    #[test]
    fn allows_active_record_repository_delegation() {
        // Regression for #2368: TypeORM `BaseEntity` Active Record static methods
        // delegate via `this.getRepository().<op>(...)`. The receiver of the
        // delegated call is a `CallExpression` (the repository lookup), not a
        // bare `this`, so the registry resolution is real logic — not a shallow
        // pass-through.
        let find = "class BaseEntity { static findActive() { return this.getRepository().find({ where: { active: true } }); } }";
        assert!(run(find).is_empty(), "expected no diagnostics, got: {:?}", run(find));

        let delete_arg = "class BaseEntity { static delete(criteria) { return this.getRepository().delete(criteria); } }";
        assert!(run(delete_arg).is_empty(), "expected no diagnostics, got: {:?}", run(delete_arg));

        let generic_lookup = "class BaseEntity { static getId(entity) { return this.getRepository<T>().getId(entity); } }";
        assert!(run(generic_lookup).is_empty(), "expected no diagnostics, got: {:?}", run(generic_lookup));
    }
}
