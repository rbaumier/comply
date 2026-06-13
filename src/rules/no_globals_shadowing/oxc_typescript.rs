use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_ast::AstKind;
use oxc_ast::ast::{TSType, TSTypeName};
use std::path::Path;
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

/// True when `path` is a TypeScript declaration file (`*.d.ts`).
fn is_declaration_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.ends_with(".d.ts"))
}

/// True when a binding named after a global is explicitly annotated with the
/// type of that global, i.e. it deliberately *injects* the global object rather
/// than accidentally colliding with its name. Examples that are exempt:
///   - `document: Document | ShadowRoot` (DOM dependency injection, #1880)
///   - `window: Window & typeof globalThis`
///   - `document: TextDocument` / `document: lsp.TextDocument` (LSP convention)
/// A binding with no annotation, or annotated with an unrelated type
/// (`document: string`), is not injection and stays flagged.
fn is_global_typed_di_binding<'a>(
    name: &str,
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
                    .is_some_and(|ann| type_matches_global(name, &ann.type_annotation));
            }
            AstKind::VariableDeclarator(decl) => {
                return decl
                    .type_annotation
                    .as_ref()
                    .is_some_and(|ann| type_matches_global(name, &ann.type_annotation));
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

/// True when the symbol's declaration is a TypeScript ambient declaration:
/// `declare const`/`declare let`/`declare var`, `declare function`, or a
/// declaration nested inside `declare global { … }` / an ambient
/// `declare module`/`declare namespace`. Ambient declarations introduce no
/// runtime binding — they describe the type of an existing global rather than
/// shadowing it — so they must not be flagged.
fn is_ambient_declaration<'a>(
    symbol_id: oxc_semantic::SymbolId,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let scoping = semantic.scoping();
    let decl_node_id = scoping.symbol_declaration(symbol_id);
    let nodes = semantic.nodes();
    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id)) {
        match kind {
            AstKind::VariableDeclaration(decl) if decl.declare => return true,
            AstKind::Function(func) if func.declare => return true,
            AstKind::TSModuleDeclaration(module) if module.declare => return true,
            AstKind::TSGlobalDeclaration(_) => return true,
            _ => {}
        }
    }
    false
}

/// True when `ty` carries — directly, or as a member of a union/intersection —
/// a type reference whose rightmost name corresponds to the global `name`. The
/// correspondence is: `document` accepts any `*Document` type (DOM `Document`
/// plus the LSP `TextDocument` convention), `window` accepts `Window`.
fn type_matches_global(name: &str, ty: &TSType) -> bool {
    match ty {
        TSType::TSUnionType(union) => union
            .types
            .iter()
            .any(|member| type_matches_global(name, member)),
        TSType::TSIntersectionType(intersection) => intersection
            .types
            .iter()
            .any(|member| type_matches_global(name, member)),
        TSType::TSTypeReference(type_ref) => {
            let type_name = match &type_ref.type_name {
                TSTypeName::IdentifierReference(ident) => ident.name.as_str(),
                TSTypeName::QualifiedName(qualified) => qualified.right.name.as_str(),
                TSTypeName::ThisExpression(_) => return false,
            };
            global_accepts_type(name, type_name)
        }
        _ => false,
    }
}

/// True when `type_name` is a valid injected type for the global `name`.
fn global_accepts_type(name: &str, type_name: &str) -> bool {
    match name {
        "document" => type_name.ends_with("Document"),
        "window" => type_name == "Window",
        _ => false,
    }
}

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        // Every declaration in a `.d.ts` file is ambient — it produces no
        // runtime binding and cannot shadow a global, so skip the file entirely.
        if is_declaration_file(ctx.path) {
            return Vec::new();
        }
        let scoping = semantic.scoping();
        let mut diagnostics = Vec::new();
        for symbol_id in scoping.symbol_ids() {
            let name = scoping.symbol_name(symbol_id);
            if !SHADOWED_GLOBALS.contains(&name) {
                continue;
            }
            if is_ambient_declaration(symbol_id, semantic) {
                continue;
            }
            if is_global_typed_di_binding(name, symbol_id, semantic) {
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

    #[test]
    fn allows_document_param_union_with_document() {
        // DOM dependency-injection: `document: Document | ShadowRoot` passes the
        // global object type explicitly (testing-library/user-event). See #1880.
        assert!(
            run_on(
                "export function getActiveElement(document: Document | ShadowRoot): Element | null { return document.activeElement; }"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_window_param_intersection_with_window() {
        // `window: Window & typeof globalThis` injects the global window type.
        assert!(
            run_on("function f(window: Window & typeof globalThis) { return window; }").is_empty()
        );
    }

    #[test]
    fn allows_window_param_window_type() {
        assert!(run_on("function f(window: Window) { return window; }").is_empty());
    }

    #[test]
    fn flags_window_param_no_annotation() {
        assert_eq!(run_on("function f(window) { return window; }").len(), 1);
    }

    #[test]
    fn flags_window_param_non_window_type() {
        assert_eq!(
            run_on("function f(window: string) { return window; }").len(),
            1
        );
    }

    #[test]
    fn allows_declare_const_console() {
        // Ambient declaration: widens the type of the global `console`, no
        // runtime binding. See issue #1847.
        assert!(run_on("declare const console: any;").is_empty());
    }

    #[test]
    fn allows_declare_var_window() {
        assert!(run_on("declare var window: any;").is_empty());
    }

    #[test]
    fn allows_declare_let_process() {
        assert!(run_on("declare let process: any;").is_empty());
    }

    #[test]
    fn allows_declare_function_set_timeout() {
        assert!(
            run_on("declare function setTimeout(handler: Function, timeout?: number): number;")
                .is_empty()
        );
    }

    #[test]
    fn allows_declare_global_console() {
        assert!(run_on("declare global { const console: any; }").is_empty());
    }

    #[test]
    fn allows_declaration_file() {
        // Every declaration in a `.d.ts` file is ambient.
        let diags =
            crate::rules::test_helpers::run_rule(&Check, "const console: any;", "globals.d.ts");
        assert!(diags.is_empty());
    }

    #[test]
    fn flags_non_declare_const_console() {
        // A genuine runtime binding must still fire — ambient exemption must
        // not leak to ordinary declarations.
        assert_eq!(run_on("const console = {};").len(), 1);
    }
}
