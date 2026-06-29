use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, PropertyKey, TSSignature, TSType};
use oxc_semantic::Semantic;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::FormalParameter]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::FormalParameter(param) = node.kind() else {
            return;
        };
        // Check if the type annotation is an object type literal (TSTypeLiteral).
        let Some(annotation) = &param.type_annotation else {
            return;
        };
        let TSType::TSTypeLiteral(type_literal) = &annotation.type_annotation else {
            return;
        };
        // Destructured params: the inline type documents the destructured shape.
        if matches!(param.pattern, BindingPattern::ObjectPattern(_)) {
            return;
        }
        // React component props are conventionally inline.
        if is_react_component_param(semantic, node) {
            return;
        }
        // Ambient/bodyless declarations (`declare function`, overload
        // signatures): the annotation IS the signature, no body to reuse a
        // named type across, so extracting it only adds boilerplate.
        if is_ambient_function_param(semantic, node) {
            return;
        }
        // A parameter of a type-level function signature (`(ctx: {...}) => R`)
        // is part of a type declaration, not a function implementation. The
        // inline shape IS the type contract — there is no body to drift from
        // and nothing to extract for reuse — so the rule does not apply.
        if is_type_level_signature_param(semantic, node) {
            return;
        }
        let name = match &param.pattern {
            BindingPattern::BindingIdentifier(id) => id.name.as_str(),
            _ => "<param>",
        };
        // React props convention: a param named `props` or a single-use shape
        // carrying `children` (test wrappers, render helpers) stays inline.
        if name == "props" || type_has_children_member(type_literal) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, param.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Parameter '{name}' has an inline object type — extract \
                 it to a named `type` declaration above the function so \
                 the shape has an identity and can't silently drift."
            ),
            severity: super::META.severity,
            span: None,
        });
    }
}

/// True when the inline type literal declares a `children` member — the
/// React props shape used by component wrappers and render helpers.
fn type_has_children_member(type_literal: &oxc_ast::ast::TSTypeLiteral) -> bool {
    type_literal.members.iter().any(|member| {
        let TSSignature::TSPropertySignature(prop) = member else {
            return false;
        };
        matches!(&prop.key, PropertyKey::StaticIdentifier(id) if id.name == "children")
    })
}

/// True when `node` is a parameter of an ambient or bodyless function — an
/// explicit `declare function` or an overload signature. Both have no body,
/// so there is no implementation to factor a named type out of.
fn is_ambient_function_param<'a>(
    semantic: &'a Semantic<'a>,
    node: &oxc_semantic::AstNode<'a>,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()).skip(1) {
        match ancestor.kind() {
            // The enclosing function: ambient when declared or bodyless.
            AstKind::Function(func) => return func.declare || func.body.is_none(),
            // An arrow always has a body and is never ambient.
            AstKind::ArrowFunctionExpression(_) => return false,
            _ => continue,
        }
    }
    false
}

/// True when `node` is the first parameter of a function whose name starts
/// with an uppercase letter — the React component naming convention.
fn is_react_component_param<'a>(
    semantic: &'a Semantic<'a>,
    node: &oxc_semantic::AstNode<'a>,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()).skip(1) {
        match ancestor.kind() {
            AstKind::Function(func) => {
                return func
                    .id
                    .as_ref()
                    .is_some_and(|id| id.name.as_bytes().first().is_some_and(|b| b.is_ascii_uppercase()));
            }
            AstKind::VariableDeclarator(decl) => {
                if let BindingPattern::BindingIdentifier(id) = &decl.id {
                    return id.name.as_bytes().first().is_some_and(|b| b.is_ascii_uppercase());
                }
                return false;
            }
            _ => continue,
        }
    }
    false
}

/// True when the nearest function-like ancestor of `node` is a type-level
/// function signature (`TSFunctionType` / `TSConstructorType`) rather than a
/// real `Function` / `ArrowFunctionExpression`. The first function-like
/// ancestor reached decides it, so a parameter of a callback type used as an
/// interface property type or `type` alias is exempt, while an implementation
/// parameter whose type merely contains a nested function type is not.
fn is_type_level_signature_param<'a>(
    semantic: &'a Semantic<'a>,
    node: &oxc_semantic::AstNode<'a>,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()).skip(1) {
        match ancestor.kind() {
            AstKind::TSFunctionType(_) | AstKind::TSConstructorType(_) => return true,
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => return false,
            _ => continue,
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
    fn flags_inline_object_param() {
        assert_eq!(
            run_on("function f(opts: { name: string; age: number }) {}").len(),
            1
        );
    }

    #[test]
    fn allows_named_type_param() {
        assert!(run_on("function f(opts: UserOptions) {}").is_empty());
    }

    #[test]
    fn allows_primitive_type_param() {
        assert!(run_on("function f(name: string) {}").is_empty());
    }

    #[test]
    fn flags_inline_on_arrow_function() {
        assert_eq!(
            run_on("const f = (opts: { a: number }) => opts.a;").len(),
            1
        );
    }

    #[test]
    fn allows_react_component_inline_props() {
        assert!(run_on("function UserCard({ name }: { name: string }) {}").is_empty());
    }

    #[test]
    fn allows_react_arrow_component_inline_props() {
        assert!(run_on("const UserCard = ({ name }: { name: string }) => null;").is_empty());
    }

    #[test]
    fn still_flags_lowercase_function() {
        assert_eq!(
            run_on("function fetchUser(opts: { id: string }) {}").len(),
            1
        );
    }

    #[test]
    fn allows_destructured_param() {
        assert!(run_on("function createPlugin({ db, auth }: { db: Database; auth: Auth }) {}").is_empty());
    }

    #[test]
    fn allows_test_wrapper_with_children() {
        assert!(
            run_on("renderHook(() => useThing(), { wrapper: (props: { children: ReactNode }) => <Provider>{props.children}</Provider> });")
                .is_empty()
        );
    }

    #[test]
    fn allows_props_named_param() {
        assert!(run_on("const f = (props: { id: string }) => props.id;").is_empty());
    }

    #[test]
    fn allows_inline_param_in_interface_function_type_property() {
        assert!(
            run_on("interface CoreHeadHooks { 'entries:normalize': (ctx: { tags: HeadTag[] }) => SyncHookResult }")
                .is_empty()
        );
    }

    #[test]
    fn allows_inline_param_in_optional_function_type_property() {
        assert!(
            run_on("interface HeadEntryOptions { onRendered?: (ctx: { renders: DomRenderTagContext[] }) => void | Promise<void> }")
                .is_empty()
        );
    }

    #[test]
    fn allows_inline_param_in_type_alias_function() {
        assert!(run_on("type Fn = (ctx: { a: number }) => void;").is_empty());
    }

    #[test]
    fn still_flags_implementation_function_param() {
        assert_eq!(run_on("function f(ctx: { a: number }) {}").len(), 1);
    }

    #[test]
    fn still_flags_implementation_arrow_param() {
        assert_eq!(run_on("const g = (ctx: { a: number }) => {};").len(), 1);
    }

    #[test]
    fn allows_ambient_declare_function() {
        assert!(
            run_on("declare function testArraySimplification(arg: {foo: Array<{[x: string]: string}>}): void;")
                .is_empty()
        );
    }

    #[test]
    fn allows_overload_signature_without_body() {
        assert!(run_on("function f(arg: { x: string }): void;").is_empty());
    }
}
