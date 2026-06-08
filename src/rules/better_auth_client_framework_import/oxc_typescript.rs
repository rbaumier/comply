use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ImportDeclaration(import) = node.kind() else {
            return;
        };
        if import.source.value.as_str() != "better-auth/client" {
            return;
        }
        let (line, column) =
            byte_offset_to_line_col(ctx.source, import.source.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Import from a framework-specific path (`better-auth/react`, `/vue`, `/svelte`, `/solid`) instead of `better-auth/client`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;
    use super::Check;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_generic_client_import() {
        assert_eq!(
            run("import { createAuthClient } from \"better-auth/client\"").len(),
            1
        );
    }


    #[test]
    fn allows_react_client_import() {
        assert!(run("import { createAuthClient } from \"better-auth/react\"").is_empty());
    }


    #[test]
    fn allows_vue_client_import() {
        assert!(run("import { createAuthClient } from \"better-auth/vue\"").is_empty());
    }
}
