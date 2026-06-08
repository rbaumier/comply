//! ts-no-duplicate-enum-values oxc backend — flag duplicate values in enum declarations.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
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
        let AstKind::TSEnumDeclaration(decl) = node.kind() else { return };

        let mut seen: Vec<String> = Vec::new();
        for member in &decl.body.members {
            let Some(init) = &member.initializer else { continue };
            let init_span = init.span();
            let val = &ctx.source[init_span.start as usize..init_span.end as usize];
            let val = val.trim();
            if val.is_empty() {
                continue;
            }
            if seen.contains(&val.to_string()) {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, init_span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!("Duplicate enum member value `{val}`."),
                    severity: Severity::Warning,
                    span: None,
                });
            } else {
                seen.push(val.to_string());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_duplicate_number_values() {
        let diags = run_on("enum E { A = 1, B = 1 }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Duplicate"));
    }


    #[test]
    fn flags_duplicate_string_values() {
        let diags = run_on(r#"enum E { A = "x", B = "x" }"#);
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn allows_unique_values() {
        assert!(run_on("enum E { A = 1, B = 2 }").is_empty());
    }


    #[test]
    fn allows_no_initializer() {
        assert!(run_on("enum E { A, B, C }").is_empty());
    }


    #[test]
    fn flags_multiple_duplicates() {
        let diags = run_on("enum E { A = 1, B = 1, C = 1 }");
        assert_eq!(diags.len(), 2);
    }
}
