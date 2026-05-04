//! ts-no-invalid-void-type OXC backend — flag `void` used outside return
//! type annotations and generic type arguments.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
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

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            let AstKind::TSVoidKeyword(kw) = node.kind() else {
                continue;
            };

            if is_return_type_context(node, semantic, kw.span.start) {
                continue;
            }
            if is_generic_type_arg(node, semantic) {
                continue;
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

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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
}
