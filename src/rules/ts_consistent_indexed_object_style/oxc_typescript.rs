use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSTypeAliasDeclaration, AstType::TSInterfaceDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::TSTypeAliasDeclaration(alias) => {
                check_type_literal(alias.id.name.as_str(), &alias.type_annotation, ctx, diagnostics);
            }
            AstKind::TSInterfaceDeclaration(iface) => {
                check_interface_body(iface.id.name.as_str(), &iface.body, ctx, diagnostics);
            }
            _ => {}
        }
    }
}

fn check_type_literal(
    decl_name: &str,
    ty: &oxc_ast::ast::TSType,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let oxc_ast::ast::TSType::TSTypeLiteral(lit) = ty else { return };
    if lit.members.len() != 1 {
        return;
    }
    let oxc_ast::ast::TSSignature::TSIndexSignature(idx) = &lit.members[0] else { return };
    if value_references_name(&idx.type_annotation.type_annotation, decl_name) {
        return;
    }
    let (key_type, value_type) = extract_index_types(idx, ctx.source);
    let (line, column) = byte_offset_to_line_col(ctx.source, lit.span.start as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: format!("A `Record<{key_type}, {value_type}>` is preferred over an index signature."),
        severity: Severity::Warning,
        span: None,
    });
}

fn check_interface_body(
    decl_name: &str,
    body: &oxc_ast::ast::TSInterfaceBody,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if body.body.len() != 1 {
        return;
    }
    let oxc_ast::ast::TSSignature::TSIndexSignature(idx) = &body.body[0] else { return };
    if value_references_name(&idx.type_annotation.type_annotation, decl_name) {
        return;
    }
    let (key_type, value_type) = extract_index_types(idx, ctx.source);
    let (line, column) = byte_offset_to_line_col(ctx.source, body.span.start as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: format!("A `Record<{key_type}, {value_type}>` is preferred over an index signature."),
        severity: Severity::Warning,
        span: None,
    });
}

fn extract_index_types<'a>(
    idx: &oxc_ast::ast::TSIndexSignature<'a>,
    source: &'a str,
) -> (&'a str, &'a str) {
    let key_type = idx
        .parameters
        .first()
        .map(|p| &source[p.type_annotation.span.start as usize..p.type_annotation.span.end as usize])
        .unwrap_or("string");
    let value_type = &source[idx.type_annotation.type_annotation.span().start as usize..idx.type_annotation.type_annotation.span().end as usize];
    (key_type, value_type)
}

/// True when the index-signature value type `ty` references `decl_name`, the
/// name of the enclosing interface/type-alias, i.e. the signature is
/// self-referential. The rule's suggested `type Name = Record<..., Name...>`
/// rewrite cannot always express such a type — a bare self-reference makes the
/// alias circular (TS2456) — so, like `@typescript-eslint`, a self-referential
/// index signature is left unflagged. The name is matched wherever it can
/// appear in the value type: a union/intersection member, an array or tuple
/// element, a generic type argument, or nested inside those.
fn value_references_name(ty: &oxc_ast::ast::TSType, decl_name: &str) -> bool {
    use oxc_ast::ast::{TSType, TSTypeName};
    match ty {
        TSType::TSTypeReference(tref) => {
            let is_self = matches!(
                &tref.type_name,
                TSTypeName::IdentifierReference(id) if id.name.as_str() == decl_name
            );
            is_self
                || tref.type_arguments.as_ref().is_some_and(|args| {
                    args.params.iter().any(|arg| value_references_name(arg, decl_name))
                })
        }
        TSType::TSArrayType(arr) => value_references_name(&arr.element_type, decl_name),
        TSType::TSUnionType(u) => u.types.iter().any(|t| value_references_name(t, decl_name)),
        TSType::TSIntersectionType(i) => {
            i.types.iter().any(|t| value_references_name(t, decl_name))
        }
        TSType::TSTupleType(tuple) => tuple
            .element_types
            .iter()
            .any(|el| tuple_element_references_name(el, decl_name)),
        TSType::TSNamedTupleMember(member) => {
            tuple_element_references_name(&member.element_type, decl_name)
        }
        TSType::TSParenthesizedType(p) => value_references_name(&p.type_annotation, decl_name),
        TSType::TSTypeOperatorType(op) => value_references_name(&op.type_annotation, decl_name),
        _ => false,
    }
}

fn tuple_element_references_name(el: &oxc_ast::ast::TSTupleElement, decl_name: &str) -> bool {
    use oxc_ast::ast::TSTupleElement;
    match el {
        TSTupleElement::TSOptionalType(opt) => {
            value_references_name(&opt.type_annotation, decl_name)
        }
        TSTupleElement::TSRestType(rest) => value_references_name(&rest.type_annotation, decl_name),
        other => other.as_ts_type().is_some_and(|inner| value_references_name(inner, decl_name)),
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
    fn flags_index_signature_in_type_literal() {
        let diags = run_on("type Foo = { [key: string]: number };");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Record"));
    }

    #[test]
    fn flags_index_signature_in_interface() {
        let diags = run_on(
            r#"
interface Foo {
    [key: string]: number;
}
"#,
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_multiple_members() {
        let diags = run_on(
            r#"
interface Foo {
    [key: string]: number;
    name: string;
}
"#,
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_self_referential_interface() {
        let diags = run_on(
            r#"
type GenericValue = string | object | number | boolean | undefined | null;
interface IDataObject {
    [key: string]: GenericValue | IDataObject | GenericValue[] | IDataObject[];
}
"#,
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_self_referential_type_alias() {
        let diags = run_on("type Tree = { [key: string]: string | Tree };");
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_self_reference_nested_in_tuple() {
        let diags = run_on("type Tree = { [key: string]: [Tree, number] };");
        assert!(diags.is_empty());
    }

    #[test]
    fn flags_reference_to_different_type() {
        let diags = run_on(
            r#"
interface Bar {
    [k: string]: OtherType;
}
"#,
        );
        assert_eq!(diags.len(), 1);
    }
}
