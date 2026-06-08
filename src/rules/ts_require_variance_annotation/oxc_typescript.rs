//! ts-require-variance-annotation oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSInterfaceDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSInterfaceDeclaration(decl) = node.kind() else {
            return;
        };

        // Only flag exported interfaces.
        let parent = semantic.nodes().parent_node(node.id());
        if !matches!(parent.kind(), AstKind::ExportNamedDeclaration(_)) {
            return;
        }

        let Some(type_params) = &decl.type_parameters else {
            return;
        };

        for param in &type_params.params {
            if !param.r#in && !param.out {
                let name = param.name.name.as_str();
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, type_params.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Generic parameter `{name}` needs an `in` or `out` variance annotation (exported interface)."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_exported_interface_without_variance() {
        let diags = run("export interface Box<T> { value: T; }");
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn allows_exported_interface_with_out_variance() {
        assert!(run("export interface Box<out T> { value: T; }").is_empty());
    }


    #[test]
    fn allows_non_exported_interface() {
        assert!(run("interface Box<T> { value: T; }").is_empty());
    }


    #[test]
    fn allows_interface_without_generics() {
        assert!(run("export interface Plain { x: number; }").is_empty());
    }
}
