//! ts-no-shadow OXC backend — variable shadowing detection via oxc_semantic.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{
    byte_offset_to_line_col, is_type_only_binding_context, is_type_only_import_binding,
};
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_ast::AstKind;
use oxc_ast::ast::BindingPattern;
use oxc_semantic::{AstNodes, NodeId};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let scoping = semantic.scoping();
        let nodes = semantic.nodes();
        let mut diagnostics = Vec::new();

        for symbol_id in scoping.symbol_ids() {
            let scope_id = scoping.symbol_scope_id(symbol_id);
            let Some(parent_scope) = scoping.scope_parent_id(scope_id) else {
                continue;
            };
            let name = scoping.symbol_name(symbol_id);
            let decl_node = scoping.symbol_declaration(symbol_id);
            // Enum members are scoped inside the enum object and are only
            // reachable as `Enum.Member`, so they never shadow a module binding.
            if matches!(nodes.kind(decl_node), AstKind::TSEnumMember(_)) {
                continue;
            }
            if std::iter::once(nodes.kind(decl_node))
                .chain(nodes.ancestor_kinds(decl_node))
                .any(is_type_only_binding_context)
            {
                continue;
            }
            let ident = oxc_str::Ident::from(name);
            if let Some(outer_symbol) = scoping.find_binding(parent_scope, ident) {
                // A type-only outer binding lives in the type namespace and is
                // erased at compile time: a `type` alias, an interface, or a
                // type-only import (`import type ...` / `import { type X }`).
                // None create a runtime binding, so a value declaration of the
                // same name shadows nothing observable.
                let outer_decl = scoping.symbol_declaration(outer_symbol);
                if is_type_only_binding_context(nodes.kind(outer_decl))
                    || is_type_only_import_binding(nodes, outer_decl)
                {
                    continue;
                }
                // Static methods cannot access the enclosing class's type
                // parameters, so a type parameter on a static method that
                // reuses the class generic's name is a separate binding, not
                // shadowing (`AnimatedArray.create<T>` over `class
                // AnimatedArray<T>`). This applies only when the outer binding
                // is the class's own type parameter.
                if is_type_param_on_static_method(nodes, decl_node)
                    && is_class_type_param(nodes, outer_decl)
                {
                    continue;
                }
                // A named function expression passed as a call argument whose
                // name matches the enclosing `const`/`let`/`var` binding is the
                // self-reference display-name idiom (`const Foo = wrap(function
                // Foo() {})`), used by `forwardRef`, `memo`, `observer`,
                // `styled(...)`, and custom HOCs to give the wrapped component a
                // stable name for stack traces. The function-expression's own
                // name binding lives only in its own scope (ECMA-262 §15.2.4),
                // so it shadows nothing observable to outside callers.
                if is_named_fn_expr_self_reference(nodes, decl_node, name) {
                    continue;
                }
                // The UMD / module-factory idiom passes an outer binding into an
                // immediately-invoked function expression whose parameter
                // deliberately re-binds the same name to create a local alias
                // (`(function (Prism) { ... })(Prism)`). The parameter shadow is
                // the intended encapsulation, not an accidental one, so a
                // parameter of an IIFE whose matching argument is the outer
                // binding of the same name is exempt.
                if is_iife_parameter_aliasing_argument(nodes, decl_node, name) {
                    continue;
                }
                let span = scoping.symbol_span(symbol_id);
                let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!("`{name}` is already declared in an outer scope."),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }

        diagnostics
    }
}

/// True when `decl` declares a generic type parameter whose nearest enclosing
/// function is the value of a `static` method.
fn is_type_param_on_static_method(nodes: &AstNodes, decl: NodeId) -> bool {
    if !matches!(nodes.kind(decl), AstKind::TSTypeParameter(_)) {
        return false;
    }
    let mut ids = nodes.ancestor_ids(decl);
    let Some(function_id) = ids.find(|&id| matches!(nodes.kind(id), AstKind::Function(_))) else {
        return false;
    };
    matches!(nodes.parent_kind(function_id), AstKind::MethodDefinition(m) if m.r#static)
}

/// True when `decl` declares a type parameter that belongs directly to a class's
/// type-parameter list (`class Foo<T>`), as opposed to a method or function.
fn is_class_type_param(nodes: &AstNodes, decl: NodeId) -> bool {
    matches!(nodes.kind(decl), AstKind::TSTypeParameter(_))
        && matches!(nodes.parent_kind(nodes.parent_id(decl)), AstKind::Class(_))
}

/// True when `decl` is the self-binding of a named function expression passed as
/// a call argument whose own name (`symbol_name`) matches the name of the
/// `const`/`let`/`var` declarator the call initializes
/// (`const Foo = wrap(function Foo() {})`).
fn is_named_fn_expr_self_reference(nodes: &AstNodes, decl: NodeId, symbol_name: &str) -> bool {
    // The shadowing symbol must be declared by a named function expression
    // whose own identifier equals the symbol name.
    let AstKind::Function(func) = nodes.kind(decl) else {
        return false;
    };
    if func.id.as_ref().is_none_or(|id| id.name.as_str() != symbol_name) {
        return false;
    }
    // It must be a direct argument of a call expression (a function
    // *declaration* can never be a child of a `CallExpression`, so this also
    // confirms it is an expression rather than a declaration).
    if !matches!(nodes.parent_kind(decl), AstKind::CallExpression(_)) {
        return false;
    }
    // The call initializes a `const`/`let`/`var` declarator whose bound name
    // matches the function-expression name.
    nodes
        .ancestor_kinds(decl)
        .find_map(|kind| match kind {
            AstKind::VariableDeclarator(declarator) => Some(declarator),
            _ => None,
        })
        .is_some_and(|declarator| match &declarator.id {
            BindingPattern::BindingIdentifier(id) => id.name.as_str() == symbol_name,
            _ => false,
        })
}

/// True when `decl` is a parameter of an immediately-invoked function expression
/// whose argument at the same position is the outer binding of the same name —
/// the UMD / module-factory aliasing idiom `(function (X) { ... })(X)` (also in
/// arrow form `((X) => { ... })(X)`).
///
/// The discriminator is structural: the parameter belongs to a function
/// expression that is the callee of the `CallExpression` invoking it in place
/// (`ParenthesizedExpression` wrappers are transparent), and the call passes a
/// plain identifier of `symbol_name` at the parameter's index. A function that
/// is merely *referenced* by a call rather than *being* its callee, or one whose
/// matching argument is anything other than that same-named identifier, is not
/// exempt.
fn is_iife_parameter_aliasing_argument(
    nodes: &AstNodes,
    decl: NodeId,
    symbol_name: &str,
) -> bool {
    use oxc_ast::ast::{Argument, Expression};

    let AstKind::FormalParameter(param) = nodes.kind(decl) else {
        return false;
    };
    let BindingPattern::BindingIdentifier(id) = &param.pattern else {
        return false;
    };
    if id.name.as_str() != symbol_name {
        return false;
    }

    // The nearest enclosing callable owns this parameter; its parameter list
    // gives the index to match against the call argument.
    let Some(fn_id) = nodes.ancestor_ids(decl).find(|&id| {
        matches!(
            nodes.kind(id),
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
        )
    }) else {
        return false;
    };
    let (params, fn_span) = match nodes.kind(fn_id) {
        AstKind::Function(func) => (&func.params, func.span),
        AstKind::ArrowFunctionExpression(arrow) => (&arrow.params, arrow.span),
        _ => return false,
    };
    let Some(param_index) = params
        .items
        .iter()
        .position(|item| item.span == param.span)
    else {
        return false;
    };

    // Walk past `ParenthesizedExpression` wrappers to the node that uses the
    // function; an IIFE's function expression is the callee of that call.
    let mut current = fn_id;
    while matches!(nodes.parent_kind(current), AstKind::ParenthesizedExpression(_)) {
        current = nodes.parent_id(current);
    }
    let AstKind::CallExpression(call) = nodes.parent_kind(current) else {
        return false;
    };
    let callee_span = match crate::oxc_helpers::peel_parens(&call.callee) {
        Expression::FunctionExpression(func) => func.span,
        Expression::ArrowFunctionExpression(arrow) => arrow.span,
        _ => return false,
    };
    if callee_span != fn_span {
        return false;
    }

    matches!(
        call.arguments.get(param_index),
        Some(Argument::Identifier(arg)) if arg.name.as_str() == symbol_name
    )
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
    fn allows_index_signature_parameter_with_shadow() {
        let d = run_on("interface I { [key: string]: number } const key = \"x\";");
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn allows_mapped_type_key_with_shadow() {
        let d = run_on("type M<T> = { [K in keyof T]: T[K] }; const K = 1;");
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn allows_infer_type_parameter_with_shadow() {
        let d = run_on("type Unpack<T> = T extends Promise<infer R> ? R : never; const R = 1;");
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn allows_enum_member_matching_interface_name() {
        let d = run_on(
            "export enum KnownIdentityType {\n  \
             SystemAssignedIdentity = \"systemAssignedIdentity\",\n  \
             UserAssignedIdentity = \"userAssignedIdentity\",\n}\n\
             export interface UserAssignedIdentity { clientId?: string; }",
        );
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn still_flags_shadowing_in_real_function() {
        // Real function params still flag as shadows.
        let d = run_on("const x = 1; function f(x: number) { return x; }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_param_shadowing_type_alias() {
        // `type tag = ...` lives in the type namespace only; a value parameter
        // named `tag` creates a runtime binding that shadows nothing observable.
        let d = run_on(
            "type tag = keyof HTMLElementTagNameMap;\n\
             export function isElementType<T extends tag>(element: Element, tag: T | T[]) { return tag; }",
        );
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn allows_param_shadowing_interface() {
        // An interface is a type-namespace-only declaration; a value parameter
        // of the same name shadows nothing at runtime.
        let d = run_on(
            "interface Options { id: number }\n\
             export function build(Options: number) { return Options; }",
        );
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn allows_param_shadowing_type_only_default_import() {
        // `import type yargs` is erased at runtime; a value param named `yargs`
        // shadows nothing observable.
        let d = run_on(
            "import type yargs from 'yargs';\n\
             export function builder(yargs: yargs.Argv) { return yargs; }",
        );
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn allows_param_shadowing_type_only_namespace_import() {
        let d = run_on(
            "import type * as yargs from 'yargs';\n\
             export function builder(yargs: yargs.Argv) { return yargs; }",
        );
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn allows_param_shadowing_type_only_named_import() {
        let d = run_on(
            "import { type Argv } from 'yargs';\n\
             export function builder(Argv: number) { return Argv; }",
        );
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn allows_param_shadowing_inline_type_named_import() {
        let d = run_on(
            "import type { Argv } from 'yargs';\n\
             export function builder(Argv: number) { return Argv; }",
        );
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn still_flags_param_shadowing_value_import() {
        // A real value import is a runtime binding, so shadowing it still fires.
        let d = run_on(
            "import yargs from 'yargs';\n\
             export function builder(yargs: number) { return yargs; }",
        );
        assert_eq!(d.len(), 1, "expected one diagnostic, got: {d:?}");
    }

    #[test]
    fn still_flags_param_shadowing_named_value_import() {
        let d = run_on(
            "import { Argv } from 'yargs';\n\
             export function builder(Argv: number) { return Argv; }",
        );
        assert_eq!(d.len(), 1, "expected one diagnostic, got: {d:?}");
    }

    #[test]
    fn allows_static_method_type_param_shadowing_class_type_param() {
        // Static methods cannot access the class's type parameters, so reusing
        // the class generic's name on a static method is a separate binding,
        // not shadowing. react-spring AnimatedArray pattern.
        let d = run_on(
            "class AnimatedArray<T extends ReadonlyArray<Value> = Value[]> extends AnimatedObject {\n\
             constructor(source: T) { super(source); }\n\
             static create<T extends ReadonlyArray<Value>>(source: T) { return new AnimatedArray(source); }\n\
             }",
        );
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn still_flags_instance_method_type_param_shadowing_class_type_param() {
        // Instance methods CAN access the class's type parameters, so reusing
        // the class generic's name on an instance method is real shadowing.
        let d = run_on(
            "class Box<T> {\n\
             map<T>(fn: (value: T) => T): T { return fn(undefined as T); }\n\
             }",
        );
        assert_eq!(d.len(), 1, "expected one diagnostic, got: {d:?}");
    }

    #[test]
    fn still_flags_static_method_value_param_shadowing_outer_value() {
        // The static-method exemption is scoped to type parameters; a value
        // parameter on a static method that shadows an outer value still fires.
        let d = run_on(
            "const source = 1;\n\
             class Factory {\n\
             static create(source: number) { return source; }\n\
             }",
        );
        assert_eq!(d.len(), 1, "expected one diagnostic, got: {d:?}");
    }

    #[test]
    fn allows_forward_ref_named_function_expression() {
        // The named function expression reuses the outer const name only for
        // display/debugging; it is the self-reference idiom, not a shadow.
        let d = run_on("const Foo = forwardRef(function Foo(props, ref) { return null; });");
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn allows_memo_named_function_expression() {
        let d = run_on("const Foo = memo(function Foo() { return null; });");
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn allows_arbitrary_hoc_named_function_expression() {
        // Issue #1697 reproduction: any higher-order wrapper, not just
        // forwardRef. The named function expression is an argument in any
        // position of the call.
        let d = run_on(
            "export const PromptButton = clientEntry(\n\
             import.meta.url,\n\
             function PromptButton(handle) { return () => {}; });",
        );
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn still_flags_function_declaration_shadowing_outer_const() {
        // A function *declaration* (not an expression passed to a call) named
        // like the outer const is a genuine shadow and must still fire.
        let d = run_on("const Foo = 1; function outer() { function Foo() {} return Foo; }");
        assert_eq!(d.len(), 1, "expected one diagnostic, got: {d:?}");
    }

    #[test]
    fn allows_iife_parameter_aliasing_imported_value() {
        // Issue #1653 reproduction: the UMD / Prism-plugin idiom passes the
        // outer import into an IIFE whose parameter re-binds the same name.
        let d = run_on(
            "import { Prism } from 'prism-react-renderer';\n\
             (function (Prism) { Prism.languages.bash = {}; })(Prism);",
        );
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn allows_arrow_iife_parameter_aliasing_outer_binding() {
        // The same idiom in arrow form.
        let d = run_on(
            "const config = { debug: false };\n\
             ((config) => { config.debug = true; })(config);",
        );
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn still_flags_nested_non_iife_function_param_shadowing_import() {
        // Negative space: a parameter of an ordinary (not immediately invoked)
        // function that shadows an outer import is a genuine accidental shadow.
        let d = run_on(
            "import { Prism } from 'prism-react-renderer';\n\
             function extend(Prism) { return Prism; }",
        );
        assert_eq!(d.len(), 1, "expected one diagnostic, got: {d:?}");
    }

    #[test]
    fn still_flags_nested_const_shadowing_outer_binding() {
        // Negative space: an inner `const` shadowing an outer binding is a real
        // shadow regardless of any IIFE elsewhere.
        let d = run_on(
            "const value = 1;\n\
             function outer() { const value = 2; return value; }",
        );
        assert_eq!(d.len(), 1, "expected one diagnostic, got: {d:?}");
    }

    #[test]
    fn still_flags_iife_parameter_when_argument_is_a_different_binding() {
        // The exemption requires the matching argument to be the same-named
        // outer binding. Passing a *different* identifier is real shadowing.
        let d = run_on(
            "import { Prism } from 'prism-react-renderer';\n\
             const other = {};\n\
             (function (Prism) { Prism.x = 1; })(other);",
        );
        assert_eq!(d.len(), 1, "expected one diagnostic, got: {d:?}");
    }

    #[test]
    fn still_flags_function_expression_referenced_not_immediately_invoked() {
        // A function expression assigned to a variable and shadowing an outer
        // binding is not an IIFE; the parameter shadow still fires.
        let d = run_on(
            "import { Prism } from 'prism-react-renderer';\n\
             const plugin = function (Prism) { return Prism; };",
        );
        assert_eq!(d.len(), 1, "expected one diagnostic, got: {d:?}");
    }

    #[test]
    fn still_flags_named_fn_expr_with_name_differing_from_outer_const() {
        // The exemption keys off the self-reference relationship (inner name ==
        // enclosing const name). A named function expression whose name differs
        // from the const but collides with a different outer binding is a real
        // shadow.
        let d = run_on(
            "const handler = 1;\n\
             const Component = memo(function handler() { return null; });",
        );
        assert_eq!(d.len(), 1, "expected one diagnostic, got: {d:?}");
    }
}
