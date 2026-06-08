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
                check_type_literal(alias.span, &alias.type_annotation, ctx, diagnostics);
            }
            AstKind::TSInterfaceDeclaration(iface) => {
                check_interface_body(iface.span, &iface.body, ctx, diagnostics);
            }
            _ => {}
        }
    }
}

fn check_type_literal(
    _decl_span: oxc_span::Span,
    ty: &oxc_ast::ast::TSType,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let oxc_ast::ast::TSType::TSTypeLiteral(lit) = ty else { return };
    if lit.members.len() != 1 {
        return;
    }
    let oxc_ast::ast::TSSignature::TSIndexSignature(idx) = &lit.members[0] else { return };
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
    _decl_span: oxc_span::Span,
    body: &oxc_ast::ast::TSInterfaceBody,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if body.body.len() != 1 {
        return;
    }
    let oxc_ast::ast::TSSignature::TSIndexSignature(idx) = &body.body[0] else { return };
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
}
