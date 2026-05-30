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

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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
}
