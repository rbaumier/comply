//! no-shallow-passthrough-method oxc backend — flag methods whose body is a
//! single `return` forwarding the exact parameters to another callee.
//!
//! Methods that are the implementation of a TypeScript overload set (the class
//! has a bodyless sibling `MethodDefinition` with the same name) are exempt:
//! inlining or removing the implementation would erase the overload signatures
//! and the type-level discrimination they provide.
//!
//! Methods that fan out to a shared helper are exempt: when two or more sibling
//! methods in the same class each forward to the same `this.<target>(...)`, the
//! distinct method names are an extension-seam (template-method) — each is a
//! separate override point a subclass can specialise without touching the
//! others. Inlining or removing one would collapse that override granularity.
//!
//! Methods whose call contract differs from the target's are exempt: exposing
//! fewer parameters than a same-class target on the same side (static or
//! instance) accepts fixes a strictly narrower contract, and defaulting or
//! marking optional a parameter the target requires accepts calls the target
//! rejects. Either way the two are not interchangeable.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, node_has_preceding_deprecated_tag};
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

/// The static key name of a method (`Identifier` or `StringLiteral` key), or
/// `None` for computed/other keys.
fn method_key_name<'a>(method: &'a oxc_ast::ast::MethodDefinition<'a>) -> Option<&'a str> {
    match &method.key {
        oxc_ast::ast::PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
        oxc_ast::ast::PropertyKey::StringLiteral(s) => Some(s.value.as_str()),
        _ => None,
    }
}

/// True when `class` has a `MethodDefinition` with the same key as `method` but
/// NO body — i.e. a TypeScript overload signature, making `method` the
/// implementation of an overload set.
fn class_has_overload_signature_for<'a>(
    class: &'a oxc_ast::ast::Class<'a>,
    method: &'a oxc_ast::ast::MethodDefinition<'a>,
) -> bool {
    let Some(name) = method_key_name(method) else {
        return false;
    };
    class.body.body.iter().any(|element| {
        matches!(element, oxc_ast::ast::ClassElement::MethodDefinition(other)
            if other.value.body.is_none() && method_key_name(other) == Some(name))
    })
}

/// The name of the `this.<target>` method a shallow pass-through forwards to,
/// i.e. when the body is a single `return this.<target>(p0, p1, …)` whose
/// arguments are exactly the method's own parameters in order. `None` for any
/// method that is not such a pass-through (the existing flag conditions).
fn passthrough_target_name<'a>(method: &'a oxc_ast::ast::MethodDefinition<'a>) -> Option<&'a str> {
    let body = method.value.body.as_ref()?;
    if body.statements.len() != 1 {
        return None;
    }
    let Statement::ReturnStatement(ret) = &body.statements[0] else { return None };
    let Expression::CallExpression(call) = ret.argument.as_ref()? else { return None };
    let Expression::StaticMemberExpression(member) = &call.callee else { return None };
    if !matches!(&member.object, Expression::ThisExpression(_)) {
        return None;
    }
    let arg_names = argument_names(&call.arguments)?;
    let params = param_names(&method.value.params);
    if params.is_empty() || params != arg_names {
        return None;
    }
    Some(member.property.name.as_str())
}

/// True when at least one OTHER method in `class` is also a shallow pass-through
/// forwarding to `target` (the same `this.<target>(...)` helper). Two or more
/// distinct method names converging on one helper is the extension-seam
/// (template-method) signal: the separate names are override points, not a
/// deletable wrapper.
fn class_has_sibling_passthrough_to<'a>(
    class: &'a oxc_ast::ast::Class<'a>,
    method: &'a oxc_ast::ast::MethodDefinition<'a>,
    target: &str,
) -> bool {
    class.body.body.iter().any(|element| {
        let oxc_ast::ast::ClassElement::MethodDefinition(other) = element else {
            return false;
        };
        // Skip the method under inspection itself (compare by identity).
        if std::ptr::eq(other.as_ref(), method) {
            return false;
        }
        passthrough_target_name(other) == Some(target)
    })
}

/// The number of arguments a formal-parameter list accepts. A rest element
/// (`...args`) accepts an unbounded count, reported as [`usize::MAX`].
fn accepted_argument_count(params: &FormalParameters) -> usize {
    if params.rest.is_some() { usize::MAX } else { params.items.len() }
}

/// The number of arguments a formal-parameter list requires. A parameter with a
/// default value or an `?` marker may be omitted, as may every parameter after
/// it.
fn required_argument_count(params: &FormalParameters) -> usize {
    params
        .items
        .iter()
        .take_while(|item| item.initializer.is_none() && !item.optional)
        .count()
}

/// The declarations of `target` that `this` reaches from `method`: members of
/// the same class body on the same side — static or instance, the two sides
/// `this` can reach.
fn class_declarations_of<'a>(
    class: &'a oxc_ast::ast::Class<'a>,
    method: &'a oxc_ast::ast::MethodDefinition<'a>,
    target: &'a str,
) -> impl Iterator<Item = &'a oxc_ast::ast::MethodDefinition<'a>> {
    class.body.body.iter().filter_map(move |element| {
        let oxc_ast::ast::ClassElement::MethodDefinition(member) = element else {
            return None;
        };
        (member.r#static == method.r#static && method_key_name(member) == Some(target)).then(|| member.as_ref())
    })
}

/// The argument counts a class publishes for a forwarding target.
#[derive(Clone, Copy)]
struct TargetArity {
    /// The fewest arguments a caller must supply.
    fewest_required: usize,
    /// The most arguments a caller may supply.
    most_accepted: usize,
}

/// The call contract `class` publishes for `target`, or `None` when it declares
/// no such member. Of an overload set only the bodyless signatures are callable
/// — the bodied implementation covers their union and no caller resolves against
/// it — so the signatures win whenever both are present.
fn class_target_arity(
    class: &oxc_ast::ast::Class,
    method: &oxc_ast::ast::MethodDefinition,
    target: &str,
) -> Option<TargetArity> {
    let mut overload_signatures: Option<TargetArity> = None;
    let mut implementation: Option<TargetArity> = None;
    for member in class_declarations_of(class, method, target) {
        let declared = TargetArity {
            fewest_required: required_argument_count(&member.value.params),
            most_accepted: accepted_argument_count(&member.value.params),
        };
        let widest = if member.value.body.is_none() { &mut overload_signatures } else { &mut implementation };
        *widest = Some(match *widest {
            Some(seen) => TargetArity {
                fewest_required: seen.fewest_required.min(declared.fewest_required),
                most_accepted: seen.most_accepted.max(declared.most_accepted),
            },
            None => declared,
        });
    }
    overload_signatures.or(implementation)
}

/// True when the target accepts more arguments than `params` exposes. The
/// wrapper then fixes a strictly narrower contract: it hides parameters the
/// target accepts — typically an `@internal` optimisation argument — so its
/// callers cannot reach them.
fn narrows_target_contract(params: &FormalParameters, target: Option<TargetArity>) -> bool {
    target.is_some_and(|arity| arity.most_accepted > accepted_argument_count(params))
}

/// True when `params` tolerates an omission the target does not: a parameter
/// carrying a default value or an `?` marker where the target requires an
/// argument. Materialising the missing value is the wrapper's own behaviour, so
/// the two are not interchangeable. A target the class does not declare leaves
/// the comparison unprovable, and the wrapper keeps the credit for the
/// tolerance it declares.
fn widens_target_contract(params: &FormalParameters, target: Option<TargetArity>) -> bool {
    let required = required_argument_count(params);
    required < params.items.len() && target.is_none_or(|arity| arity.fewest_required > required)
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
        let mut enclosing_class: Option<&'a oxc_ast::ast::Class<'a>> = None;
        for ancestor in semantic.nodes().ancestors(node.id()) {
            if let AstKind::Class(class) = ancestor.kind() {
                if class.super_class.is_some() || !class.implements.is_empty() {
                    return;
                }
                // TypeScript overload implementation: a sibling `MethodDefinition`
                // with the same key and no body is a type-only overload signature.
                // The body here is the single callable implementation; inlining or
                // removing it would erase the overload signatures and the type
                // discrimination they provide.
                if class_has_overload_signature_for(class, method) {
                    return;
                }
                enclosing_class = Some(class);
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

        // A method whose own leading JSDoc carries an `@deprecated` tag is a
        // deliberately-retained public-API alias: the name is an external
        // contract that outlives its trivial forwarding body (e.g. graphql-js
        // `GraphQLEnumType.serialize` forwarding to `coerceOutputValue` during a
        // deprecation window). Inlining is impossible (call sites are external
        // consumers) and removal is the breaking change the deprecation defers,
        // so the passthrough is intentional — the same class of external
        // significance as a decorator above.
        if node_has_preceding_deprecated_tag(semantic.comments(), ctx.source, method.span.start as usize) {
            return;
        }

        // The body must be a single `return this.<target>(p0, p1, …)` whose
        // arguments are exactly the method's own parameters in order. A receiver
        // that is itself a call or member chain (e.g. knex's
        // `this._bool('or').whereRaw(...)`) mutates state before delegating, so
        // it is behaviourally distinct — `passthrough_target_name` rejects it.
        let Some(target) = passthrough_target_name(method) else { return };

        // Extension-seam (template-method) fan-out: when a sibling method in the
        // same class also forwards to the same `this.<target>(...)` helper, the
        // distinct method names are separate override points — a subclass can
        // specialise one (e.g. only `styleOptionDescription`) without touching
        // the others. Collapsing them into the shared helper would erase that
        // override granularity, so the pass-through is intentional. A lone
        // wrapper with no sibling sharing its target is still a deletable
        // indirection and keeps flagging.
        if let Some(class) = enclosing_class
            && class_has_sibling_passthrough_to(class, method, target)
        {
            return;
        }

        // Narrowing facade: the target accepts more arguments than the wrapper
        // exposes (e.g. vite's `ensureEntryFromUrl` forwards to an `@internal`
        // `_ensureEntryFromUrl` that takes an extra optimisation argument). The
        // wrapper fixes a strictly narrower contract, so inlining it would let
        // callers pass what it deliberately hides.
        //
        // Widening facade: the wrapper defaults or marks optional a parameter
        // the target requires (e.g. minisearch's `vacuum(options = {})` over a
        // `conditionalVacuum` that insists on the argument). Callers of the
        // wrapper may omit what callers of the target may not, so inlining the
        // call does not compile and deleting the wrapper drops the default.
        //
        // A wrapper whose contract matches its target's adds nothing and keeps
        // flagging.
        let target_arity = enclosing_class.and_then(|class| class_target_arity(class, method, target));
        let params = &method.value.params;
        if narrows_target_contract(params, target_arity) || widens_target_contract(params, target_arity) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, method.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Method is a pure pass-through — forwards the same arguments with no added logic. Inline the call or remove the indirection.".into(),
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
    fn allows_deprecated_forwarding_alias() {
        // Regression for #3905: graphql-js `GraphQLEnumType.serialize` /
        // `parseValue` are `@deprecated` public-API aliases forwarding to their
        // renamed replacements during a deprecation window. The method name is
        // an external contract — it cannot be inlined (call sites are external
        // consumers) nor removed (that is the breaking change the deprecation
        // defers), so the forwarding body is intentional.
        let serialize = "class GraphQLEnumType { /** @deprecated use `coerceOutputValue()` instead, `serialize()` will be removed in v18 */ serialize(outputValue) { return this.coerceOutputValue(outputValue); } }";
        assert!(run(serialize).is_empty(), "expected no diagnostics, got: {:?}", run(serialize));

        let parse_value = "class GraphQLEnumType { /** @deprecated use `coerceInputValue()` instead, `parseValue()` will be removed in v18 */ parseValue(inputValue, hideSuggestions) { return this.coerceInputValue(inputValue, hideSuggestions); } }";
        assert!(run(parse_value).is_empty(), "expected no diagnostics, got: {:?}", run(parse_value));
    }

    #[test]
    fn flags_passthrough_with_non_deprecated_jsdoc() {
        // A leading JSDoc that does NOT carry `@deprecated` does not mark the
        // name as an external contract, so a shallow pass-through still flags.
        let src = "class A { /** does a thing */ foo(a, b) { return this.bar(a, b); } }";
        assert_eq!(run(src).len(), 1, "expected one diagnostic, got: {:?}", run(src));
    }

    #[test]
    fn flags_passthrough_when_deprecated_tag_belongs_to_another_method() {
        // The `@deprecated` tag here is the leading JSDoc of `other`, not of the
        // pass-through `foo` below it. Only the method's OWN immediately-
        // preceding comment exempts it, so `foo` must still flag.
        let src = "class A { /** @deprecated */ other() { return 1; } foo(a, b) { return this.bar(a, b); } }";
        assert_eq!(run(src).len(), 1, "expected one diagnostic, got: {:?}", run(src));
    }

    #[test]
    fn allows_overload_implementation() {
        // Regression for #4415: the implementation of a TypeScript overload set
        // (here `refine`) forwards to a private helper that accepts the widest
        // type. The bodyless sibling signatures provide type-level
        // discrimination — inlining or removing the implementation would erase
        // them, so it cannot be flagged as a shallow pass-through.
        let src = "class Schema { refine<R>(fn: (x: unknown) => x is R): A; refine(fn: (x: unknown) => void): B; refine(fn: (x: unknown) => unknown): A | B { return this._refine(fn); } }";
        assert!(run(src).is_empty(), "expected no diagnostics, got: {:?}", run(src));
    }

    #[test]
    fn flags_passthrough_when_bodyless_sibling_has_different_name() {
        // A bodyless sibling with a DIFFERENT name (`baz`) is not an overload
        // signature for `bar`, so `bar` is still a deletable shallow
        // pass-through and must flag. Proves the name match is required.
        let src = "class Foo { baz(x): void; bar(x) { return this._bar(x); } }";
        assert_eq!(run(src).len(), 1, "expected one diagnostic, got: {:?}", run(src));
    }

    #[test]
    fn allows_fan_out_override_hooks() {
        // Regression for #5023: commander.js `Help` class exposes documented
        // override hooks — `styleCommandDescription` / `styleOptionDescription` /
        // … each forward to a shared `styleDescriptionText`. The separate names
        // are the override-point API surface (a subclass can restyle one element
        // type only), so two-or-more siblings fanning out to the same helper are
        // exempt even though the class has no heritage clause.
        let src = "class Help {
            styleCommandDescription(str) { return this.styleDescriptionText(str); }
            styleOptionDescription(str) { return this.styleDescriptionText(str); }
            styleSubcommandDescription(str) { return this.styleDescriptionText(str); }
            styleArgumentDescription(str) { return this.styleDescriptionText(str); }
            styleDescriptionText(str) { return str; }
        }";
        assert!(run(src).is_empty(), "expected no diagnostics, got: {:?}", run(src));
    }

    #[test]
    fn flags_lone_wrapper_with_no_sibling_sharing_target() {
        // A single pass-through whose target no sibling shares is still a
        // deletable indirection — the fan-out exemption must not over-suppress a
        // genuine pointless wrapper. Here `wrap` forwards to `collaborate` while
        // the sibling `other` forwards to a DIFFERENT helper, so neither shares a
        // target and both must flag.
        let src = "class A {
            wrap(a, b) { return this.collaborate(a, b); }
            other(x) { return this.somethingElse(x); }
        }";
        assert_eq!(run(src).len(), 2, "expected two diagnostics, got: {:?}", run(src));
    }

    #[test]
    fn allows_narrowing_facade_over_internal_target() {
        // Regression for #6874: vite's `EnvironmentModuleGraph.ensureEntryFromUrl`
        // exposes two parameters and forwards to the `@internal`
        // `_ensureEntryFromUrl`, which takes a third optimisation argument. The
        // wrapper fixes a strictly narrower public contract — inlining it would
        // let callers pass the internal argument it hides.
        let src = "class EnvironmentModuleGraph {
            async ensureEntryFromUrl(rawUrl: string, setIsSelfAccepting = true): Promise<EnvironmentModuleNode> {
                return this._ensureEntryFromUrl(rawUrl, setIsSelfAccepting)
            }
            /** @internal */
            async _ensureEntryFromUrl(rawUrl: string, setIsSelfAccepting = true, resolved?: PartialResolvedId): Promise<EnvironmentModuleNode> {
                const mod = resolved ?? (await this.resolveId(rawUrl))
                mod.isSelfAccepting = setIsSelfAccepting
                return mod
            }
        }";
        assert!(run(src).is_empty(), "expected no diagnostics, got: {:?}", run(src));
    }

    #[test]
    fn allows_narrowing_facade_over_variadic_target() {
        // A target with a rest element accepts unboundedly many arguments, so a
        // wrapper exposing a fixed list still narrows the contract.
        let src = "class Logger { warn(message) { return this.write(message); } write(message, ...details) { return message; } }";
        assert!(run(src).is_empty(), "expected no diagnostics, got: {:?}", run(src));
    }

    #[test]
    fn flags_passthrough_when_declared_target_takes_the_same_arguments() {
        // The target is declared in the same class and accepts exactly the
        // arguments the wrapper exposes, so the wrapper narrows nothing and is
        // still a deletable indirection. Bounds the narrowing-facade exemption.
        let src = "class NestFactoryStatic {
            createNestInstance(instance) { return this.createProxy(instance); }
            createProxy(target) { return new Proxy(target, {}); }
        }";
        assert_eq!(run(src).len(), 1, "expected one diagnostic, got: {:?}", run(src));
    }

    #[test]
    fn allows_narrowing_facade_over_wider_overload_signature() {
        // A bodyless signature of `load` accepts an argument `read` does not
        // expose, so the target's callers can reach what the wrapper hides.
        let src = "class Loader {
            read(path) { return this.load(path); }
            load(path: string): string;
            load(path: string, encoding: string): string;
            load(path: string, encoding?: string): string { return encoding ?? path; }
        }";
        assert!(run(src).is_empty(), "expected no diagnostics, got: {:?}", run(src));
    }

    #[test]
    fn flags_passthrough_when_only_the_overload_implementation_is_wider() {
        // Callers of an overloaded target resolve against its bodyless
        // signatures; the bodied implementation covers their union and none can
        // reach it. `bar`'s only callable shape takes exactly what `foo`
        // exposes, so `foo` narrows nothing.
        let src = "class A {
            foo(a) { return this.bar(a); }
            bar(a: string): void;
            bar(a: string | number, b?: unknown): void {}
        }";
        assert_eq!(run(src).len(), 1, "expected one diagnostic, got: {:?}", run(src));
    }

    #[test]
    fn flags_passthrough_whose_wider_homonym_lives_in_another_class() {
        // The lookup is scoped to the enclosing class body: a wider `load`
        // declared by a different class is not what `this.load` resolves to, so
        // it narrows nothing.
        let src = "class Reader {
            read(path) { return this.load(path); }
            load(path) { return path; }
        }
        class Other {
            load(path, encoding) { return encoding ?? path; }
        }";
        assert_eq!(run(src).len(), 1, "expected one diagnostic, got: {:?}", run(src));
    }

    #[test]
    fn flags_static_passthrough_whose_wider_homonym_is_an_instance_method() {
        // `this` inside a static method reaches the static side only, so the
        // instance `read` is not the target and its extra parameter narrows
        // nothing. The static `read` accepts exactly what `lookup` exposes.
        let src = "class Cache {
            static lookup(key) { return this.read(key); }
            static read(key) { return key; }
            read(key, fallback) { return fallback; }
        }";
        assert_eq!(run(src).len(), 1, "expected one diagnostic, got: {:?}", run(src));
    }

    #[test]
    fn allows_wrapper_supplying_a_default_the_target_requires() {
        // Regression for #8130: minisearch's `vacuum(options: VacuumOptions = {})`
        // forwards to a `conditionalVacuum` whose parameter is required. The
        // default widens the contract — `flush()` type-checks, `doFlush()` does
        // not — so the wrapper is not interchangeable with its target.
        let defaulted = "class Store {
            flush (options: FlushOptions = {}): number { return this.doFlush(options) }
            doFlush (options: FlushOptions): number { return options.batch ?? 0 }
        }";
        assert!(run(defaulted).is_empty(), "expected no diagnostics, got: {:?}", run(defaulted));

        let optional = "class Store {
            flush (options?: FlushOptions): number { return this.doFlush(options) }
            doFlush (options: FlushOptions | undefined): number { return 0 }
        }";
        assert!(run(optional).is_empty(), "expected no diagnostics, got: {:?}", run(optional));
    }

    #[test]
    fn flags_wrapper_whose_default_mirrors_the_targets_default() {
        // The target tolerates exactly the same omission, so the wrapper's
        // default materialises nothing the target would not. Bounds the
        // widening-facade exemption.
        let src = "class Store {
            flush (options: FlushOptions = {}): number { return this.doFlush(options) }
            doFlush (options: FlushOptions = {}): number { return options.batch ?? 0 }
        }";
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
