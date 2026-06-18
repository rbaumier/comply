use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{ClassShape, byte_offset_to_line_col, enclosing_class};
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [crate::rules::backend::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let nodes = semantic.nodes();

        for node in nodes.iter() {
            let method_def = match node.kind() {
                AstKind::MethodDefinition(m) => m,
                _ => continue,
            };

            // Skip static methods
            if method_def.r#static {
                continue;
            }

            // Skip constructors
            if method_def.kind == oxc_ast::ast::MethodDefinitionKind::Constructor {
                continue;
            }

            // Skip getters keyed by a well-known protocol symbol (e.g.
            // `get [Symbol.toStringTag]()`, `get [Symbol.iterator]()`). These
            // must live on the prototype to satisfy the protocol contract
            // (`Object.prototype.toString`, the iteration protocol, …); making
            // them `static` puts the behavior on the constructor instead and
            // breaks the semantics, so absence of `this` is not a smell.
            if method_def.kind == oxc_ast::ast::MethodDefinitionKind::Get
                && is_symbol_member_key(&method_def.key)
            {
                continue;
            }

            // Skip abstract methods (no body)
            let Some(body) = &method_def.value.body else {
                continue;
            };

            // Skip `override` methods: they fulfill a base-class contract, so
            // making them `static` would break the override even when the body
            // happens not to reference `this`.
            if method_def.r#override {
                continue;
            }

            // Skip no-op / not-implemented stubs: an empty body, or a body whose
            // only statement is a `throw` (e.g. `throw new Error('not
            // implemented')`). These exist to satisfy a signature so subclasses
            // or interface implementors can override them; `static` is wrong.
            if is_stub_body(body) {
                continue;
            }

            // Skip decorated methods
            if !method_def.decorators.is_empty() {
                continue;
            }

            // Skip methods whose enclosing class is decorated, extends a base
            // class, or implements an interface. With `extends`/`implements`,
            // the method may be required by the base-class or interface
            // contract (e.g. NestJS DI factories, overrides), so making it
            // `static` or extracting it would break polymorphism.
            //
            // Also skip methods that reference the enclosing class's own type
            // parameters in any type position (return type, parameter types, or
            // body type-argument lists). A `static` method cannot reference
            // class type parameters (TS2302), so a generic fluent-builder method
            // like `context<T>() { return new Builder<T, TMeta>(); }` legitimately
            // omits `this` yet cannot be made `static`.
            if let Some(class) = enclosing_class(node.id(), nodes) {
                let shape = ClassShape::of(class);
                if shape.is_decorated || shape.has_super_class || shape.has_implements {
                    continue;
                }
                if method_references_class_type_param(method_def.span, class, nodes) {
                    continue;
                }
            }

            // Check if body contains `this`
            if body_contains_this(method_def.span.start, nodes) {
                continue;
            }

            let name = match &method_def.key {
                oxc_ast::ast::PropertyKey::StaticIdentifier(ident) => ident.name.as_str(),
                _ => "<computed>",
            };

            let (line, column) =
                byte_offset_to_line_col(ctx.source, method_def.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Method `{name}` does not use `this` — make it `static` \
                     or extract to a standalone function."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }

        diagnostics
    }
}

/// Whether a method body is a no-op / not-implemented stub: either empty or a
/// single `throw` statement. Such bodies exist only to satisfy a signature so
/// subclasses or interface implementors can override them, so the absence of
/// `this` is not a smell.
fn is_stub_body(body: &oxc_ast::ast::FunctionBody) -> bool {
    match body.statements.as_slice() {
        [] => true,
        [stmt] => matches!(stmt, oxc_ast::ast::Statement::ThrowStatement(_)),
        _ => false,
    }
}

/// Whether a computed property key is a member access on the global `Symbol`,
/// e.g. `[Symbol.toStringTag]` or `[Symbol.iterator]`.
fn is_symbol_member_key(key: &oxc_ast::ast::PropertyKey) -> bool {
    let oxc_ast::ast::PropertyKey::StaticMemberExpression(member) = key else {
        return false;
    };
    matches!(&member.object, oxc_ast::ast::Expression::Identifier(id) if id.name == "Symbol")
}

/// Whether the method references any of the enclosing class's type parameters
/// in a type position. A `static` method cannot reference class type parameters
/// (TS2302), so such a method cannot be made `static` even when its body omits
/// `this`.
///
/// Class type parameters are matched by name against every `TSTypeReference`
/// whose span falls inside the method — this covers return types, parameter type
/// annotations, and body type-argument lists (`new Builder<T, TMeta>()`)
/// uniformly. Returns `false` when the class has no type parameters.
fn method_references_class_type_param(
    method_span: oxc_span::Span,
    class: &oxc_ast::ast::Class,
    nodes: &oxc_semantic::AstNodes,
) -> bool {
    let Some(type_params) = &class.type_parameters else {
        return false;
    };
    if type_params.params.is_empty() {
        return false;
    }

    for node in nodes.iter() {
        let AstKind::TSTypeReference(type_ref) = node.kind() else {
            continue;
        };
        if type_ref.span.start < method_span.start || type_ref.span.end > method_span.end {
            continue;
        }
        let oxc_ast::ast::TSTypeName::IdentifierReference(ident) = &type_ref.type_name else {
            continue;
        };
        if type_params
            .params
            .iter()
            .any(|param| param.name.name == ident.name)
        {
            return true;
        }
    }
    false
}

/// Check if any descendant of the method body references `this`, stopping at
/// nested function/class boundaries.
fn body_contains_this(
    method_span_start: u32,
    nodes: &oxc_semantic::AstNodes,
) -> bool {
    for child in nodes.iter() {
        if !matches!(child.kind(), AstKind::ThisExpression(_)) {
            continue;
        }
        // Walk up from this `this` expression to see if it belongs to our method.
        // The hierarchy is: MethodDefinition -> Function -> FunctionBody -> ...
        // The method's own Function is the one that binds `this` for the method,
        // so we allow it. We stop at OTHER Function/Class nodes.
        let mut current = child.id();
        let mut found_method = false;
        loop {
            let parent_id = nodes.parent_id(current);
            if parent_id == current {
                break;
            }
            let parent = nodes.get_node(parent_id);
            match parent.kind() {
                AstKind::MethodDefinition(m) if m.span.start == method_span_start => {
                    found_method = true;
                    break;
                }
                // Arrow functions don't rebind `this` — continue upward
                AstKind::ArrowFunctionExpression(_) => {}
                // The method's own Function node is the direct child of MethodDefinition.
                // Check if the grandparent is our MethodDefinition.
                AstKind::Function(_) => {
                    let gp_id = nodes.parent_id(parent_id);
                    if gp_id != parent_id {
                        let gp = nodes.get_node(gp_id);
                        if let AstKind::MethodDefinition(m) = gp.kind()
                            && m.span.start == method_span_start {
                                // This is the method's own function — allow
                                current = parent_id;
                                continue;
                            }
                    }
                    // Different function — rebinds `this`
                    break;
                }
                AstKind::Class(_) => break,
                _ => {}
            }
            current = parent_id;
        }
        if found_method {
            return true;
        }
    }
    false
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_method_without_this() {
        let diags = run_on("class Foo { bar() { return 1; } }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("bar"));
    }

    #[test]
    fn allows_method_with_this() {
        assert!(run_on("class Foo { bar() { return this.x; } }").is_empty());
    }

    #[test]
    fn allows_static_method() {
        assert!(run_on("class Foo { static bar() { return 1; } }").is_empty());
    }

    #[test]
    fn allows_constructor() {
        assert!(run_on("class Foo { constructor() { const x = 1; } }").is_empty());
    }

    #[test]
    fn allows_decorated_method_without_this() {
        let src = "class Foo { @Get() bar() { return 1; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_methods_in_decorated_class_without_this() {
        let src = "@Controller()\nclass Foo { bar() { return 1; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_method_in_class_implementing_interface() {
        // Issue #972: NestJS factory pattern — `createGqlOptions` is required
        // by the `GqlOptionsFactory` interface and cannot be made static.
        let src = "class ConfigService implements GqlOptionsFactory {\n\
                   createGqlOptions() { return { typePaths: [] }; }\n\
                   }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_method_in_class_extending_base_class() {
        // Issue #972: `serializeError` overrides a method of the parent class.
        let src = "class ErrorHandlingProxy extends ClientGrpcProxy {\n\
                   serializeError(err) { return new RpcException(err); }\n\
                   }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_override_method_in_extends_class() {
        let src = "class Foo extends Bar { override baz() { return 1; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_well_known_symbol_getter_without_this() {
        // Issue #2049: `get [Symbol.toStringTag]()` must stay a prototype getter
        // so `Object.prototype.toString.call(instance)` works; making it static
        // changes the semantics, so absence of `this` is not a smell.
        let src = "class FakeGraphQLObjectType {\n\
                   get [Symbol.toStringTag]() { return 'GraphQLObjectType'; }\n\
                   }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_symbol_iterator_getter_without_this() {
        let src = "class Foo { get [Symbol.iterator]() { return function* () {}; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_plain_getter_without_this() {
        // The exemption is scoped to protocol-symbol getters; an ordinary getter
        // that never uses `this` is still a smell.
        let diags = run_on("class Foo { get bar() { return 1; } }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_interface_implementation_noop() {
        // Issue #1228: `init` is a no-op required by the `Driver` interface; it
        // must match the interface signature and cannot be made static.
        let src = "class DummyDriver implements Driver {\n\
                   async init(): Promise<void> {}\n\
                   }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_override_method_directly() {
        // Issue #1228: an `override` method extends a base-class contract; making
        // it static breaks the override.
        let src = "class Foo extends Bar { override baz(): void { doWork(); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_empty_method_body() {
        // Issue #1228: an empty body is a no-op stub, not a missing-`this` smell.
        let diags = run_on("class Foo { noop() {} }");
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_throw_only_stub() {
        // Issue #1228: a not-implemented stub whose body only throws must keep its
        // instance-method shape so subclasses/implementors can override it.
        let src = "class Foo {\n\
                   notImplemented() { throw new Error('not implemented'); }\n\
                   }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_plain_class_method_with_real_body() {
        // Negative space: a method in a plain class (no `implements`, not
        // `override`, with a real non-empty/non-throw body) that ignores `this`
        // is still flagged.
        let diags = run_on("class Foo { compute(): number { return 1 + 2; } }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("compute"));
    }

    #[test]
    fn allows_fluent_builder_referencing_class_type_params() {
        // Issue #3856: a generic fluent builder threads the class's own type
        // parameters (`TMeta`, `TContext`) through `new Builder<…>()` in the body.
        // A `static` method cannot reference class type parameters (TS2302), so
        // neither method can be made static even though neither uses `this`.
        let src = "class Builder<TContext, TMeta> {\n\
                   context<TNewContext>() { return new Builder<TNewContext, TMeta>(); }\n\
                   meta<TNewMeta>() { return new Builder<TContext, TNewMeta>(); }\n\
                   }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_method_with_class_type_param_in_parameter_type() {
        // Issue #3856: a class type parameter referenced in a parameter type
        // annotation also blocks `static`.
        let src = "class C<T> { foo(x: T) { return 1; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_method_with_class_type_param_in_return_type() {
        // Issue #3856: a class type parameter referenced in the return type also
        // blocks `static`.
        let src = "class C<T> { foo(): T | undefined { return undefined; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_generic_class_method_not_referencing_class_type_param() {
        // True positive: a method in a generic class that references NO class
        // type parameter and omits `this` can still be made static.
        let diags = run_on("class C<T> { foo() { return 42; } }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("foo"));
    }
}
