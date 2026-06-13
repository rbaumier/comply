//! no-unsupported-node-builtins oxc backend — compare each Node.js API usage
//! against the minimum Node version declared in the nearest `package.json`'s
//! `engines.node` field.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

use super::{lookup_global, lookup_instance_method, lookup_static_method, min_node_major};

pub struct Check;

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

        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::IdentifierReference(ident) => {
                    if is_declaration_name(node, semantic) {
                        continue;
                    }
                    let text = ident.name.as_str();
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
        let dir = TempDir::new().unwrap();
        let pkg =
            format!(r#"{{"name":"t","version":"0.0.0","engines":{{"node":"{node_version}"}}}}"#);
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
}
