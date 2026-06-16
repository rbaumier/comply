//! ts-no-invalid-void-type OXC backend — flag `void` used outside return
//! type annotations and generic type arguments.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_return_type_context(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    void_start: u32,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::Function(f) => {
                if let Some(ret) = &f.return_type
                    && void_start >= ret.span.start && void_start < ret.span.end {
                        return true;
                    }
                return false;
            }
            AstKind::ArrowFunctionExpression(f) => {
                if let Some(ret) = &f.return_type
                    && void_start >= ret.span.start && void_start < ret.span.end {
                        return true;
                    }
                return false;
            }
            // TS function-type signatures like `(open: boolean) => void`
            // (in a parameter / variable annotation, NOT an arrow
            // expression). The whole type's `return_type` is required.
            AstKind::TSFunctionType(ft) => {
                let ret_span = ft.return_type.span;
                return void_start >= ret_span.start && void_start < ret_span.end;
            }
            AstKind::TSConstructorType(ct) => {
                let ret_span = ct.return_type.span;
                return void_start >= ret_span.start && void_start < ret_span.end;
            }
            AstKind::TSMethodSignature(ms) => {
                if let Some(ret) = &ms.return_type
                    && void_start >= ret.span.start && void_start < ret.span.end {
                        return true;
                    }
                return false;
            }
            // Interface call signatures like `(ctx: X): Foo | void` — the
            // `return_type` is the boundary, reached before TSInterfaceDeclaration.
            AstKind::TSCallSignatureDeclaration(cs) => {
                if let Some(ret) = &cs.return_type
                    && void_start >= ret.span.start && void_start < ret.span.end {
                        return true;
                    }
                return false;
            }
            AstKind::TSTypeAliasDeclaration(_) | AstKind::TSInterfaceDeclaration(_) => {
                break;
            }
            _ => continue,
        }
    }
    false
}

fn is_generic_type_arg(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::TSTypeParameterInstantiation(_) => return true,
            AstKind::Function(_)
            | AstKind::ArrowFunctionExpression(_)
            | AstKind::Class(_) => return false,
            _ => continue,
        }
    }
    false
}

// `<T = void>` — `void` as the default of a generic type parameter is valid
// TypeScript (it sets `T` when the caller omits the argument). The constraint
// position (`<T extends void>`) is NOT exempted here.
fn is_generic_param_default(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    void_start: u32,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        if let AstKind::TSTypeParameter(param) = ancestor.kind() {
            return param.default.as_ref().is_some_and(|default| {
                void_start >= default.span().start && void_start < default.span().end
            });
        }
    }
    false
}

// `void` as a member of a union type at the top level of a type alias or
// conditional type (`type X = ... | void`, `T extends U ? A : B | void`) is
// valid TypeScript: it marks the resolved value as discardable in a type-level
// computation. Scoped to those two contexts — a `void` union inside a parameter
// or return annotation is left to `is_return_type_context`, so crossing a
// function/parameter boundary disqualifies the exemption.
fn is_union_member_in_type_level_alias(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let mut saw_union = false;
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::TSUnionType(_) => saw_union = true,
            AstKind::TSConditionalType(_) | AstKind::TSTypeAliasDeclaration(_) => {
                return saw_union;
            }
            // A function/parameter boundary means the union sits in a
            // parameter or return position, not a top-level type-level union.
            AstKind::FormalParameter(_)
            | AstKind::TSFunctionType(_)
            | AstKind::TSConstructorType(_)
            | AstKind::TSMethodSignature(_)
            | AstKind::TSCallSignatureDeclaration(_)
            | AstKind::Function(_)
            | AstKind::ArrowFunctionExpression(_)
            | AstKind::Class(_)
            | AstKind::TSInterfaceDeclaration(_) => return false,
            _ => continue,
        }
    }
    false
}

// The rightmost identifier of a heritage reference (`PromiseLike`,
// `ns.PromiseLike`). Returns `None` for non-identifier expressions.
fn heritage_head_name<'a>(expr: &'a oxc_ast::ast::Expression<'a>) -> Option<&'a str> {
    use oxc_ast::ast::Expression;
    match expr {
        Expression::Identifier(id) => Some(id.name.as_str()),
        Expression::StaticMemberExpression(member) => Some(member.property.name.as_str()),
        _ => None,
    }
}

fn ts_type_name_head<'a>(name: &'a oxc_ast::ast::TSTypeName<'a>) -> &'a str {
    use oxc_ast::ast::TSTypeName;
    match name {
        TSTypeName::IdentifierReference(id) => id.name.as_str(),
        TSTypeName::QualifiedName(q) => q.right.name.as_str(),
        TSTypeName::ThisExpression(_) => "",
    }
}

fn is_promise_like(name: &str) -> bool {
    name == "PromiseLike" || name == "Promise"
}

// `(value: void) => ...` is structurally required when the enclosing
// class/interface implements (or extends) `PromiseLike<...>` / `Promise<...>`:
// the callback parameter must type the resolved value as `void`. Detected via
// the heritage clause (`implements`/`extends`), never the method name. The
// `void` must be in a parameter position (a `FormalParameter` is crossed on the
// way up), so a plain `let x: void` inside such a class still fires.
fn is_promise_like_callback_param(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let mut in_parameter = false;
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::FormalParameter(_) => in_parameter = true,
            AstKind::Class(class) if in_parameter => {
                let super_match = class
                    .super_class
                    .as_ref()
                    .and_then(heritage_head_name)
                    .is_some_and(is_promise_like);
                let implements_match = class
                    .implements
                    .iter()
                    .any(|clause| is_promise_like(ts_type_name_head(&clause.expression)));
                return super_match || implements_match;
            }
            AstKind::TSInterfaceDeclaration(iface) if in_parameter => {
                return iface
                    .extends
                    .iter()
                    .filter_map(|heritage| heritage_head_name(&heritage.expression))
                    .any(is_promise_like);
            }
            AstKind::Class(_) | AstKind::TSInterfaceDeclaration(_) => return false,
            _ => continue,
        }
    }
    false
}

// `type T = { foo: void }` — `void` as the type annotation of a property in a
// type literal or interface body is valid TypeScript: it is a type-level
// position, not a value annotation. Scoped via span containment to the
// property's own `type_annotation`, and a function/parameter boundary crossed
// first disqualifies it (so `{ foo: (x: void) => string }` still fires on the
// parameter).
fn is_type_literal_property_type(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    void_start: u32,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::TSPropertySignature(sig) => {
                return sig.type_annotation.as_ref().is_some_and(|annotation| {
                    void_start >= annotation.span.start && void_start < annotation.span.end
                });
            }
            AstKind::FormalParameter(_)
            | AstKind::TSFunctionType(_)
            | AstKind::TSConstructorType(_)
            | AstKind::TSMethodSignature(_)
            | AstKind::TSCallSignatureDeclaration(_)
            | AstKind::Function(_)
            | AstKind::ArrowFunctionExpression(_)
            | AstKind::Class(_) => return false,
            _ => continue,
        }
    }
    false
}

// `type T = [void, number]` — `void` as a tuple element type is valid
// TypeScript: tuple positions are type-level. A function/parameter boundary
// crossed before the `TSTupleType` disqualifies it (so `[(x: void) => string]`
// still fires on the parameter).
fn is_tuple_element_type(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::TSTupleType(_) => return true,
            AstKind::FormalParameter(_)
            | AstKind::TSFunctionType(_)
            | AstKind::TSConstructorType(_)
            | AstKind::TSMethodSignature(_)
            | AstKind::TSCallSignatureDeclaration(_)
            | AstKind::Function(_)
            | AstKind::ArrowFunctionExpression(_)
            | AstKind::Class(_) => return false,
            _ => continue,
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSVoidKeyword]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSVoidKeyword(kw) = node.kind() else {
            return;
        };

        if is_return_type_context(node, semantic, kw.span.start) {
            return;
        }
        if is_generic_type_arg(node, semantic) {
            return;
        }
        if is_generic_param_default(node, semantic, kw.span.start) {
            return;
        }
        if is_union_member_in_type_level_alias(node, semantic) {
            return;
        }
        if is_promise_like_callback_param(node, semantic) {
            return;
        }
        if is_type_literal_property_type(node, semantic, kw.span.start) {
            return;
        }
        if is_tuple_element_type(node, semantic) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, kw.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`void` is only valid as a return type or generic type argument."
                .into(),
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_void_variable() {
        let diags = run_on("let x: void;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_void_parameter() {
        let diags = run_on("function foo(x: void) {}");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_void_return_type() {
        assert!(run_on("function foo(): void {}").is_empty());
    }

    #[test]
    fn allows_void_in_generic() {
        assert!(run_on("let x: Promise<void>;").is_empty());
    }

    #[test]
    fn allows_void_in_function_type_callback() {
        // Regression for rbaumier/comply#20 — TS function type with void
        // return, common in callback prop declarations.
        let diags = run_on("type OnChange = (open: boolean) => void;");
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_void_in_inline_function_type() {
        let src = r#"function setup(cb: (n: number) => void) { cb(1); }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_void_in_method_signature() {
        let src = "interface Listener { onChange(open: boolean): void }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_void_in_constructor_type() {
        let src = "type Make = new (x: number) => void;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_void_as_type_alias_generic_default() {
        // Regression for rbaumier/comply#1094 — `void` as a generic default.
        let src = "export type Fn<T = void> = (...values: any[]) => T;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_void_as_function_generic_default() {
        // Regression for rbaumier/comply#1094 — azure-sdk-for-js poller.
        let src = "export function poll<TResponse, TResult = void>(\
                   p: (r: TResponse) => Promise<TResult>) {}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_void_as_generic_constraint() {
        // The constraint position is still invalid, unlike the default.
        let diags = run_on("type Fn<T extends void> = () => T;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_void_in_call_signature_return_union() {
        // Regression for rbaumier/comply#1719 — vuejs/pinia PiniaPlugin: `void`
        // in a `| void` union return type of an interface call signature.
        let src = "interface PiniaPlugin { (context: PiniaPluginContext): Partial<X> | void }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_void_in_call_signature_param_union() {
        // Negative space: `void` in a union outside the return type (here a
        // parameter annotation of a call signature) is still invalid.
        let src = "interface F { (ctx: string | void): number }";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_void_union_in_conditional_type_alias() {
        // Regression for rbaumier/comply#1675 — elysiajs/elysia ResolveHandler:
        // `void` as a union member in a conditional type's branch.
        let src = "type ResolveHandler<A> = A extends Record<string, unknown> \
                   ? A : unknown | void;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_void_union_in_type_alias() {
        let src = "type Discardable = string | void;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_void_param_in_promise_like_class() {
        // Regression for rbaumier/comply#1675 — elysiajs/elysia PromiseGroup:
        // `(value: void)` callback param required by `implements PromiseLike<void>`.
        let src = "class PromiseGroup implements PromiseLike<void> {\
                   then<R = void>(onfulfilled?: ((value: void) => R) | null): PromiseLike<R> {\
                   return this as any; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_void_param_in_promise_like_interface() {
        let src = "interface Thenable extends PromiseLike<void> {\
                   then(onfulfilled?: (value: void) => unknown): void }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_void_param_in_non_promise_class() {
        // Negative space: `(value: void)` is only exempt when the enclosing
        // class implements PromiseLike/Promise — a plain class still fires.
        let src = "class Plain {\
                   then(onfulfilled?: (value: void) => unknown): void {} }";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_void_variable_in_promise_like_class() {
        // Negative space: the PromiseLike exemption is parameter-scoped — a
        // plain `let x: void` inside the class body still fires.
        let src = "class PromiseGroup implements PromiseLike<void> {\
                   run() { let x: void; } }";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_void_as_type_literal_property() {
        // Regression for rbaumier/comply#3325 — sindresorhus/type-fest
        // readonly-deep: `void` as a property type (bare and in a union) in a
        // type literal.
        let src = "type VoidType = { foo: void; bar: string | void };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_void_as_interface_property() {
        let src = "interface VoidType { foo: void }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_void_as_tuple_element() {
        // Regression for rbaumier/comply#3325 — sindresorhus/type-fest
        // split-on-rest-element: `void` as a tuple element type.
        let src = "type T = [void, number];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_void_as_tuple_element_with_rest() {
        let src = "type T = SplitOnRestElement<[void, ...never[], 1]>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_void_param_in_type_literal_property_function() {
        // Negative space: the property exemption is type-position-scoped — a
        // `void` parameter of a function-typed property still fires.
        let src = "type T = { foo: (x: void) => string };";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_void_param_in_tuple_function_element() {
        // Negative space: the tuple exemption is type-position-scoped — a
        // `void` parameter of a function-typed tuple element still fires.
        let src = "type T = [(x: void) => string];";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
    }
}
