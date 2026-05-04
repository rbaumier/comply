use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::TSType;
use std::sync::Arc;

pub struct Check;

/// True if the TSType contains `undefined` as a union member.
fn union_has_undefined(ty: &TSType) -> bool {
    match ty {
        TSType::TSUnionType(union) => union.types.iter().any(|t| union_has_undefined(t)),
        TSType::TSUndefinedKeyword(_) => true,
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::TSPropertySignature,
            AstType::PropertyDefinition,
            AstType::FormalParameter,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (is_optional, type_ann, span_start) = match node.kind() {
            AstKind::TSPropertySignature(sig) => {
                (sig.optional, sig.type_annotation.as_ref(), sig.span.start)
            }
            AstKind::PropertyDefinition(def) => {
                (def.optional, def.type_annotation.as_ref(), def.span.start)
            }
            AstKind::FormalParameter(param) => {
                (param.optional, param.type_annotation.as_ref(), param.span.start)
            }
            _ => return,
        };

        if !is_optional {
            return;
        }

        let Some(type_ann) = type_ann else { return };
        if !union_has_undefined(&type_ann.type_annotation) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`?:` already implies `| undefined` — remove the redundant union member."
                .into(),
            severity: super::META.severity,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_optional_with_undefined() {
        assert_eq!(
            run_on("interface I { name?: string | undefined; }").len(),
            1
        );
    }

    #[test]
    fn flags_optional_with_undefined_complex() {
        assert_eq!(
            run_on("interface I { value?: number | null | undefined; }").len(),
            1
        );
    }

    #[test]
    fn allows_optional_without_undefined() {
        assert!(run_on("interface I { name?: string; }").is_empty());
    }

    #[test]
    fn allows_required_with_undefined() {
        assert!(run_on("interface I { name: string | undefined; }").is_empty());
    }
}
