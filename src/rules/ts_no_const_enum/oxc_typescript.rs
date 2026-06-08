use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSEnumDeclaration]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["enum"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSEnumDeclaration(decl) = node.kind() else {
            return;
        };
        if !decl.r#const {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, decl.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`const enum` is inlined at compile time and breaks with \
                      isolatedModules; use a regular enum or a union type instead."
                .into(),
            severity: Severity::Warning,
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
    fn flags_const_enum() {
        let diags = run_on("const enum E { A, B }");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "ts-no-const-enum");
    }


    #[test]
    fn allows_regular_enum() {
        assert!(run_on("enum E { A, B }").is_empty());
    }


    #[test]
    fn flags_declare_const_enum() {
        let diags = run_on("declare const enum E { A, B }");
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn flags_exported_const_enum() {
        let diags = run_on("export const enum E { A, B }");
        assert_eq!(diags.len(), 1);
    }
}
