//! no-this-in-static OXC backend — flag `this`/`super` references whose binding
//! is a `static` class context (static method, getter, setter, property
//! initializer, or static initialization block).
//!
//! In a static context `this` is the class constructor itself and `super` is the
//! parent class. A bare `this` used as a value (`foo(this)`, `return this`,
//! `this === x`) is almost always a mistake for code meant to operate on an
//! instance and is flagged. Constructing, member-accessing, or instance-checking
//! through `this` (`new this()`, `this.member`, `this[k]`, `this.#field`,
//! `x instanceof this`) is exempt: there `this` resolves to the actual —
//! possibly subclass — class, so these reach inherited/overridden static members
//! or perform a subclass-aware identity check polymorphically. Replacing them
//! with a hardcoded class name would break subclassing.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, MethodDefinitionKind};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// True when `this_node` is the `this` used directly as the callee of a `new`
/// expression (`new this(...)`). There `this` names the class constructor on
/// purpose — it is the conventional "construct an instance of the current
/// class" idiom — so it is exempt. The `this` *argument* of such a call
/// (`new this(this)`) is a distinct node and is not exempted.
fn is_new_this_callee(
    this_node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let AstKind::NewExpression(new_expr) = nodes.kind(nodes.parent_id(this_node.id())) else {
        return false;
    };
    new_expr.callee.span() == this_node.kind().span()
}

/// True when `this_node` is the *object* of a member access on `this`
/// (`this.member`, `this[member]`, or `this.#member`). In a static context
/// `this` resolves to the class the method was invoked on — possibly a subclass
/// — so member access reaches inherited / overridden static members through the
/// class hierarchy (`this.MAPPINGS`, `this.staticHelper()`, `this.#field`).
/// Hardcoding the declaring class name instead would break that polymorphism, so
/// this access is exempt. The member *property* and any computed-key `this` are
/// distinct nodes and are not exempted.
fn is_this_member_object(
    this_node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let this_span = this_node.kind().span();
    match nodes.kind(nodes.parent_id(this_node.id())) {
        AstKind::StaticMemberExpression(member) => member.object.span() == this_span,
        AstKind::ComputedMemberExpression(member) => member.object.span() == this_span,
        AstKind::PrivateFieldExpression(member) => member.object.span() == this_span,
        _ => false,
    }
}

/// True when `this_node` is the right operand of an `instanceof` expression
/// (`x instanceof this`). In a static context `this` resolves to the class the
/// method was invoked on — possibly a subclass — so `instanceof this` is a
/// subclass-aware identity check that follows the class hierarchy. Hardcoding the
/// declaring class name instead would break that polymorphism, so this use is
/// exempt. The *left* operand `this` (`this instanceof X`) is a bare value and is
/// not exempted.
fn is_this_instanceof_rhs(
    this_node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let AstKind::BinaryExpression(binary) = nodes.kind(nodes.parent_id(this_node.id())) else {
        return false;
    };
    binary.operator == BinaryOperator::Instanceof
        && binary.right.span() == this_node.kind().span()
}

/// Determines whether the `this`/`super` at `node` is bound to a `static` class
/// context.
///
/// Walks ancestors from the reference upward to the first `this`-binding
/// boundary (the first non-arrow control-flow root), mirroring JavaScript's
/// `this` semantics:
/// - `ArrowFunctionExpression` is transparent — arrows inherit the enclosing
///   `this`, so a `this` inside an arrow declared in a static body is still the
///   static `this`. Keep walking.
/// - A non-arrow `Function` is a hard boundary: it rebinds `this`. The reference
///   is the static `this` only if that function is the value of a `static`
///   method / getter / setter. A function nested deeper (a regular `function`
///   declared inside the static body, an instance method, an object method)
///   rebinds `this` to something else, so the reference is not the static `this`.
/// - A `StaticBlock` (`static { … }`) is implicitly static and is itself the
///   binding root.
/// - A `static` `PropertyDefinition` initializer binds `this` to the class when
///   the reference sits directly in the initializer (no intervening function).
/// - Any other terminal boundary (a class, a non-static member, the program
///   root) means the reference is not a static `this`.
fn is_in_static_context(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    // Tracks the most recently entered non-arrow `Function`: when its parent is a
    // class member we can decide staticness from that member's modifier.
    let mut entered_function = false;
    for ancestor in nodes.ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::ArrowFunctionExpression(_) => continue,
            AstKind::Function(_) => {
                // First non-arrow function boundary. If it is the value of a
                // class member, the parent node is that member; staticness is
                // decided there. Otherwise the function rebinds `this` to a
                // non-static context.
                entered_function = true;
            }
            AstKind::MethodDefinition(method) if entered_function => {
                return method.r#static
                    && matches!(
                        method.kind,
                        MethodDefinitionKind::Method
                            | MethodDefinitionKind::Get
                            | MethodDefinitionKind::Set
                    );
            }
            AstKind::PropertyDefinition(prop) => {
                // A static property initializer (`static x = this.y`) binds `this`
                // to the class only when the reference is directly in the
                // initializer. If we first entered a function, that function
                // already rebound `this` (e.g. `static x = function () { this }`)
                // and the member is irrelevant.
                if entered_function {
                    return false;
                }
                return prop.r#static;
            }
            AstKind::StaticBlock(_) if !entered_function => return true,
            AstKind::Program(_) => return false,
            _ => {
                // Any other boundary reached after entering a function means the
                // function was not a class member (a nested standalone function),
                // so `this` is not the static `this`.
                if entered_function {
                    return false;
                }
            }
        }
    }
    false
}

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
            let (keyword, span) = match node.kind() {
                AstKind::ThisExpression(this_expr) => {
                    if is_new_this_callee(node, semantic)
                        || is_this_member_object(node, semantic)
                        || is_this_instanceof_rhs(node, semantic)
                    {
                        continue;
                    }
                    ("this", this_expr.span)
                }
                AstKind::Super(super_expr) => ("super", super_expr.span),
                _ => continue,
            };

            if !is_in_static_context(node, semantic) {
                continue;
            }

            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "`{keyword}` in a static context refers to the class itself, not an instance — use the {} name instead.",
                    if keyword == "super" { "parent class" } else { "class" }
                ),
                severity: Severity::Error,
                span: None,
            });
        }

        diagnostics
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    // --- Valid cases (mirrors Biome's valid.js) ---

    #[test]
    fn allows_this_in_free_function() {
        assert!(run_on("function foo() { this }").is_empty());
    }

    #[test]
    fn allows_this_in_arrow_at_top_level() {
        assert!(run_on("() => { this }").is_empty());
    }

    #[test]
    fn allows_this_in_constructor() {
        assert!(run_on("class A { constructor() { this } }").is_empty());
    }

    #[test]
    fn allows_this_in_instance_method() {
        assert!(run_on("class A { foo() { this } }").is_empty());
    }

    #[test]
    fn allows_this_in_nested_function_inside_static_method() {
        // A regular `function` declared inside a static method rebinds `this`;
        // the inner `this` is NOT the static `this`.
        assert!(run_on("class A { static foo() { function foo() { this } } }").is_empty());
    }

    #[test]
    fn allows_new_this_in_static_factory() {
        // `new this(name)` — `this` is the `new` callee, the documented factory
        // idiom, so it is exempt.
        let src = "class Base {\n    static create(name) {\n        return new this(name);\n    }\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_new_this_in_static_field_initializer() {
        assert!(run_on("class Base { static field = new this(); }").is_empty());
    }

    #[test]
    fn allows_new_this_in_static_block() {
        assert!(run_on("class Base { static { new this(); } }").is_empty());
    }

    // --- Invalid cases (mirrors Biome's invalid.js) ---

    #[test]
    fn flags_super_in_static_block() {
        // `static { this.CONSTANT += super.foo(); }` — `this.CONSTANT` is an
        // inherited-static access (exempt); `super.foo()` is flagged.
        let diags = run_on("class B extends A { static { this.CONSTANT += super.foo(); } }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_this_member_in_static_property_initializer() {
        // `static CONSTANT = this.OTHER;` — reads an inherited static member.
        assert!(run_on("class B extends A { static CONSTANT = this.OTHER; }").is_empty());
    }

    #[test]
    fn flags_super_in_static_property_initializer() {
        let diags = run_on("class B extends A { static OTHER = super.ANOTHER; }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_bare_this_and_super_in_static_getter() {
        // Bare `this` value + `super.x` member access — both flagged.
        let src = "class B extends A {\n    static get property() {\n        this;\n        return super.x;\n    }\n}";
        let diags = run_on(src);
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn flags_bare_this_and_super_in_arrow_inside_static_setter() {
        // Arrows inherit the static `this`/`super`; bare `this` + `super.x` fire.
        let src = "class B extends A {\n    static set property(x) {\n        () => this;\n        () => super.x = x;\n    }\n}";
        let diags = run_on(src);
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn flags_super_in_static_method() {
        // `this.CONSTANT` is an inherited-static access (exempt); `super.ANOTHER`
        // is flagged.
        let src =
            "class B extends A { static method() { return this.CONSTANT + super.ANOTHER; } }";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_super_in_static_method_of_named_class_expression() {
        // `this.X` exempt (inherited static), `super.Y` flagged.
        let src =
            "const D = class D extends f() { static method() { return this.X + super.Y; } }";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_super_in_static_method_of_anonymous_class_expression() {
        // `this.X` exempt (inherited static), `super.Y` flagged.
        let src =
            "const E = class extends f() { static method() { return this.X + super.Y; } }";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_this_argument_but_not_callee_in_new_this() {
        // `new this(this)` — callee `this` exempt, argument `this` flagged.
        let diags = run_on("class FactoryCases { static method() { new this(this); } }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_this_argument_in_new_other() {
        // `new Foo(this)` — `this` argument flagged.
        let diags = run_on("class FactoryCases { static method() { new Foo(this); } }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_this_member_in_new_member_expression() {
        // `new this.Factory()` — `this` is the object of a member access, the
        // inherited-static factory idiom, so it is exempt.
        assert!(run_on("class FactoryCases { static method() { new this.Factory(); } }").is_empty());
    }

    #[test]
    fn flags_only_bare_this_arguments_in_factory_cases() {
        // `new this(this)` callee exempt, bare-`this` argument flagged;
        // `new Foo(this)` bare-`this` argument flagged; `new this.Factory()`
        // member-object `this` exempt — 2 diagnostics total.
        let src = "class FactoryCases {\n    static method() {\n        new this(this);\n        new Foo(this);\n        new this.Factory();\n    }\n}";
        let diags = run_on(src);
        assert_eq!(diags.len(), 2);
    }

    // --- Boundary-precision guards ---

    #[test]
    fn flags_bare_this_in_nested_block_inside_static_method() {
        // Nested blocks/loops/conditionals inherit the static `this` (only
        // non-arrow functions reset it); a bare `this` value there is flagged.
        let src = "class A { static foo() { if (x) { for (;;) { foo(this); } } } }";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_this_in_method_nested_in_static_method() {
        // An object method declared inside a static body rebinds `this`.
        let src = "class A { static foo() { const o = { m() { return this.x; } }; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_bare_this_in_arrow_nested_in_static_property() {
        // A static property initialized with an arrow keeps the static `this`; a
        // bare `this` value there is flagged.
        let diags = run_on("class A { static x = () => foo(this); }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_this_in_function_value_of_static_property() {
        // A `function` value of a static property rebinds `this`.
        assert!(run_on("class A { static x = function () { return this.y; }; }").is_empty());
    }

    #[test]
    fn allows_this_in_non_static_property_initializer() {
        // Instance property initializer — `this` is the instance, valid.
        assert!(run_on("class A { x = this.y; }").is_empty());
    }

    // --- Private static member access via `this` ---

    #[test]
    fn allows_this_private_field_access_in_static_method() {
        // `this.#field` in a static method keeps polymorphism — exempt.
        let src = "class A { static #count = 0; static m() { return this.#count; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_private_method_call_in_static_method() {
        // `this.#method()` — the receiver `this` of a private call is exempt.
        let src = "class A { static #priv() {} static m() { return this.#priv(); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_private_access_in_static_block() {
        let src = "class A { static #x = 0; static { this.#x; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_public_member_access_in_static_method() {
        // `this.publicStatic` reaches an inherited/overridable static member
        // through the (possibly subclass) class — exempt.
        assert!(run_on("class A { static m() { return this.publicStatic; } }").is_empty());
    }

    #[test]
    fn allows_this_computed_member_access_in_static_method() {
        // `this[key]` is computed inherited-static access — exempt.
        assert!(run_on("class A { static m(key) { return this[key]; } }").is_empty());
    }

    #[test]
    fn flags_this_as_computed_key_in_static_method() {
        // `obj[this]` — `this` is the computed *key*, not the member object, so
        // it is a bare `this` value and stays flagged.
        let diags = run_on("class A { static m() { return obj[this]; } }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_bare_this_in_static_method() {
        let diags = run_on("class A { static m() { return this; } }");
        assert_eq!(diags.len(), 1);
    }

    // --- Regression: issue #5409 (class-hierarchy patterns) ---

    #[test]
    fn allows_polymorphic_factory_new_this() {
        // `return new this()` constructs the subclass the static method was
        // invoked on — the canonical polymorphic-factory idiom.
        assert!(run_on("class P { static create() { return new this(); } }").is_empty());
    }

    #[test]
    fn allows_inherited_static_property_access() {
        // `this.DEFAULT` resolves an overridable inherited static member.
        let src = "class M { static from() { if (!this.MAPPINGS) return; return this.DEFAULT; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_inherited_static_method_call() {
        // `this.staticHelper()` dispatches to the subclass's static override.
        assert!(run_on("class M { static run() { return this.staticHelper(); } }").is_empty());
    }

    // --- Regression: issue #5448 (`instanceof this` subclass-aware check) ---

    #[test]
    fn allows_instanceof_this_in_static_factory() {
        // `definitions instanceof this` — subclass-aware identity check in a
        // static factory on a subclass of a built-in (`extends Array`); the
        // `super.from(...)` call on the same line is still flagged.
        let src = "class OptionDefinitions extends Array {\n    static from(definitions) {\n        if (definitions instanceof this) return definitions;\n        return super.from(definitions);\n    }\n}";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_instanceof_this_rhs_in_static_method() {
        // `x instanceof this` — `this` is the right operand, the polymorphic
        // type-check idiom, so it is exempt.
        assert!(run_on("class A { static m(x) { return x instanceof this; } }").is_empty());
    }

    #[test]
    fn flags_this_as_instanceof_lhs_in_static_method() {
        // `this instanceof X` — `this` is the left operand (a bare value), not
        // the class being checked against, so it stays flagged.
        let diags = run_on("class A { static m() { return this instanceof Foo; } }");
        assert_eq!(diags.len(), 1);
    }
}
