//! ts-no-empty-function OxcCheck backend.
//!
//! Flag functions/methods with empty bodies. A body that contains a comment is
//! treated as non-empty (the comment is the "intentionally empty" signal).
//! Dependency-injection constructors — whose parameters carry an accessibility
//! modifier, `readonly`, or a decorator — are exempt: the parameters are the work.
//! Empty method stubs in a type-constrained object literal (`const x: T = { m:
//! () => {} }`, `{ … } satisfies T`, `{ … } as T`) are exempt: the constraining
//! interface makes them mandatory no-op implementations (Null Object pattern).
//! An empty function as the right operand of a `??` / `||` fallback
//! (`existing ?? (() => {})`) is exempt: the no-op body is the intended behavior
//! when the left operand is nullish/falsy.
//! An empty function as a JSX attribute value (`onClick={() => {}}`) is exempt in
//! any file: it is a deliberate no-op satisfying a required event-handler prop.
//! Other placeholder callback positions — call/new arguments — are exempt only in
//! test files.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Argument, Expression, FunctionBody, LogicalOperator, ObjectPropertyKind, PropertyKey,
};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.") || s.contains(".spec.") || s.contains("__tests__") || s.contains("_test.")
}

/// Returns true when the empty function — after transparently unwrapping a single
/// `ParenthesizedExpression` — is the value of a JSX *attribute*
/// (`onClick={() => {}}`): its enclosing `JSXExpressionContainer` is a
/// `JSXAttribute` value, not a JSX child expression. A no-op handler satisfying a
/// required event-handler prop is a deliberate interface placeholder, so it is
/// exempt in production and test code alike. A bare function as a JSX *child*
/// (`<div>{() => {}}</div>`) is not an attribute value and is not exempted here.
fn is_jsx_attribute_callback_position(
    nodes: &oxc_semantic::AstNodes,
    node_id: oxc_semantic::NodeId,
) -> bool {
    // Unwrap at most one ParenthesizedExpression wrapper.
    let mut outer_id = node_id;
    let parent_id = nodes.parent_id(outer_id);
    if parent_id != outer_id
        && matches!(nodes.kind(parent_id), AstKind::ParenthesizedExpression(_))
    {
        outer_id = parent_id;
    }
    let container_id = nodes.parent_id(outer_id);
    if container_id == outer_id
        || !matches!(nodes.kind(container_id), AstKind::JSXExpressionContainer(_))
    {
        return false;
    }
    let attr_id = nodes.parent_id(container_id);
    attr_id != container_id && matches!(nodes.kind(attr_id), AstKind::JSXAttribute(_))
}

/// Returns true when the function expression sits in a placeholder callback
/// position: the value of a JSX attribute (see
/// `is_jsx_attribute_callback_position`), or an argument to a call/new expression
/// (including parenthesized).
fn is_placeholder_callback_position(
    nodes: &oxc_semantic::AstNodes,
    node_id: oxc_semantic::NodeId,
) -> bool {
    if is_jsx_attribute_callback_position(nodes, node_id) {
        return true;
    }
    let parent_id = nodes.parent_id(node_id);
    if parent_id == node_id {
        return false;
    }
    match nodes.kind(parent_id) {
        AstKind::CallExpression(call) => {
            let node_span = nodes.kind(node_id).span();
            call.arguments.iter().any(|arg| arg.span() == node_span)
        }
        AstKind::NewExpression(new_expr) => {
            let node_span = nodes.kind(node_id).span();
            new_expr.arguments.iter().any(|arg| arg.span() == node_span)
        }
        AstKind::ParenthesizedExpression(_) => {
            let grandparent_id = nodes.parent_id(parent_id);
            if grandparent_id == parent_id {
                return false;
            }
            matches!(
                nodes.kind(grandparent_id),
                AstKind::CallExpression(_) | AstKind::NewExpression(_)
            )
        }
        _ => false,
    }
}

/// Returns true when the empty function is the callable target of `new
/// Proxy(target, handler)`. `Proxy`'s first argument is required by the language
/// to be callable when the proxy intercepts calls; its body is intentionally
/// empty because all call behavior is delegated to the handler's `apply` /
/// `construct` trap. The body is structurally mandated, not a forgotten no-op.
///
/// Gated on: the function is the FIRST argument of a `NewExpression` whose callee
/// is the bare `Proxy` global-constructor identifier (not a member expression
/// such as `foo.Proxy`). When the handler (second argument) is an object literal,
/// it must define an `apply` or `construct` trap — the reason the target body is
/// empty. When the handler is absent or not an inspectable object literal (a
/// variable reference, say), the first-argument-of-`Proxy` shape alone suffices,
/// since the target is required to be callable regardless.
fn is_callable_proxy_target(
    nodes: &oxc_semantic::AstNodes,
    node_id: oxc_semantic::NodeId,
) -> bool {
    let parent_id = nodes.parent_id(node_id);
    if parent_id == node_id {
        return false;
    }
    let AstKind::NewExpression(new_expr) = nodes.kind(parent_id) else {
        return false;
    };
    if !matches!(&new_expr.callee, Expression::Identifier(id) if id.name.as_str() == "Proxy") {
        return false;
    }
    // The function must be the FIRST argument (the proxy target), not any argument.
    let node_span = nodes.kind(node_id).span();
    let Some(first_arg) = new_expr.arguments.first() else {
        return false;
    };
    if first_arg.span() != node_span {
        return false;
    }
    // When the handler is an object literal, require an `apply` / `construct`
    // trap (the reason the target body is empty). When it is absent or a
    // non-literal expression whose traps cannot be inspected, accept the
    // first-argument-of-`Proxy` shape on its own.
    match new_expr.arguments.get(1) {
        Some(Argument::ObjectExpression(handler)) => handler.properties.iter().any(|prop| {
            let ObjectPropertyKind::ObjectProperty(p) = prop else {
                return false;
            };
            let key = match &p.key {
                PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                PropertyKey::StringLiteral(s) => s.value.as_str(),
                _ => return false,
            };
            key == "apply" || key == "construct"
        }),
        _ => true,
    }
}

/// Returns true when the type pins the object to an interface shape. `any`,
/// `unknown`, and the `as const` const-assertion do not — they make no method
/// mandatory — so they are not interface constraints.
fn ts_type_constrains(ty: &oxc_ast::ast::TSType) -> bool {
    use oxc_ast::ast::{TSType, TSTypeName};
    match ty {
        TSType::TSAnyKeyword(_) | TSType::TSUnknownKeyword(_) => false,
        TSType::TSTypeReference(r) => !matches!(
            &r.type_name,
            TSTypeName::IdentifierReference(id) if id.name.as_str() == "const"
        ),
        _ => true,
    }
}

/// Returns true when the function is the value of an object-literal property
/// whose enclosing object literal is type-constrained — either the initializer
/// of a typed `const x: T = { … }`, or wrapped in a `satisfies T` / `as T`
/// assertion. The empty bodies are then mandatory no-op stubs of the
/// constraining interface's methods (Null Object pattern), not dead code.
fn is_typed_object_literal_method_stub(
    nodes: &oxc_semantic::AstNodes,
    node_id: oxc_semantic::NodeId,
) -> bool {
    let prop_id = nodes.parent_id(node_id);
    if prop_id == node_id {
        return false;
    }
    let AstKind::ObjectProperty(prop) = nodes.kind(prop_id) else {
        return false;
    };
    // The function must be the property VALUE, not a computed key.
    if prop.value.span() != nodes.kind(node_id).span() {
        return false;
    }
    let obj_id = nodes.parent_id(prop_id);
    if obj_id == prop_id || !matches!(nodes.kind(obj_id), AstKind::ObjectExpression(_)) {
        return false;
    }
    // Walk up from the object literal, peeling assertion / paren wrappers, to
    // find a type constraint on the literal itself.
    let mut current = obj_id;
    loop {
        let parent = nodes.parent_id(current);
        if parent == current {
            return false;
        }
        match nodes.kind(parent) {
            AstKind::TSSatisfiesExpression(e) => return ts_type_constrains(&e.type_annotation),
            AstKind::TSAsExpression(e) => return ts_type_constrains(&e.type_annotation),
            AstKind::ParenthesizedExpression(_) => current = parent,
            AstKind::VariableDeclarator(decl) => {
                return decl
                    .type_annotation
                    .as_ref()
                    .is_some_and(|ann| ts_type_constrains(&ann.type_annotation));
            }
            _ => return false,
        }
    }
}

/// Returns true when the empty function — after transparently unwrapping a
/// single `ParenthesizedExpression` — is the RIGHT operand of a `??` or `||`
/// logical expression (`existing ?? (() => {})`, `existing || function () {}`).
/// A no-op fallback is intentional: the function does nothing when the left
/// operand is null/undefined (`??`) or falsy (`||`). Emptiness is the semantics,
/// not an oversight. Only `||` / `??` qualify (not `&&`), and only the right
/// operand (a left-operand empty function is not a fallback).
fn is_logical_fallback_position(
    nodes: &oxc_semantic::AstNodes,
    node_id: oxc_semantic::NodeId,
) -> bool {
    // Unwrap at most one ParenthesizedExpression wrapper, mirroring how
    // `is_placeholder_callback_position` looks through parens.
    let mut outer_id = node_id;
    let parent_id = nodes.parent_id(outer_id);
    if parent_id != outer_id && matches!(nodes.kind(parent_id), AstKind::ParenthesizedExpression(_))
    {
        outer_id = parent_id;
    }

    let logical_id = nodes.parent_id(outer_id);
    if logical_id == outer_id {
        return false;
    }
    let AstKind::LogicalExpression(expr) = nodes.kind(logical_id) else {
        return false;
    };
    if !matches!(expr.operator, LogicalOperator::Or | LogicalOperator::Coalesce) {
        return false;
    }
    expr.right.span() == nodes.kind(outer_id).span()
}

/// Returns true when the function body is empty: no statements, no directives,
/// and no comment between the braces. A comment is the explicit "intentionally
/// empty" signal, so a comment-bearing body is treated as non-empty.
fn is_empty_body(body: &FunctionBody, semantic: &oxc_semantic::Semantic) -> bool {
    if !body.statements.is_empty() || !body.directives.is_empty() {
        return false;
    }
    let start = body.span.start;
    let end = body.span.end;
    !semantic
        .comments()
        .iter()
        .any(|comment| comment.span.start >= start && comment.span.end <= end)
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Function, AstType::ArrowFunctionExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (body_opt, span, is_method) = match node.kind() {
            AstKind::Function(func) => {
                // Check if this is a constructor with parameter properties
                // by looking at parent for MethodDefinition context.
                let parent = semantic.nodes().parent_node(node.id());
                let is_method = matches!(parent.kind(), AstKind::MethodDefinition(_));
                (func.body.as_ref(), func.span, is_method)
            }
            AstKind::ArrowFunctionExpression(arrow) => {
                (Some(&arrow.body), arrow.span, false)
            }
            _ => return,
        };

        let Some(body) = body_opt else { return };

        // Arrow functions with expression bodies (no block) are never empty.
        if matches!(node.kind(), AstKind::ArrowFunctionExpression(arrow) if arrow.expression) {
            return;
        }

        if !is_empty_body(body, semantic) {
            return;
        }

        // An empty function as the RIGHT operand of `??` or `||` is a no-op
        // fallback (`existing ?? (() => {})`): the empty body is the intended
        // behavior when the left operand is nullish/falsy, not dead code. Exempt
        // regardless of file kind.
        if is_logical_fallback_position(semantic.nodes(), node.id()) {
            return;
        }

        // Empty method stubs in a type-constrained object literal are mandatory
        // no-op implementations of the constraining interface's methods (Null
        // Object pattern), not dead code.
        if is_typed_object_literal_method_stub(semantic.nodes(), node.id()) {
            return;
        }

        // The callable target of `new Proxy(() => {}, handler)` must be callable
        // and its body is intentionally empty — all call behavior is delegated to
        // the handler's `apply` / `construct` trap. Structurally mandated, so it
        // is exempt in production code as well as tests.
        if is_callable_proxy_target(semantic.nodes(), node.id()) {
            return;
        }

        // A no-op function as a JSX attribute value (`onClick={() => {}}`) is a
        // deliberate placeholder satisfying a required event-handler prop. The
        // attribute's interface mandates it in production as well as tests, so it
        // is exempt regardless of file kind.
        if is_jsx_attribute_callback_position(semantic.nodes(), node.id()) {
            return;
        }

        // Other placeholder callback positions — call/new arguments — stay exempt
        // only in test files. Dual-read: the unit-test harness injects an empty
        // default FileCtx, so `in_test_dir` is false in tests — fall back to the
        // local check, which also covers the `_test.` infix `in_test_dir` does not.
        if (ctx.file.path_segments.in_test_dir || is_test_file(ctx.path))
            && is_placeholder_callback_position(semantic.nodes(), node.id())
        {
            return;
        }

        // Skip dependency-injection constructors: a constructor whose parameters
        // carry an accessibility modifier (`private`/`public`/`protected`),
        // `readonly`, or a decorator (e.g. `@Inject(...)`) is a parameter-property
        // constructor — the parameters ARE the work, not an empty body.
        if is_method
            && let AstKind::MethodDefinition(method) = semantic.nodes().parent_node(node.id()).kind()
            && method.key.is_specific_id("constructor")
            && let AstKind::Function(func) = node.kind()
            && func.params.items.iter().any(|param| {
                param.accessibility.is_some() || param.readonly || !param.decorators.is_empty()
            })
        {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Unexpected empty function.".into(),
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
    

    #[test]
    fn allows_empty_arrow_in_jsx_prop_in_test_file() {
        let src = r#"
            const x = <Foo onClose={() => {}} />;
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "Foo.test.tsx").is_empty());
    }

    #[test]
    fn allows_empty_function_expression_in_jsx_prop_in_test_file() {
        let src = r#"
            const x = <Foo onClose={function () {}} />;
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "Foo.test.tsx").is_empty());
    }

    #[test]
    fn allows_empty_arrow_as_call_argument_in_test_file() {
        let src = r#"
            useEffect(() => {}, []);
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "Foo.test.tsx").is_empty());
    }

    #[test]
    fn allows_parenthesized_empty_arrow_as_call_argument_in_test_file() {
        // Regression: useEffect((() => {}), []) — ParenthesizedExpression parent
        // must not fall through to the `_ => false` arm.
        let src = r#"
            useEffect((() => {}), []);
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "Foo.test.tsx").is_empty());
    }

    #[test]
    fn flags_empty_arrow_in_variable_assignment_in_test_file() {
        // Negative control: direct assignment is not a placeholder callback position.
        let src = r#"
            const handler = () => {};
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "Foo.test.tsx");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_named_function_declaration_in_test_file() {
        let src = r#"
            function doNothing() {}
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "Foo.test.tsx");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_empty_arrow_in_jsx_prop_in_non_test_file() {
        // Repro #6968 (remix-run/remix bench): a no-op handler satisfying a
        // required JSX event-handler prop is a deliberate placeholder, exempt in
        // production as well as tests.
        let src = r#"
            const x = (
                <tr onClick={() => {}}>
                    <td>{row.id}</td>
                </tr>
            );
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").is_empty());
    }

    #[test]
    fn flags_empty_arrow_as_jsx_child_in_non_test_file() {
        // Negative space: a bare function as a JSX *child* is not an attribute
        // value (it satisfies no prop contract), so it is still flagged.
        let src = r#"
            const x = <div>{() => {}}</div>;
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.tsx");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_empty_arrow_as_call_argument_in_non_test_file() {
        // Negative space: a plain empty arrow as a call argument is NOT a JSX
        // attribute placeholder; outside test files it is still flagged.
        let src = r#"
            foo(() => {});
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.tsx");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_empty_arrow_as_plain_call_argument_in_test_file() {
        // The non-JSX placeholder positions stay exempt in test files.
        let src = r#"
            foo(() => {});
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "Foo.test.tsx").is_empty());
    }

    #[test]
    fn allows_di_constructor_with_decorated_param() {
        // NestJS: a decorated DI parameter is the constructor's purpose.
        let src = r#"
            export class HelperService {
                constructor(@Inject(REQUEST) request) {}
            }
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "helper.service.ts").is_empty());
    }

    #[test]
    fn allows_di_constructor_with_decorated_param_property() {
        let src = r#"
            export class HelperService {
                constructor(@Inject(REQUEST) public readonly request) {}
            }
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "helper.service.ts").is_empty());
    }

    #[test]
    fn allows_constructor_with_readonly_param_property() {
        let src = r#"
            export class HelperService {
                constructor(readonly request: Request) {}
            }
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "helper.service.ts").is_empty());
    }

    #[test]
    fn allows_method_with_comment_only_body() {
        let src = r#"
            export class HelperService {
                public noop() {
                    // intentionally empty
                }
            }
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "helper.service.ts").is_empty());
    }

    #[test]
    fn allows_function_with_block_comment_only_body() {
        let src = r#"
            function noop() {
                /* intentionally empty */
            }
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }

    #[test]
    fn flags_empty_constructor_with_plain_param() {
        // Negative space: a plain (non-property, non-decorated) param is not DI.
        let src = r#"
            export class Foo {
                constructor(request: Request) {}
            }
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "foo.ts");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_empty_no_arg_constructor() {
        // Negative space: a bare empty constructor with no params is still dead.
        let src = r#"
            export class Foo {
                constructor() {}
            }
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "foo.ts");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_empty_noop_method_without_comment() {
        // Negative space: a `noop` method with NO comment is still flagged.
        let src = r#"
            export class Foo {
                public noop() {}
            }
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "foo.ts");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_empty_stubs_in_typed_object_literal() {
        // Repro #6275: Null-Object stubs of an interface in a typed object literal.
        let src = r#"
            const inertActorScope: ActorScope<X> = {
                defer: () => {},
                logger: () => {},
                emit: () => {},
            };
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "actorScope.ts").is_empty());
    }

    #[test]
    fn allows_empty_stubs_in_object_literal_with_satisfies() {
        let src = r#"
            const x = { m: () => {} } satisfies SomeIface;
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }

    #[test]
    fn allows_empty_stubs_in_object_literal_with_as() {
        let src = r#"
            const y = { m: () => {} } as SomeIface;
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }

    #[test]
    fn allows_empty_function_expression_stub_in_typed_object_literal() {
        let src = r#"
            const x: SomeIface = { m: function () {} };
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }

    #[test]
    fn flags_empty_arrow_in_untyped_object_literal() {
        // Negative space (the key distinction): an UNTYPED object literal's empty
        // arrow may be a forgotten ad-hoc handler — still flagged.
        let src = r#"
            const handlers = { onClick: () => {} };
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.ts");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_empty_arrow_in_object_literal_with_as_const() {
        // Negative space: `as const` is a const-assertion, not an interface
        // constraint — the empty arrow may be a forgotten handler, still flagged.
        let src = r#"
            const handlers = { onClick: () => {} } as const;
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.ts");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_empty_arrow_in_object_literal_with_as_any() {
        // Negative space: `as any` is an escape hatch, not an interface constraint.
        let src = r#"
            const handlers = { onClick: () => {} } as any;
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.ts");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_empty_function_declaration_outside_object_literal() {
        let src = r#"
            function foo() {}
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.ts");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_empty_arrow_in_variable_init() {
        // Negative space: a variable init (not an object property) still flags.
        let src = r#"
            const f = () => {};
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.ts");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_empty_arrow_callable_proxy_target_in_production() {
        // Repro #6664 (unjs/magicast): the proxy target must be callable; its body
        // is empty because the handler's `apply` trap owns all call behavior. The
        // `get` / `apply` traps are non-empty so the arrow is the only candidate.
        let src = r#"
            const p = new Proxy(() => {}, {
                get(target, key) { return target[key]; },
                apply() { throw new Error("x"); },
            });
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "proxy.ts").is_empty());
    }

    #[test]
    fn allows_empty_function_expression_callable_proxy_target_in_production() {
        // Function-expression target form, handler with an `apply` trap.
        let src = r#"
            const p = new Proxy(function () {}, {
                apply() { return 1; },
            });
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "proxy.ts").is_empty());
    }

    #[test]
    fn allows_empty_arrow_proxy_target_with_non_literal_handler_in_production() {
        // Fallback: the handler is a variable reference, so its traps cannot be
        // inspected. `Proxy`'s first argument is required to be callable, so the
        // empty callable target is legitimate on the shape alone.
        let src = r#"
            const p = new Proxy(() => {}, handler);
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "proxy.ts").is_empty());
    }

    #[test]
    fn flags_empty_arrow_proxy_target_when_handler_lacks_call_trap_in_production() {
        // Negative space: an object-literal handler with no `apply` / `construct`
        // trap does not delegate the call, so the empty target body is still dead.
        let src = r#"
            const p = new Proxy(() => {}, { get() { return 1; } });
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "proxy.ts");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_empty_arrow_non_proxy_new_expression_in_production() {
        // Negative space: the exemption is specific to the `Proxy` global; an
        // empty target of any other constructor is still flagged.
        let src = r#"
            const p = new NotProxy(() => {}, {});
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "proxy.ts");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_empty_arrow_as_nullish_coalescing_fallback_in_production() {
        // Repro #6330 (mobxjs/mobx): an empty arrow as the right operand of `??`
        // is a callable no-op fallback used when the left side is null/undefined.
        let src = r#"
            const clearTimers = reg["finalizeAllImmediately"] ?? (() => {});
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "index.ts").is_empty());
    }

    #[test]
    fn allows_empty_arrow_as_logical_or_fallback_in_production() {
        let src = r#"
            const f = existing || (() => {});
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }

    #[test]
    fn allows_empty_function_expression_as_logical_or_fallback_in_production() {
        let src = r#"
            const f = existing || function () {};
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }

    #[test]
    fn flags_plain_empty_arrow_assignment_not_a_fallback() {
        // Negative space: a bare assignment is not a fallback position — the guard
        // must not broaden to all empty functions.
        let src = r#"
            const f = () => {};
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.ts");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_empty_arrow_as_logical_and_operand() {
        // Negative space: `&&` is not a fallback idiom — only `||` / `??` qualify.
        let src = r#"
            const f = cond && (() => {});
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.ts");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_empty_arrow_as_left_operand_of_fallback() {
        // Negative space: a LEFT-operand empty function is not a fallback; only the
        // right operand (the no-op default) is exempt.
        let src = r#"
            const f = (() => {}) || existing;
        "#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.ts");
        assert_eq!(diags.len(), 1);
    }
}
