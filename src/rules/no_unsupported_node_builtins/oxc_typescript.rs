//! no-unsupported-node-builtins oxc backend — compare each newer ECMAScript
//! built-in method usage against the minimum Node version declared in the
//! nearest `package.json`'s `engines.node` field.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

use super::{lookup_instance_method, lookup_static_method, min_node_major};

pub struct Check;

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
            let AstKind::StaticMemberExpression(mem) = node.kind() else {
                continue;
            };
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
        // The non-Node runtime dir guard suppresses version-gated method checks.
        let d = setup_at_path(
            ">=18",
            "const s = [3, 1, 2].toSorted();",
            "runtime-tests/deno/middleware.test.tsx",
        );
        assert!(d.is_empty());
    }

    #[test]
    fn skips_file_in_cloudflare_workers_dir() {
        let d = setup_at_path(
            ">=18",
            "const s = [3, 1, 2].toSorted();",
            "runtime-tests/cloudflare-workers/handler.ts",
        );
        assert!(d.is_empty());
    }

    #[test]
    fn exempts_web_platform_globals_issue_1187() {
        // Regression for issue #1187: WHATWG / WinterCG web-platform globals are
        // runtime-agnostic (browsers, Deno, Bun, Cloudflare Workers, Node 18+),
        // so they are never version-gated even with a low `engines.node`.
        let d = setup_at_path(
            ">=16",
            "const r = new Request('x'); const h = new Headers(); const ws = new WebSocket('wss://x'); const ev = new CustomEvent('x'); const ac = new AbortController(); structuredClone({}); navigator.userAgent; fetch('https://example.com'); return new Response('ok', { status: 200, headers: h });",
            "src/context.ts",
        );
        assert!(d.is_empty(), "{d:?}");
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
        // A newer ECMAScript built-in method (not a web-platform global) must
        // still flag: Array.prototype.toSorted requires Node 20.
        let d = setup_at_path(">=18", "const s = [3, 1, 2].toSorted();", "src/sort.ts");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].message.contains("toSorted"));
    }

    #[test]
    fn still_flags_unsupported_static_method() {
        // Static methods on built-in constructors stay version-gated:
        // Object.hasOwn requires Node 16.
        let d = setup_at_path(">=14", "const has = Object.hasOwn({}, 'k');", "src/has.ts");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].message.contains("Object.hasOwn"));
    }

    #[test]
    fn allows_method_at_or_above_min_version() {
        // Array.prototype.toSorted is available from Node 20, so no diagnostic.
        let d = setup_at_path(">=20", "const s = [3, 1, 2].toSorted();", "src/sort.ts");
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn flags_array_from_async_below_22() {
        let d = setup_at_path(">=20", "Array.fromAsync(iter);", "src/from.ts");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].message.contains("Array.fromAsync"));
    }

    #[test]
    fn skips_when_no_engines_field() {
        let pkg = r#"{"name":"t","version":"0.0.0"}"#;
        let d = setup_with_pkg(pkg, "const s = [3, 1, 2].toSorted();", "src/sort.ts");
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn parse_min_version_standalone() {
        use super::super::parse_min_version;
        assert_eq!(parse_min_version(">=16.0.0"), Some(16));
        assert_eq!(parse_min_version("^18"), Some(18));
        assert_eq!(parse_min_version("20.x"), Some(20));
        assert_eq!(parse_min_version(">=14 <22"), Some(14));
        assert_eq!(parse_min_version(">=14 || >=16"), Some(14));
        assert_eq!(parse_min_version(">=20 || >=18"), Some(18));
        assert_eq!(parse_min_version("garbage"), None);
    }
}
