//! no-unsupported-node-builtins oxc backend — compare each Node.js API usage
//! against the minimum Node version declared in the nearest `package.json`'s
//! `engines.node` field.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

use super::{
    is_browser_dom_global, lookup_global, lookup_instance_method, lookup_static_method,
    min_node_major, targets_browser,
};

pub struct Check;

/// True if the identifier reference resolves to a local binding (a declared
/// variable, parameter, import, etc.) that shadows the global Node.js API of the
/// same name. References to a genuine Node global stay unresolved (no
/// `symbol_id`), so only a same-named local declaration yields `Some` here.
fn resolves_to_local_binding(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Some(ref_id) = ident.reference_id.get() else {
        return false;
    };
    semantic.scoping().get_reference(ref_id).symbol_id().is_some()
}

/// True if the identifier is in a declaration position (variable name, param
/// name, function/class name). Prevents "shim" declarations from tripping
/// the rule on themselves.
fn is_declaration_name<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let parent_id = semantic.nodes().parent_id(node.id());
    if node.id() == parent_id {
        return false;
    }
    let parent = semantic.nodes().get_node(parent_id);
    matches!(
        parent.kind(),
        AstKind::VariableDeclarator(_)
            | AstKind::Function(_)
            | AstKind::Class(_)
            | AstKind::MethodDefinition(_)
            | AstKind::FormalParameter(_)
            | AstKind::FormalParameters(_)
            | AstKind::ArrowFunctionExpression(_)
            | AstKind::BindingRestElement(_)
            | AstKind::LabeledStatement(_)
    )
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
        let Some(min_version) = min_node_major(ctx) else {
            return Vec::new();
        };

        let path_str = ctx.path.to_string_lossy().replace('\\', "/");
        if super::NON_NODE_RUNTIME_DIRS.iter().any(|p| path_str.contains(p)) {
            return Vec::new();
        }

        let browser_target = targets_browser(ctx);

        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::IdentifierReference(ident) => {
                    if is_declaration_name(node, semantic) {
                        continue;
                    }
                    if resolves_to_local_binding(ident, semantic) {
                        continue;
                    }
                    let text = ident.name.as_str();
                    if browser_target && is_browser_dom_global(text) {
                        continue;
                    }
                    if let Some(required) = lookup_global(text).filter(|&r| r > min_version) {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, ident.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: format!(
                                "`{text}` is not available in Node.js {min_version}; requires Node.js {required} or later."
                            ),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
                AstKind::StaticMemberExpression(mem) => {
                    let prop_text = mem.property.name.as_str();

                    // Static method on `Object` / `Array`.
                    if let oxc_ast::ast::Expression::Identifier(obj) = &mem.object {
                        let obj_text = obj.name.as_str();
                        if let Some(required) =
                            lookup_static_method(obj_text, prop_text).filter(|&r| r > min_version)
                        {
                            let (line, column) =
                                byte_offset_to_line_col(ctx.source, mem.span.start as usize);
                            diagnostics.push(Diagnostic {
                                path: Arc::clone(&ctx.path_arc),
                                line,
                                column,
                                rule_id: super::META.id.into(),
                                message: format!(
                                    "`{obj_text}.{prop_text}` is not available in Node.js {min_version}; requires Node.js {required} or later."
                                ),
                                severity: Severity::Warning,
                                span: None,
                            });
                            continue;
                        }
                    }

                    // Instance method — flagged regardless of receiver shape.
                    if let Some(required) =
                        lookup_instance_method(prop_text).filter(|&r| r > min_version)
                    {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, mem.property.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: format!(
                                "`.{prop_text}()` is not available in Node.js {min_version}; requires Node.js {required} or later."
                            ),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
                _ => {}
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser as OxcParser;
    use oxc_semantic::SemanticBuilder;
    use oxc_span::SourceType;
    use std::fs;
    use tempfile::TempDir;

    fn setup_at_path(
        node_version: &str,
        source: &str,
        rel_path: &str,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        let pkg =
            format!(r#"{{"name":"t","version":"0.0.0","engines":{{"node":"{node_version}"}}}}"#);
        setup_with_pkg(&pkg, source, rel_path)
    }

    fn setup_with_pkg(pkg: &str, source: &str, rel_path: &str) -> Vec<crate::diagnostic::Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), pkg).unwrap();

        let full = dir.path().join(rel_path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(&full, source).unwrap();
        let full = fs::canonicalize(&full).unwrap();

        let lang = Language::from_path(&full).unwrap_or(Language::TypeScript);
        let sf = SourceFile {
            path: full.clone(),
            language: lang,
        };
        let refs: Vec<&SourceFile> = vec![&sf];
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);

        let source_type = match lang {
            Language::Tsx => SourceType::tsx(),
            _ => SourceType::ts(),
        };
        let allocator = Allocator::default();
        let parse_ret = OxcParser::new(&allocator, source, source_type).parse();
        let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
        let ctx = CheckCtx::for_test_with_project(&full, source, &project);
        Check.run_on_semantic(&semantic, &ctx)
    }

    #[test]
    fn skips_file_in_deno_dir() {
        // Regression for issue #831: fetch is a Web API valid in Deno
        let d = setup_at_path(
            ">=16",
            "fetch('https://example.com');",
            "runtime-tests/deno/middleware.test.tsx",
        );
        assert!(d.is_empty());
    }

    #[test]
    fn skips_file_in_cloudflare_workers_dir() {
        let d = setup_at_path(
            ">=16",
            "fetch('https://example.com');",
            "runtime-tests/cloudflare-workers/handler.ts",
        );
        assert!(d.is_empty());
    }

    #[test]
    fn still_flags_in_regular_dir() {
        // Guard must not suppress diagnostics for normal Node.js files
        let d = setup_at_path(">=16", "fetch('https://example.com');", "src/app.ts");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("fetch"));
    }

    #[test]
    fn treats_fetch_api_value_types_as_universal_issue_1029() {
        // Regression for issue #1029: Request/Response/Headers/FormData are
        // WHATWG standards across all modern runtimes, never version-gated.
        let d = setup_at_path(
            ">=16",
            "const f = (r: Response) => { const q = new Request('x'); const h = new Headers(); const fd = new FormData(); return new Response('ok'); };",
            "src/context.ts",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn treats_blob_and_file_as_universal_issue_1882() {
        // Regression for issue #1882: Blob and File are WHATWG web-platform
        // APIs (lib.dom.d.ts binary-data / file-upload types) available across
        // all modern runtimes, never version-gated even when engines.node is low.
        let d = setup_at_path(
            ">=12",
            "type ItemData = Record<string, Blob | string>; const g = (item: DataTransferItem): File | null => null; const b = new Blob([]); const f = new File([], 'n');",
            "src/utils/dataTransfer/Clipboard.ts",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn still_flags_genuinely_unsupported_builtin() {
        // A Node builtin that is NOT in the WHATWG exemption list must still flag.
        let d = setup_at_path(">=12", "structuredClone({});", "src/clone.ts");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("structuredClone"));
    }

    #[test]
    fn exempts_browser_dom_globals_in_browser_framework_package_issue_1834() {
        // Regression for issue #1834: ionic's @ionic/core depends on @stencil/core
        // and ships a browser bundle, so engines.node gates build tooling, not the
        // runtime. CustomEvent / navigator are browser DOM globals and must not flag.
        let pkg = r#"{"name":"@ionic/core","engines":{"node":">= 16"},"dependencies":{"@stencil/core":"4.43.5"}}"#;

        let transition = setup_with_pkg(
            pkg,
            "export const lifecycle = (el: HTMLElement | undefined, eventName: string) => { if (el) { const ev = new CustomEvent(eventName, { bubbles: false }); el.dispatchEvent(ev); } };",
            "core/src/utils/transition/index.ts",
        );
        assert!(transition.is_empty(), "{transition:?}");

        let haptic = setup_with_pkg(
            pkg,
            "export const available = () => typeof navigator !== 'undefined' && navigator.vibrate !== undefined;",
            "core/src/utils/native/haptic.ts",
        );
        assert!(haptic.is_empty(), "{haptic:?}");
    }

    #[test]
    fn still_flags_non_dom_global_in_browser_framework_package_issue_1834() {
        // Browser-target exemption only covers browser DOM globals. A genuine Node
        // bug — structuredClone on Node 16 — must still flag even with a stencil dep.
        let pkg = r#"{"name":"@ionic/core","engines":{"node":">= 16"},"dependencies":{"@stencil/core":"4.43.5"}}"#;
        let d = setup_with_pkg(pkg, "const c = structuredClone({});", "core/src/util.ts");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].message.contains("structuredClone"));
    }

    #[test]
    fn still_flags_browser_dom_global_without_browser_signal() {
        // No browserslist, no electron/vscode engine, no browser framework dep:
        // CustomEvent is a genuine Node-version bug and must still flag.
        let d = setup_at_path(">=16", "const ev = new CustomEvent('x');", "src/server.ts");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].message.contains("CustomEvent"));
    }

    #[test]
    fn exempts_local_shadow_of_node_global_issue_1703() {
        // Regression for issue #1703: a `const fetch = vi.fn()` mock shadows the
        // global fetch API; references to it resolve to the local binding, not
        // globalThis.fetch, and must not be flagged.
        let source = r#"it('computed', () => {
  const a = atom(0)
  const b = atom((get) => get(a))
  const fetch = vi.fn()
  const c = atom((get) => fetch(get(a)))
  const w = atom(null, (get, set) => {
    set(a, 1)
    fetch.mockClear()
    store.set(w)
    expect(fetch).toHaveBeenCalledOnce()
  })
})"#;
        let d = setup_at_path(">=12", source, "tests/vanilla/dependency.test.tsx");
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn still_flags_global_fetch_alongside_unrelated_local_issue_1703() {
        // The shadow exemption is scoped: a genuine global `fetch` call must still
        // flag even when an unrelated local named differently exists.
        let d = setup_at_path(
            ">=12",
            "const handler = () => fetch('https://example.com');",
            "src/api.ts",
        );
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].message.contains("fetch"));
    }

    #[test]
    fn exempts_browser_dom_globals_with_browserslist() {
        let pkg = r#"{"name":"t","engines":{"node":">= 16"},"browserslist":["last 2 versions"]}"#;
        let d = setup_with_pkg(pkg, "const ev = new CustomEvent('x');", "src/widget.ts");
        assert!(d.is_empty(), "{d:?}");
    }
}
