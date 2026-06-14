//! ts-no-invalid-this OXC backend — flag `this` expressions outside
//! classes/object methods.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::jsdoc_helpers::scan_blocks;
use oxc_ast::CommentKind;
use oxc_ast::ast::{AssignmentTarget, Expression};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// True when the standalone `function` at `func_start` is preceded by a leading
/// `/** … */` JSDoc block that gives it an explicit type contract governing
/// `this` — either a `@type {…}` annotation (the function's whole signature,
/// possibly an aliased function type like `@type {Equals}`, or an inline
/// `@type {(this: T, …) => …}`) or a `@this {T}` tag. Such a function is
/// type-checked against a declared signature whose `this` binding is part of the
/// contract, so a `this` in its body is intentional, not a stray reference.
fn has_this_typed_jsdoc(
    source: &str,
    semantic: &oxc_semantic::Semantic,
    func_start: usize,
) -> bool {
    for comment in semantic.comments() {
        if comment.kind == CommentKind::Line {
            continue;
        }
        let comment_end = comment.span.end as usize;
        if comment_end > func_start {
            continue;
        }
        // Only the JSDoc block immediately preceding the function counts:
        // whitespace plus an optional `export` keyword may sit between them.
        let Some(between) = source.get(comment_end..func_start) else {
            continue;
        };
        let trimmed = between.trim();
        if !trimmed.is_empty() && trimmed != "export" && trimmed != "export default" {
            continue;
        }
        let comment_start = comment.span.start as usize;
        let Some(raw) = source.get(comment_start..comment_end) else {
            continue;
        };
        if !raw.starts_with("/**") {
            continue;
        }
        let Some(block) = scan_blocks(raw).into_iter().next() else {
            continue;
        };
        if block
            .tags()
            .iter()
            .any(|tag| tag.name == "type" || tag.name == "this")
        {
            return true;
        }
    }
    false
}

/// True when `expr` is a `*.prototype` member access (e.g. `Foo.prototype`),
/// or an identifier bound to such an access (e.g. `var proto = Foo.prototype`).
/// These are the receivers of the prototype-patching idiom, where a function
/// assigned to one of their members gains the instance as `this` at call time.
fn is_prototype_object(
    expr: &Expression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    match expr {
        Expression::StaticMemberExpression(member) => member.property.name == "prototype",
        Expression::Identifier(ident) => {
            let Some(ref_id) = ident.reference_id.get() else {
                return false;
            };
            let scoping = semantic.scoping();
            let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
                return false;
            };
            let decl = scoping.symbol_declaration(sym_id);
            let AstKind::VariableDeclarator(declarator) =
                semantic.nodes().kind(decl)
            else {
                return false;
            };
            matches!(
                &declarator.init,
                Some(Expression::StaticMemberExpression(member))
                    if member.property.name == "prototype"
            )
        }
        _ => false,
    }
}

/// True when `func_id` is a function expression assigned to a member of a
/// prototype object (`proto[m] = function() {}` / `Foo.prototype.m = function() {}`).
/// In that idiom `this` is bound to the instance at call time, so `this` inside
/// the function body is valid.
fn is_prototype_method_assignment(
    func_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let AstKind::AssignmentExpression(assign) = nodes.kind(nodes.parent_id(func_id)) else {
        return false;
    };
    let object = match &assign.left {
        AssignmentTarget::StaticMemberExpression(member) => &member.object,
        AssignmentTarget::ComputedMemberExpression(member) => &member.object,
        _ => return false,
    };
    is_prototype_object(object, semantic)
}

/// Mocha test/suite globals whose `function` callback is invoked with a
/// Test/Suite context bound to `this` (`this.timeout`, `this.retries`,
/// `this.skip`, `this.slow`).
const MOCHA_GLOBALS: &[&str] = &[
    "describe", "it", "before", "after", "beforeEach", "afterEach", "context",
    "specify",
];

/// True when `callee` is a Mocha global, either bare (`it(...)`) or a
/// `.only`/`.skip` variant (`describe.only(...)`, `it.skip(...)`).
fn callee_is_mocha_global(callee: &Expression) -> bool {
    match callee {
        Expression::Identifier(ident) => {
            MOCHA_GLOBALS.contains(&ident.name.as_str())
        }
        Expression::StaticMemberExpression(member)
            if matches!(member.property.name.as_str(), "only" | "skip") =>
        {
            matches!(
                &member.object,
                Expression::Identifier(ident)
                    if MOCHA_GLOBALS.contains(&ident.name.as_str())
            )
        }
        _ => false,
    }
}

/// True when `func_id` is a `function` expression passed directly as an argument
/// to a Mocha global (`describe`/`it`/`before`/... and `.only`/`.skip`
/// variants). Mocha binds a Test/Suite context to `this` in such callbacks, so
/// `this` inside the function body is valid. The function expression is the
/// parent or grandparent's child of the `CallExpression` depending on whether
/// the AST wraps arguments.
fn is_mocha_callback(
    func_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let parent_id = nodes.parent_id(func_id);
    let call = match nodes.kind(parent_id) {
        AstKind::CallExpression(call) => call,
        _ => {
            let gp_id = nodes.parent_id(parent_id);
            let AstKind::CallExpression(call) = nodes.kind(gp_id) else {
                return false;
            };
            call
        }
    };
    callee_is_mocha_global(&call.callee)
}

/// True when `expr` is a Cypress command chain rooted in the `cy` global
/// (`cy`, `cy.get(...)`, `cy.get(...).as(...).contains(...)`, …). The chain is a
/// left-spine of member accesses and calls that bottoms out at the `cy`
/// identifier.
fn is_cypress_chain(expr: &Expression) -> bool {
    let mut current = expr;
    loop {
        match current {
            Expression::Identifier(ident) => return ident.name == "cy",
            Expression::CallExpression(call) => current = &call.callee,
            Expression::StaticMemberExpression(member) => current = &member.object,
            Expression::ComputedMemberExpression(member) => current = &member.object,
            _ => return false,
        }
    }
}

/// True when `func_id` is a `function` expression passed as an argument to a
/// `.then(...)`/`.should(...)` member call on a Cypress chain
/// (`cy.get(...).then(function () { this.alias })`). Cypress binds the shared
/// test context to `this` in such callbacks — aliases registered via
/// `.as('name')` are read as `this.name` — so `this` inside the body is valid.
fn is_cypress_callback(
    func_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let parent_id = nodes.parent_id(func_id);
    let call = match nodes.kind(parent_id) {
        AstKind::CallExpression(call) => call,
        _ => {
            let gp_id = nodes.parent_id(parent_id);
            let AstKind::CallExpression(call) = nodes.kind(gp_id) else {
                return false;
            };
            call
        }
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    if !matches!(member.property.name.as_str(), "then" | "should") {
        return false;
    }
    is_cypress_chain(&member.object)
}

/// Chai plugin-registration methods. Each invokes its registered function with
/// `this` bound to the `chai.Assertion` instance, so `this` in the function body
/// is the documented Chai plugin API.
const CHAI_REGISTRATION_METHODS: &[&str] = &[
    "addMethod", "addProperty", "overwriteMethod", "overwriteProperty",
];

/// True when `expr` is a `chai.Assertion` receiver — either the bare `Assertion`
/// identifier or a member access ending in `.Assertion` (e.g. `chai.Assertion`).
/// This is the object on which Chai's plugin-registration methods are called.
fn is_chai_assertion_receiver(expr: &Expression) -> bool {
    match expr {
        Expression::Identifier(ident) => ident.name == "Assertion",
        Expression::StaticMemberExpression(member) => member.property.name == "Assertion",
        _ => false,
    }
}

/// True when `func_id` is a `function` expression passed as an argument to a Chai
/// plugin-registration call (`chai.Assertion.addMethod(name, function () {...})`,
/// `Assertion.overwriteProperty(...)`, …). Chai invokes the registered function
/// with `this` bound to the Assertion instance, so `this` inside the body is the
/// documented plugin API and is valid.
fn is_chai_registration_callback(
    func_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let parent_id = nodes.parent_id(func_id);
    let call = match nodes.kind(parent_id) {
        AstKind::CallExpression(call) => call,
        _ => {
            let gp_id = nodes.parent_id(parent_id);
            let AstKind::CallExpression(call) = nodes.kind(gp_id) else {
                return false;
            };
            call
        }
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    if !CHAI_REGISTRATION_METHODS.contains(&member.property.name.as_str()) {
        return false;
    }
    is_chai_assertion_receiver(&member.object)
}

/// True when `name` follows the constructor-function convention (starts with an
/// uppercase ASCII letter, e.g. `Suspense`, `Component`). Such functions are
/// conventionally invoked with `new`, so `this` is the new instance.
fn is_constructor_name(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

/// True when the reference at `ref_node_id` is used in a way that binds the
/// function's `this` at call time:
/// - `new F(...)` — constructor invocation,
/// - `F.call(this, ...)` / `F.apply(...)` / `F.bind(...)` — explicit binding,
/// - `x.member = F` — assigned as a method value (receives the receiver as `this`).
fn reference_binds_this(
    ref_node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    match nodes.kind(nodes.parent_id(ref_node_id)) {
        AstKind::NewExpression(_) => true,
        AstKind::StaticMemberExpression(member) => {
            matches!(member.property.name.as_str(), "call" | "apply" | "bind")
        }
        AstKind::AssignmentExpression(assign) => {
            matches!(
                assign.left,
                AssignmentTarget::StaticMemberExpression(_)
                    | AssignmentTarget::ComputedMemberExpression(_)
            ) && assign.right.span() == nodes.kind(ref_node_id).span()
        }
        _ => false,
    }
}

/// True when the standalone `function` at `func` is a constructor function whose
/// `this` is bound at call time — either by the PascalCase naming convention, or
/// because its name is referenced as `new`/`.call`/`.apply`/`.bind`/method-value
/// somewhere in the module.
fn is_constructor_function(
    func: &oxc_ast::ast::Function,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Some(id) = &func.id else {
        return false;
    };
    if is_constructor_name(&id.name) {
        return true;
    }
    let Some(symbol_id) = id.symbol_id.get() else {
        return false;
    };
    semantic
        .scoping()
        .get_resolved_references(symbol_id)
        .any(|reference| reference_binds_this(reference.node_id(), semantic))
}

fn is_valid_this_context(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    source: &str,
) -> bool {
    // Walk up from the ThisExpression. The first `this`-binding boundary
    // determines validity:
    // - ArrowFunction: transparent, keep going.
    // - Function inside a MethodDefinition (class method): valid.
    // - Function that is an object-literal method or property value: valid.
    // - Standalone Function: invalid — stop.
    // - Class: valid (property initializer, etc.).
    let mut entered_function: Option<oxc_span::Span> = None;
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::Class(_) => return true,
            AstKind::ArrowFunctionExpression(_) => continue,
            AstKind::Function(func) => {
                // Explicit TypeScript `this` parameter: a function declaring a
                // formal `this` parameter (`function f(this: T, …) {…}`) types
                // its `this` context as part of the signature, so `this` in the
                // body is the declared binding and is valid.
                if func.this_param.is_some() {
                    return true;
                }
                // Prototype-patching idiom: a function assigned to a member
                // of a `*.prototype` object is a method — `this` is the
                // instance at call time, so it's valid.
                if is_prototype_method_assignment(ancestor.id(), semantic) {
                    return true;
                }
                // Mocha callback: a `function` passed to `describe`/`it`/hooks
                // is invoked with a Test/Suite context bound to `this`
                // (`this.timeout()`, `this.retries()`), so `this` is valid.
                if is_mocha_callback(ancestor.id(), semantic) {
                    return true;
                }
                // Cypress callback: a `function` passed to `.then()`/`.should()`
                // on a `cy` chain is invoked with the shared test context bound
                // to `this` (`this.alias` from a prior `.as('alias')`), so
                // `this` is valid.
                if is_cypress_callback(ancestor.id(), semantic) {
                    return true;
                }
                // Chai plugin registration: a `function` passed to
                // `chai.Assertion.addMethod`/`addProperty`/`overwriteMethod`/
                // `overwriteProperty` is invoked with `this` bound to the
                // Assertion instance — the documented plugin API — so `this`
                // is valid.
                if is_chai_registration_callback(ancestor.id(), semantic) {
                    return true;
                }
                // Constructor function: a PascalCase `function`, or one
                // referenced via `new`/`.call(this)`/`.apply`/`.bind` or
                // assigned as a method value, gets the instance as `this`.
                if is_constructor_function(func, semantic) {
                    return true;
                }
                // JSDoc `@type {…}` / `@this {…}` annotation: the function has an
                // explicit declared type contract whose `this` binding is part
                // of the signature (e.g. `/** @type {(this: T, …) => …} */` or an
                // aliased function type), so `this` in the body is intentional.
                if has_this_typed_jsdoc(source, semantic, func.span.start as usize) {
                    return true;
                }
                // Mark that we've entered a function scope; need to
                // check if it's wrapped in a MethodDefinition.
                entered_function = Some(func.span);
            }
            AstKind::MethodDefinition(_) if entered_function.is_some() => {
                // The Function was a class method — `this` is valid.
                return true;
            }
            AstKind::PropertyDefinition(_) if entered_function.is_some() => {
                // Property initializer context — valid.
                return true;
            }
            AstKind::ObjectProperty(prop)
                if entered_function.is_some_and(|func_span| {
                    prop.method || prop.value.span() == func_span
                }) =>
            {
                // Object-literal method or function-valued property —
                // `this` is bound to the object when called as `obj.key()`.
                // Both the shorthand form (`{ foo() { this } }`,
                // `prop.method == true`) and the non-shorthand form
                // (`{ foo: function () { this } }`, where the entered function
                // is exactly the property value) are valid. A function nested
                // deeper inside the value (`{ foo: arr.map(function () { this }) }`)
                // has a different value span and stays flagged.
                return true;
            }
            _ => {
                // If we already hit a standalone function (not a method),
                // any other ancestor means `this` is unbound.
                if entered_function.is_some() {
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
            let AstKind::ThisExpression(this_expr) = node.kind() else {
                continue;
            };

            if is_valid_this_context(node, semantic, ctx.source) {
                continue;
            }

            let (line, column) =
                byte_offset_to_line_col(ctx.source, this_expr.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`this` used outside a class or valid context — likely a bug."
                    .into(),
                severity: Severity::Warning,
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

    #[test]
    fn flags_this_at_top_level() {
        let diags = run_on("console.log(this);");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_this_in_class_method() {
        assert!(run_on("class Foo { bar() { return this.x; } }").is_empty());
    }

    #[test]
    fn flags_this_in_standalone_function() {
        let diags = run_on("function foo() { return this; }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_this_in_object_literal_async_iterator_method() {
        let src = "const asyncIterable = {\n  next() { return iter.next(); },\n  [Symbol.asyncIterator]() {\n    return this;\n  },\n};";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_function_valued_property() {
        // A `function` expression that is the value of an object property is a
        // method — `this` is bound to the object when called as `obj.foo()`.
        assert!(run_on("const obj = { foo: function() { return this; } };").is_empty());
    }

    #[test]
    fn allows_this_in_named_function_expression_property() {
        // Regression for #1642: fastify defines public-API methods as named
        // function expressions assigned to object properties (`function _delete`)
        // for clearer stack traces; `this` is the instance at call time.
        let src = "const fastify = {\n  delete: function _delete (url, options, handler) {\n    return router.prepareRoute.call(this, { method: 'DELETE', url, options, handler })\n  },\n  hasPlugin: function (name) {\n    return this[kRegisteredPlugins].includes(name)\n  },\n};";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_this_in_function_nested_in_property_value() {
        // Negative: a `function` nested inside the property value (not the value
        // itself) gets no object binding — `this` is unbound and must fire.
        let diags = run_on("const obj = { foo: arr.map(function () { return this.x; }) };");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_this_in_prototype_patch_via_alias() {
        // Regression for #2031: `proto[method] = function() { this }` where
        // `proto` is an alias of `SomeClass.prototype`.
        let src = "var proto = SvelteDate.prototype;\nproto[method] = function (...args) {\n  return this.x.apply(this, args);\n};";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_prototype_patch_static() {
        let src = "Foo.prototype.m = function () { return this.x; };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_prototype_patch_computed() {
        let src = "Foo.prototype[k] = function () { return this.x; };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_this_in_non_prototype_member_assignment() {
        // A function assigned to a plain (non-prototype) object member is still
        // a standalone function — `this` is unbound and must fire.
        let diags = run_on("obj.m = function () { return this.x; };");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_this_in_mocha_it_callback() {
        // Regression for #2023: Mocha binds a Test context to `this` inside an
        // `it(name, function() {...})` callback.
        let src = "it('/POST (concurrent)', function () {\n  this.retries(10);\n  return request(server).post('/concurrent').expect(200);\n});";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_mocha_describe_skip_callback() {
        // Regression for #2023: `describe.skip(name, function() {...})` with
        // `this.timeout()` / `this.retries()`.
        let src = "describe.skip('Kafka transport', function () {\n  this.timeout(50000);\n  this.retries(10);\n});";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_mocha_hook_callback() {
        let src = "before('Start app', function () {\n  this.timeout(10000);\n});";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_this_in_non_mocha_callback() {
        // A `function` passed to a non-Mocha call gets no context — `this` is
        // still unbound and must fire.
        let diags = run_on("arr.forEach(function () { return this.x; });");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_this_in_pascal_case_constructor_function() {
        // Regression for #1916: a PascalCase `function` is a constructor function
        // by convention — called with `new`, `this` is the new instance.
        let src = "export function Suspense() {\n  this._pendingSuspensionCount = 0;\n  this._suspenders = null;\n  this._detachOnNextRender = null;\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_constructor_function_calling_super_via_call() {
        // Regression for #1916: prototype-based inheritance — the PascalCase
        // constructor uses `.call(this, ...)` and assigns `this.*`.
        let src = "export function Component(props, context) {\n  CevicheComponent.call(this, props, context);\n  const render = this.render;\n  this.render = function () {};\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_function_assigned_as_method() {
        // Regression for #1916: a lowercase `function` referenced as a method
        // value (`this.x = fn`) receives the instance as `this` at call time.
        let src = "function shouldUpdate(nextProps) {\n  const ref = this.props.ref;\n  return shallowDiffers(this.props, nextProps);\n}\nfunction Memoed(props) {\n  this.shouldComponentUpdate = shouldUpdate;\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_function_invoked_with_call_this() {
        // Regression for #1916: a lowercase `function` invoked elsewhere via
        // `.call(this)` is explicitly bound, so `this` in its body is valid.
        let src = "function init() {\n  return this.x;\n}\nfunction Widget() {\n  init.call(this);\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_this_in_lowercase_free_function() {
        // Negative: an ordinary lowercase free function never used as a
        // constructor or bound method still has a stray `this`.
        let diags = run_on("function foo() {\n  return this.bar;\n}\nfoo();");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_this_in_cypress_then_callback() {
        // Regression for #1842: Cypress binds the shared test context to `this`
        // inside a `function` callback passed to `.then()` in a `cy` chain.
        // Aliases registered via `.as('name')` are read as `this.name`.
        let src = "cy.get('div')\n  .contains('animate')\n  .as('spring')\n  .then(function () {\n    const bounds = this.miniDefault[0].getBoundingClientRect();\n  });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_cypress_should_callback() {
        // Regression for #1842: `.should(function() {...})` is the other Cypress
        // chain method that binds the test context to `this`.
        let src = "cy.get('@spring').should(function () {\n  expect(this.value).to.equal(1);\n});";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_function_with_jsdoc_type_alias() {
        // Regression for #1775: a `.js` function whose JSDoc `@type` assigns an
        // aliased function type that declares `this` (`type Equals = (this:
        // Value, …) => boolean`) is type-checked against that contract.
        let src = "/** @type {Equals} */\nexport function equals(value) {\n  return value === this.v;\n}\n\n/** @type {Equals} */\nexport function safe_equals(value) {\n  return !safe_not_equal(value, this.v);\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_function_with_inline_jsdoc_this_type() {
        // Regression for #1775: an inline `@type {(this: T, …) => …}` declares
        // the `this` binding directly in the function signature.
        let src = "/** @type {(this: Value, value: unknown) => boolean} */\nexport function equals(value) {\n  return value === this.v;\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_function_with_jsdoc_this_tag() {
        // Regression for #1775: the `@this {T}` tag names the `this` context.
        let src = "/** @this {Value} */\nexport function equals(value) {\n  return value === this.v;\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_this_in_function_with_unrelated_jsdoc() {
        // Negative: a JSDoc block without `@type`/`@this` does not declare a
        // `this` context, so a stray `this` must still fire.
        let diags = run_on("/** Does a thing. @param value - input */\nexport function equals(value) {\n  return value === this.v;\n}");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_this_in_chai_add_method_callback() {
        // Regression for #1549: Chai binds the Assertion instance to `this`
        // inside a `function` passed to `chai.Assertion.addMethod(...)`.
        let src = "chai.Assertion.addMethod('x', function () {\n  return this._obj;\n});";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_chai_overwrite_property_callback() {
        // Regression for #1549: the other Chai plugin-registration methods
        // (`addProperty`/`overwriteMethod`/`overwriteProperty`) and a bare
        // `Assertion` receiver bind `this` the same way.
        let src = "Assertion.overwriteProperty('ok', function () {\n  return this.assert(this._obj);\n});";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_this_in_free_function_at_module_scope() {
        // Negative-space guard for #1549: a free `function` at module scope is
        // not a Chai registration callback — `this` is unbound and must fire.
        let diags = run_on("function f() {\n  return this.x;\n}");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_this_in_array_foreach_callback() {
        // Negative-space guard for #1549: a bare `function` passed to `forEach`
        // is a genuine invalid-this — the Chai allowance must not leak to it.
        let diags = run_on("[1].forEach(function () {\n  return this.x;\n});");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_this_in_non_assertion_add_method_callback() {
        // Negative-space guard for #1549: `addMethod` on a non-Assertion
        // receiver is not the Chai API — `this` stays unbound and must fire.
        let diags = run_on("registry.addMethod('x', function () {\n  return this._obj;\n});");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_this_in_promise_then_callback() {
        // Negative: a `function` callback passed to a plain Promise `.then()`
        // (no `cy` chain root) gets no bound `this` — must still fire.
        let diags = run_on("fetch('/x').then(function () {\n  return this.value;\n});");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_this_in_function_with_explicit_this_param_returning_this() {
        // Regression for #1342: a fluent function declaring an explicit
        // TypeScript `this` parameter (`function use(this: unknown, …)`) types
        // its `this` context, so returning `this` from the body is valid.
        let src = "function use(this: unknown, url: string | null) {\n  return this;\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_typed_constructor_function_with_this_param() {
        // Regression for #1342: an old-style constructor function with an
        // explicit `this` parameter (`function Holder(this: HolderInstance)`)
        // declares the type of `this`, so assigning `this.*` is valid.
        let src = "function Holder(this: HolderInstance) {\n  this.req = null;\n  this.res = null;\n  this.url = null;\n  this.context = null;\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_this_in_function_without_explicit_this_param() {
        // Negative-space guard for #1342: a plain standalone function with no
        // explicit `this` parameter (and outside any class/object method) still
        // has an unbound `this` and must fire.
        let diags = run_on("function f(url: string) {\n  return this.x;\n}");
        assert_eq!(diags.len(), 1);
    }
}
