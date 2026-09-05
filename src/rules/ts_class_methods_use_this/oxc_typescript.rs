use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{
    ClassShape, byte_offset_to_line_col, enclosing_class, is_protocol_slot_key,
};
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::borrow::Cow;
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

            // Skip members keyed by a protocol slot, in any member form: method
            // (`[Symbol.iterator]()`), generator (`async *[Symbol.asyncIterator]()`),
            // getter (`get [Symbol.toStringTag]()`) or setter. These must live on
            // the prototype to satisfy the contract that owns the key
            // (`Object.prototype.toString`, the iteration protocol, `Hash.hash()`
            // …); making them `static` puts the behavior on the constructor
            // instead and breaks the semantics, so absence of `this` is not a
            // smell.
            if is_protocol_slot_key(&method_def.key, semantic) {
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
            // class, implements an interface, or is `abstract`. With
            // `extends`/`implements`, the method may be required by the
            // base-class or interface contract (e.g. NestJS DI factories,
            // overrides), so making it `static` or extracting it would break
            // polymorphism. An `abstract class` is by definition designed to be
            // subclassed: its concrete methods are virtual defaults that
            // subclasses override (`override usesPivotTable()`), so they must
            // stay instance methods even when their current body omits `this` —
            // `static` methods cannot participate in `override` dispatch.
            //
            // Also skip methods that reference the enclosing class's own type
            // parameters in any type position (return type, parameter types, or
            // body type-argument lists). A `static` method cannot reference
            // class type parameters (TS2302), so a generic fluent-builder method
            // like `context<T>() { return new Builder<T, TMeta>(); }` legitimately
            // omits `this` yet cannot be made `static`.
            if let Some(class) = enclosing_class(node.id(), nodes) {
                let shape = ClassShape::of(class);
                if shape.is_decorated
                    || shape.has_super_class
                    || shape.has_implements
                    || shape.is_abstract
                {
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

            let label = method_label(method_def, ctx.source);

            let (line, column) =
                byte_offset_to_line_col(ctx.source, method_def.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Method `{label}` does not use `this` — make it `static` \
                     or extract to a standalone function."
                ),
                severity: Severity::Error,
                span: None,
            });
        }

        diagnostics
    }
}

/// Labels the method with the source text of its key, which the reader can
/// search for. The key span covers every form: `foo`, `#foo` (sigil included),
/// `'foo bar'`, `42`. A computed key gets its brackets back. A key written over
/// several lines folds onto one, because the text output holds one diagnostic
/// per line.
fn method_label<'a>(method_def: &oxc_ast::ast::MethodDefinition, source: &'a str) -> Cow<'a, str> {
    let span = method_def.key.span();
    let key_text = &source[span.start as usize..span.end as usize];
    let key_text: Cow<'a, str> = if key_text.contains(['\n', '\r']) {
        Cow::Owned(key_text.split_whitespace().collect::<Vec<_>>().join(" "))
    } else {
        Cow::Borrowed(key_text)
    };
    if method_def.computed {
        Cow::Owned(format!("[{key_text}]"))
    } else {
        key_text
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
    fn allows_symbol_iterator_method_without_this() {
        // Issue #8152: the iterable protocol is written as a plain method far more
        // often than as a getter. `static [Symbol.iterator]()` makes the class
        // iterable instead of its instances, so the remediation is wrong here for
        // exactly the reason it is wrong on the getter form.
        let src = "class Foo { [Symbol.iterator]() { return [][Symbol.iterator](); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_async_generator_symbol_async_iterator_method() {
        // Issue #8152: an async generator keyed by `Symbol.asyncIterator` is a
        // `MethodDefinition` like any other and must be exempt too.
        let src = "class Foo { async *[Symbol.asyncIterator]() { yield 1; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_imported_protocol_symbol_method() {
        // Issue #8151: `effect` publishes its protocol slots as its own symbols.
        // `Hash.hash(instance)` reads the member off the prototype, so
        // `static [Hash.symbol]()` makes it unreachable.
        let src = "import { Hash } from 'effect';\n\
                   class Hello {\n\
                   [Hash.symbol]() { return 0; }\n\
                   }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_bare_imported_symbol_key_method() {
        // Issue #8151: the same contract written without the namespace hop — the
        // key itself is the imported binding.
        let src = "import { NodeInspectSymbol } from 'effect';\n\
                   class Hello {\n\
                   [NodeInspectSymbol]() { return 'Hello'; }\n\
                   }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_local_unique_symbol_keyed_method() {
        // Issue #8151: a `unique symbol` binding exists to be an unforgeable
        // member key; the member it keys is a slot, not an ordinary method.
        let src = "const localSym: unique symbol = Symbol('local');\n\
                   class Foo { [localSym]() { return 1; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_local_symbol_call_keyed_method() {
        // Issue #8151: an un-annotated `const s = Symbol('s')` is symbol-valued
        // from its initializer alone.
        let src = "const localSym = Symbol('local');\n\
                   class Foo { [localSym]() { return 1; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_computed_key_from_local_string_constant() {
        // Negative space for #8151: the exemption must not degrade into "never
        // report a computed member". A key bound to a local string is an ordinary
        // method name, and the method is still a `static` candidate.
        let src = "const KEY = 'compute';\n\
                   class Foo { [KEY]() { return 1; } }";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`[KEY]`"));
    }

    #[test]
    fn flags_computed_key_from_imported_object_element_access() {
        // Negative space for #8151: import provenance is read from the key's root
        // identifier, and only through a static member access. A dynamic
        // `[names[0]]` proves nothing about the key and stays flagged.
        let src = "import { names } from './names';\n\
                   class Foo { [names[0]]() { return 1; } }";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
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

    #[test]
    fn allows_concrete_method_in_abstract_class() {
        // Issue #6990: a concrete method in an `abstract class` with no `extends`
        // clause is a virtual default that subclasses override
        // (`AbstractSqlPlatform extends Platform { override usesPivotTable() … }`).
        // Making it `static` would break the override, so absence of `this` is
        // not a smell — even though the class itself has no parent.
        let src = "abstract class Platform {\n\
                   usesPivotTable(): boolean { return false; }\n\
                   usesImplicitTransactions(): boolean { return true; }\n\
                   }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn names_private_methods_with_their_hash_sigil() {
        // Issue #6818: `#isJsonContentType` / `#getCurrentTime` are `#private`
        // methods of `sindresorhus/ky`'s `Ky` class. Both are still candidates
        // for `static`, but the diagnostic must name them so the reader can find
        // them without cross-referencing the line number.
        let src = "class Ky {\n\
                   #isJsonContentType(contentType: string): boolean {\n\
                   const mimeType = (contentType.split(';', 1)[0] ?? '').trim().toLowerCase();\n\
                   return /\\/(?:.*[.+-])?json$/.test(mimeType);\n\
                   }\n\
                   #getCurrentTime(): number {\n\
                   return globalThis.performance?.now() ?? Date.now();\n\
                   }\n\
                   }";
        let diags = run_on(src);
        assert_eq!(diags.len(), 2);
        assert!(diags[0].message.contains("`#isJsonContentType`"));
        assert!(diags[1].message.contains("`#getCurrentTime`"));
    }

    #[test]
    fn allows_private_method_using_this() {
        // Negative space for #6818: a `#private` method that reads `this` stays
        // silent.
        let src = "class Ky { #currentTime(): number { return this.now; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn names_literal_keyed_methods() {
        // A string or numeric key renders as written.
        let diags = run_on("class Foo { 'do work'() { return 1; } 42() { return 2; } }");
        assert_eq!(diags.len(), 2);
        assert!(diags[0].message.contains("`'do work'`"));
        assert!(diags[1].message.contains("`42`"));
    }

    #[test]
    fn names_computed_method_with_its_brackets() {
        // A computed key has no name, so the diagnostic shows the expression as
        // written, brackets included.
        let diags = run_on("class Foo { [KEY]() { return 1; } }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`[KEY]`"));
    }

    #[test]
    fn names_multiline_computed_method_on_one_line() {
        // A computed key written across several lines renders on one.
        let src = "class Foo {\n\
                   [KEY\n\
                   .toUpperCase()]() { return 1; }\n\
                   }";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`[KEY .toUpperCase()]`"));
        assert!(!diags[0].message.contains('\n'));
    }

    #[test]
    fn names_line_continuation_string_keyed_method_on_one_line() {
        // A string key split with a line continuation folds onto one line too.
        let diags = run_on("class Foo { 'multi\\\nline'() { return 1; } }");
        assert_eq!(diags.len(), 1);
        assert!(!diags[0].message.contains('\n'));
    }

    #[test]
    fn flags_same_method_in_non_abstract_class() {
        // Negative space for #6990: the exemption is scoped to `abstract class`.
        // The same `this`-free method in a plain (non-abstract) class is still a
        // candidate for `static`.
        let diags = run_on("class Platform { usesPivotTable(): boolean { return false; } }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("usesPivotTable"));
    }
}
