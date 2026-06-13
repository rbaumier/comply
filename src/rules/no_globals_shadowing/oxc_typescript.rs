use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_ast::AstKind;
use oxc_ast::ast::{TSType, TSTypeName};
use std::sync::Arc;

const SHADOWED_GLOBALS: &[&str] = &[
    "console",
    "window",
    "document",
    "process",
    "global",
    "globalThis",
    "setTimeout",
    "setInterval",
];

pub struct Check;

/// True when a binding named `document` is annotated with a `*Document` type
/// (e.g. `TextDocument`, `lsp.TextDocument`). In Node.js Language Server
/// Protocol code `document: TextDocument` is the idiomatic name for an LSP
/// document and shadows no real DOM global, so it must not be flagged.
fn is_lsp_text_document<'a>(
    symbol_id: oxc_semantic::SymbolId,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let scoping = semantic.scoping();
    let decl_node_id = scoping.symbol_declaration(symbol_id);
    let nodes = semantic.nodes();
    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id)) {
        match kind {
            AstKind::FormalParameter(param) => {
                return param
                    .type_annotation
                    .as_ref()
                    .is_some_and(|ann| type_ends_with_document(&ann.type_annotation));
            }
            AstKind::VariableDeclarator(decl) => {
                return decl
                    .type_annotation
                    .as_ref()
                    .is_some_and(|ann| type_ends_with_document(&ann.type_annotation));
            }
            // Stop at function / program boundaries — no annotation found.
            AstKind::Function(_)
            | AstKind::ArrowFunctionExpression(_)
            | AstKind::Program(_) => return false,
            _ => {}
        }
    }
    false
}

/// True when `ty` is a type reference whose rightmost name ends with `Document`
/// (`TextDocument`, `Document`, `lsp.TextDocument`, …).
fn type_ends_with_document(ty: &TSType) -> bool {
    let TSType::TSTypeReference(type_ref) = ty else { return false };
    let name = match &type_ref.type_name {
        TSTypeName::IdentifierReference(ident) => ident.name.as_str(),
        TSTypeName::QualifiedName(qualified) => qualified.right.name.as_str(),
        TSTypeName::ThisExpression(_) => return false,
    };
    name.ends_with("Document")
}

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let scoping = semantic.scoping();
        let mut diagnostics = Vec::new();
        for symbol_id in scoping.symbol_ids() {
            let name = scoping.symbol_name(symbol_id);
            if !SHADOWED_GLOBALS.contains(&name) {
                continue;
            }
            if name == "document" && is_lsp_text_document(symbol_id, semantic) {
                continue;
            }
            let span = scoping.symbol_span(symbol_id);
            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Local variable shadows global `{name}` — rename to avoid confusion."
                ),
                severity: super::META.severity,
                span: None,
            });
        }
        diagnostics
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
    fn flags_const_console() {
        assert_eq!(run_on("const console = {};").len(), 1);
    }

    #[test]
    fn flags_let_window() {
        assert_eq!(run_on("let window = {};").len(), 1);
    }

    #[test]
    fn allows_different_name() {
        assert!(run_on("const myConsole = {};").is_empty());
    }

    #[test]
    fn allows_console_usage() {
        assert!(run_on("console.log('hello');").is_empty());
    }

    #[test]
    fn flags_destructured_console() {
        assert_eq!(run_on("const { console } = obj;").len(), 1);
    }

    #[test]
    fn flags_function_param_console() {
        assert_eq!(
            run_on("function f(console: any) { return console; }").len(),
            1
        );
    }

    #[test]
    fn allows_document_param_text_document() {
        // LSP convention: `document: TextDocument` shadows no real DOM global
        // in a Node.js server. See issue #2067.
        assert!(
            run_on("function doHover(document: TextDocument, position: Position) { return document; }")
                .is_empty()
        );
    }

    #[test]
    fn allows_document_var_text_document() {
        assert!(run_on("const document: TextDocument = openFakeDocument();").is_empty());
    }

    #[test]
    fn allows_document_qualified_text_document() {
        assert!(run_on("const document: lsp.TextDocument = make();").is_empty());
    }

    #[test]
    fn flags_untyped_document_var() {
        // A genuine shadow with no LSP `*Document` annotation must still fire.
        assert_eq!(run_on("const document = {};").len(), 1);
    }

    #[test]
    fn flags_document_param_non_document_type() {
        assert_eq!(
            run_on("function f(document: string) { return document; }").len(),
            1
        );
    }
}
