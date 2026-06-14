use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::project::Framework;
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_ast::AstKind;
use oxc_ast::ast::{TSType, TSTypeName};
use std::path::Path;
use std::sync::Arc;

/// Globals that exist in every JS runtime (Node, Deno, browser, workers).
/// Shadowing one is always confusing, regardless of where the file runs.
const UNIVERSAL_GLOBALS: &[&str] = &[
    "console",
    "process",
    "global",
    "globalThis",
    "setTimeout",
    "setInterval",
];

/// Globals that only exist in a browser/DOM environment. In a pure Node.js
/// project there is no `window`/`document` to shadow, so a local of that name
/// (e.g. a GraphQL `DocumentNode` named `document`) collides with nothing and
/// is only flagged when the file actually runs in a browser context — see
/// [`file_runs_in_browser`].
const BROWSER_GLOBALS: &[&str] = &["window", "document"];

pub struct Check;

/// True when the file plausibly runs in a browser/DOM environment, so the
/// browser-only globals in [`BROWSER_GLOBALS`] are genuinely in scope. The
/// signals are read-only from central project/file context:
///   - the file is JSX/TSX — JSX renders to the DOM;
///   - the project uses a DOM-rendering framework (anything but `Plain`);
///   - the nearest `package.json` declares `browserslist` — explicit browser
///     build targets.
/// When none hold the file is treated as Node.js / server-side, where `window`
/// and `document` are not globals.
fn file_runs_in_browser(ctx: &CheckCtx) -> bool {
    if ctx.lang == crate::files::Language::Tsx {
        return true;
    }
    if ctx.project.framework != Framework::Plain {
        return true;
    }
    ctx.project
        .nearest_package_json(ctx.path)
        .is_some_and(|pkg| pkg.has_browserslist)
}

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

/// True when the symbol is introduced by an `import` declaration: a default
/// import (`import process from 'node:process'`), a named import
/// (`import { global } from '@storybook/global'`), a renamed/aliased named
/// import (`import { global as globalThis } from '@storybook/global'`), or a
/// namespace import (`import * as global from '…'`). An import binding re-exposes
/// a value from another module under a chosen name — it does not declare a local
/// variable that masks the global of that name — so it must not be flagged.
fn is_import_binding<'a>(
    symbol_id: oxc_semantic::SymbolId,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let scoping = semantic.scoping();
    let decl_node_id = scoping.symbol_declaration(symbol_id);
    matches!(
        semantic.nodes().kind(decl_node_id),
        AstKind::ImportSpecifier(_)
            | AstKind::ImportDefaultSpecifier(_)
            | AstKind::ImportNamespaceSpecifier(_)
    )
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
        let in_browser = file_runs_in_browser(ctx);
        let scoping = semantic.scoping();
        let mut diagnostics = Vec::new();
        for symbol_id in scoping.symbol_ids() {
            let name = scoping.symbol_name(symbol_id);
            let is_universal = UNIVERSAL_GLOBALS.contains(&name);
            let is_browser = BROWSER_GLOBALS.contains(&name);
            if !is_universal && !is_browser {
                continue;
            }
            // Browser-only globals shadow nothing outside a DOM environment.
            if is_browser && !in_browser {
                continue;
            }
            if is_ambient_declaration(symbol_id, semantic) {
                continue;
            }
            if is_import_binding(symbol_id, semantic) {
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

    /// A `.tsx` path is a browser/DOM context, so browser-only globals
    /// (`window`/`document`) are in scope and a local shadowing one fires.
    fn run_on_browser(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_const_console() {
        assert_eq!(run_on("const console = {};").len(), 1);
    }

    #[test]
    fn flags_let_window() {
        assert_eq!(run_on_browser("let window = {};").len(), 1);
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
        // A genuine shadow with no LSP `*Document` annotation must still fire
        // in a browser context.
        assert_eq!(run_on_browser("const document = {};").len(), 1);
    }

    #[test]
    fn flags_document_param_non_document_type() {
        assert_eq!(
            run_on_browser("function f(document: string) { return document; }").len(),
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
        assert_eq!(
            run_on_browser("function f(window) { return window; }").len(),
            1
        );
    }

    #[test]
    fn flags_window_param_non_window_type() {
        assert_eq!(
            run_on_browser("function f(window: string) { return window; }").len(),
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

    #[test]
    fn allows_document_domain_var_in_node_project() {
        // Issue #1729: in a pure Node.js project (no browser signal), `document`
        // is not a global — graphql-js names a parsed `DocumentNode` `document`.
        // Nothing is shadowed, so the rule must stay silent.
        assert!(run_on("const document = parse('{ syncField }');").is_empty());
    }

    #[test]
    fn allows_document_param_in_node_project() {
        // VS Code extension convention (issue #1506): `document: TextDocument`
        // in the Node extension host shadows no real DOM global.
        assert!(
            run_on("function lens(document) { return document.lineCount; }").is_empty()
        );
    }

    #[test]
    fn flags_document_var_in_browser_project() {
        // Negative space: in a browser/DOM context the real `document` global is
        // in scope, so a local named `document` is a genuine shadow.
        assert_eq!(run_on_browser("const document = {};").len(), 1);
    }

    #[test]
    fn flags_universal_global_in_node_project() {
        // Universal globals (`process`/`console`/timers) exist in Node too, so a
        // local shadowing one fires regardless of environment.
        assert_eq!(run_on("const process = {};").len(), 1);
        assert_eq!(run_on("let setTimeout = () => {};").len(), 1);
    }

    #[test]
    fn allows_aliased_named_import_globalthis() {
        // Issue #1667: `import { global as globalThis }` from the
        // @storybook/global cross-env polyfill is an import binding, not a local
        // variable masking the global.
        assert!(
            run_on(
                "import { global as globalThis } from '@storybook/global';\nglobalThis.__X__ = 1;"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_named_import_global() {
        // Issue #1667: `import { global }` is likewise an import binding.
        assert!(
            run_on("import { global } from '@storybook/global';\nconst { window: w } = global;")
                .is_empty()
        );
    }

    #[test]
    fn allows_default_import_process() {
        // A default import binding named after a global re-exposes a module value
        // and does not mask the global.
        assert!(run_on("import process from 'node:process';\nprocess.exit(0);").is_empty());
    }

    #[test]
    fn allows_namespace_import_global() {
        assert!(run_on("import * as globalThis from '@storybook/global';").is_empty());
    }

    #[test]
    fn flags_local_const_globalthis_despite_import_exemption() {
        // Negative space: a genuine local `const globalThis = {}` still shadows
        // the global — the import exemption must not leak to runtime declarations.
        assert_eq!(run_on("const globalThis = {};").len(), 1);
        assert_eq!(run_on("let global = {};").len(), 1);
    }
}
