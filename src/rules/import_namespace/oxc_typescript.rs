//! OXC backend for import-namespace.

use rustc_hash::{FxHashMap, FxHashSet};
use std::path::PathBuf;
use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::project::import_index::{ExportKind, ImportKind};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;

/// True when the object identifier `obj` resolves to the namespace-import
/// binding itself rather than a local variable that shadows it. A local
/// `const`/`let`/`var`/param/function named the same as the namespace import
/// rebinds the name in its scope, so `obj.member` then refers to that local
/// value, not the imported module — and its members must not be checked
/// against the module's exports.
///
/// Resolution goes `reference_id` → symbol → declaration node; the namespace
/// import declares its binding as an `ImportNamespaceSpecifier` node, so any
/// other declaration kind (or an unresolved reference) means the import is
/// shadowed and the access is skipped.
fn refers_to_namespace_import(
    obj: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Some(ref_id) = obj.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let decl = scoping.symbol_declaration(sym_id);
    matches!(semantic.nodes().kind(decl), AstKind::ImportNamespaceSpecifier(_))
}

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
        let mut diagnostics = Vec::new();
        let index = ctx.project.import_index();
        if index.is_empty() {
            return diagnostics;
        }

        let canon = index.canonical(ctx.path);

        // 1. Collect namespace imports: local_name -> resolved source path.
        let mut ns_map: FxHashMap<String, PathBuf> = FxHashMap::default();
        for imp in index.get_imports(&canon) {
            if imp.kind == ImportKind::Namespace
                && let Some(src) = &imp.source_path {
                    ns_map.insert(imp.local_name.clone(), src.clone());
                }
        }

        if ns_map.is_empty() {
            return diagnostics;
        }

        // 2. For each source module, collect exported names.
        let mut exports_by_source: FxHashMap<PathBuf, FxHashSet<String>> = FxHashMap::default();
        for src in ns_map.values() {
            if exports_by_source.contains_key(src) {
                continue;
            }
            let exports = index.get_exports(src);
            let has_star = exports.iter().any(|e| e.kind == ExportKind::StarReExport);
            if has_star {
                continue;
            }
            let names: FxHashSet<String> = exports.iter().map(|e| e.name.clone()).collect();
            exports_by_source.insert(src.clone(), names);
        }

        // 3. Walk all StaticMemberExpression nodes
        for node in semantic.nodes().iter() {
            let AstKind::StaticMemberExpression(member) = node.kind() else {
                continue;
            };

            let Expression::Identifier(obj) = &member.object else {
                continue;
            };
            let obj_name = obj.name.as_str();

            let Some(src_path) = ns_map.get(obj_name) else {
                continue;
            };
            let Some(export_names) = exports_by_source.get(src_path) else {
                continue;
            };

            // A local binding (const/let/var/param/function) named the same as
            // the namespace import shadows it in scope; `obj.member` then refers
            // to the local value, not the module, so skip the export check.
            if !refers_to_namespace_import(obj, semantic) {
                continue;
            }

            let prop_name = member.property.name.as_str();
            if !export_names.contains(prop_name) {
                let (line, column) = byte_offset_to_line_col(
                    ctx.source,
                    member.property.span.start as usize,
                );
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "import-namespace".into(),
                    message: format!("`{prop_name}` is not exported by the source module."),
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
    use super::Check;
    use crate::config::Config;
    use crate::diagnostic::Diagnostic;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use crate::rules::backend::{CheckCtx, OxcCheck};
    use oxc_allocator::Allocator;
    use oxc_parser::Parser as OxcParser;
    use oxc_semantic::SemanticBuilder;
    use oxc_span::SourceType;
    use std::fs;
    use tempfile::TempDir;

    /// Build a temporary multi-file project and run the namespace-import check
    /// against the file at `entry_rel`. Drop `_dir` last so the temp tree
    /// outlives the diagnostics.
    fn run_in_project(files: &[(&str, &str)], entry_rel: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        let mut source_files = Vec::new();
        let mut entry_source = String::new();
        for (rel, content) in files {
            let p = dir.path().join(rel);
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&p, content).unwrap();
            if *rel == entry_rel {
                entry_source = (*content).to_string();
            }
            source_files.push(SourceFile {
                path: p.clone(),
                language: Language::from_path(&p).unwrap(),
            });
        }

        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);

        let entry_path = fs::canonicalize(dir.path().join(entry_rel)).unwrap();
        let lang = Language::from_path(&entry_path).unwrap();
        let source_type = match lang {
            Language::Tsx => SourceType::tsx(),
            Language::JavaScript => SourceType::cjs(),
            _ => SourceType::ts(),
        };
        let allocator = Allocator::default();
        let parse_ret = OxcParser::new(&allocator, &entry_source, source_type).parse();
        let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
        let file_ctx =
            crate::rules::file_ctx::FileCtx::build(&entry_path, &entry_source, lang, &project);
        let ctx = CheckCtx::for_test_full(&entry_path, &entry_source, &project, &file_ctx);
        Check.run_on_semantic(&semantic, &ctx)
    }

    #[test]
    fn flags_missing_export_on_genuine_namespace_import() {
        let d = run_in_project(
            &[
                ("src/string.ts", "export const isString = (x: unknown) => typeof x === 'string';"),
                ("app.ts", "import * as S from './src/string';\nS.equals(1, 1);"),
            ],
            "app.ts",
        );
        assert_eq!(d.len(), 1, "genuine missing export must be flagged: {d:?}");
        assert!(d[0].message.contains("equals"));
    }

    #[test]
    fn ignores_local_variable_shadowing_namespace_import() {
        // Issue #1235 (fp-ts pattern): a local `const S` shadows the namespace
        // import, so `S.equals` refers to the local value, not the module.
        let d = run_in_project(
            &[
                ("src/string.ts", "export const isString = (x: unknown) => typeof x === 'string';"),
                (
                    "app.ts",
                    "import * as S from './src/string';\n\
                     function getEq() {\n  \
                       const S = { equals: (a: number, b: number) => a === b };\n  \
                       return S.equals(1, 1);\n\
                     }\n\
                     getEq();",
                ),
            ],
            "app.ts",
        );
        assert!(
            d.is_empty(),
            "local `const S` shadows the import; S.equals must not flag: {d:?}"
        );
    }
}
