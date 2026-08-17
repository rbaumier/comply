//! no-reflect-get oxc backend — flag a `Reflect.get(...)` call that does not
//! forward the key of the `Proxy` trap it sits in.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, identifier_is_unshadowed_global};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, BindingPattern, Expression, FormalParameters, PropertyKey};
use std::sync::Arc;

/// The `Proxy` trap that reads a property.
const TRAP: &str = "get";

/// The name a binding pattern introduces, when it is a plain identifier.
fn binding_name<'a>(pattern: &'a BindingPattern<'a>) -> Option<&'a str> {
    match pattern {
        BindingPattern::BindingIdentifier(id) => Some(id.name.as_str()),
        _ => None,
    }
}

/// True when `key` names the `get` trap.
fn key_is_trap(key: &PropertyKey) -> bool {
    match key {
        PropertyKey::StaticIdentifier(id) => id.name.as_str() == TRAP,
        PropertyKey::StringLiteral(s) => s.value.as_str() == TRAP,
        _ => false,
    }
}

/// The parameters of the nearest enclosing function, when that function is the
/// value of a method or property named `get`. That covers every handler shape —
/// an inline `new Proxy` argument, a detached literal with or without a
/// `ProxyHandler<T>` annotation, a `satisfies` clause, a class trap — because
/// none of them changes what the trap itself looks like.
fn enclosing_trap_params<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<&'a FormalParameters<'a>> {
    let nodes = semantic.nodes();
    let function = nodes.ancestors(node.id()).find(|ancestor| {
        matches!(
            ancestor.kind(),
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
        )
    })?;
    let params = match function.kind() {
        AstKind::Function(f) => &f.params,
        AstKind::ArrowFunctionExpression(f) => &f.params,
        _ => return None,
    };
    let owns_trap = match nodes.parent_node(function.id()).kind() {
        AstKind::ObjectProperty(property) => key_is_trap(&property.key),
        AstKind::MethodDefinition(method) => key_is_trap(&method.key),
        _ => false,
    };
    owns_trap.then_some(params)
}

/// True when `call` forwards the key of the `get` trap it sits in. Forwarding
/// the key is what makes a call a trap: the target is routinely recomputed —
/// resolved from an AsyncLocalStorage holder, a lazy connection — while the key
/// always comes straight from the trap's own parameter.
fn forwards_trap_key<'a>(
    call: &oxc_ast::ast::CallExpression<'a>,
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let Some(params) = enclosing_trap_params(node, semantic) else {
        return false;
    };

    // `get(...args) { return Reflect.get(...args); }` — the whole argument list
    // is the trap's rest binding, so there is no second argument to test.
    if let [Argument::SpreadElement(spread)] = call.arguments.as_slice() {
        let Expression::Identifier(spread_name) = &spread.argument else {
            return false;
        };
        return params
            .rest
            .as_ref()
            .and_then(|rest| binding_name(&rest.rest.argument))
            .is_some_and(|rest_name| rest_name == spread_name.name.as_str());
    }

    // `get(target, prop)` and `get(target, prop, receiver)` — the second
    // parameter is the key.
    if !(2..=3).contains(&params.items.len()) {
        return false;
    }
    let Some(Argument::Identifier(key_argument)) = call.arguments.get(1) else {
        return false;
    };
    params
        .items
        .get(1)
        .and_then(|parameter| binding_name(&parameter.pattern))
        .is_some_and(|key_parameter| key_parameter == key_argument.name.as_str())
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Reflect"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != TRAP {
            return;
        }
        let Expression::Identifier(object) = &member.object else {
            return;
        };
        if object.name.as_str() != "Reflect"
            || !identifier_is_unshadowed_global(object, semantic)
            || forwards_trap_key(call, node, semantic)
        {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`Reflect.get` reads a property without its name reaching the type \
                      system — read it on a typed value, or parse the input into a named \
                      domain type at its boundary."
                .into(),
            severity: Severity::Error,
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_literal_key_read() {
        assert_eq!(run(r#"const name = Reflect.get(target, "name");"#).len(), 1);
    }

    #[test]
    fn flags_dynamic_key_read() {
        assert_eq!(run("const v = Reflect.get(obj, key);").len(), 1);
    }

    #[test]
    fn flags_non_forwarding_call_inside_a_trap() {
        let src = r#"new Proxy(t, { get(target, prop) { return Reflect.get(store, "name"); } });"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_forwarding_call_outside_a_trap() {
        assert_eq!(run("function read(a, b) { return Reflect.get(a, b); }").len(), 1);
    }

    #[test]
    fn allows_inline_trap() {
        let src = "new Proxy(t, { get(target, prop, receiver) { return Reflect.get(target, prop, receiver); } });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_trap_omitting_the_receiver() {
        let src = "new Proxy(t, { get(target, prop, receiver) { return Reflect.get(target, prop); } });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_detached_annotated_handler() {
        // Regression for issue #652 (als-proxy.ts): the target is resolved from
        // an AsyncLocalStorage holder, only the key is forwarded.
        let src = r#"
            const handler: ProxyHandler<postgres.Sql> = {
                get(_target, prop) {
                    const source = als.getStore() ?? rawPg;
                    const propertyValue = Reflect.get(source, prop);
                    return propertyValue;
                },
            };
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_detached_unannotated_handler() {
        let src = "const h = { get(t, p) { return Reflect.get(t, p); } }; new Proxy(x, h);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_class_trap() {
        let src = "class H implements ProxyHandler<T> { get(t, p, r) { return Reflect.get(t, p, r); } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_spread_forwarding() {
        let src = "new Proxy(t, { get(...args) { return Reflect.get(...args); } });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_reflect_apply() {
        assert!(run("Reflect.apply(fn, null, args);").is_empty());
    }

    #[test]
    fn allows_other_reflect_methods() {
        assert!(run("Reflect.getOwnPropertyDescriptor(o, k);").is_empty());
        assert!(run("Reflect.has(o, k);").is_empty());
    }

    #[test]
    fn allows_shadowed_binding() {
        assert!(run("const Reflect = makeThing(); Reflect.get(a, b);").is_empty());
    }

    #[test]
    fn allows_ordinary_get_method() {
        assert!(run("store.get(key);").is_empty());
    }
}
