//! OXC backend for import-namespace.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::project::import_index::{ExportKind, ImportKind};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;

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

        let canon = std::fs::canonicalize(ctx.path).unwrap_or_else(|_| ctx.path.to_path_buf());

        // 1. Collect namespace imports: local_name -> resolved source path.
        let mut ns_map: HashMap<String, PathBuf> = HashMap::new();
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
        let mut exports_by_source: HashMap<PathBuf, HashSet<String>> = HashMap::new();
        for src in ns_map.values() {
            if exports_by_source.contains_key(src) {
                continue;
            }
            let exports = index.get_exports(src);
            let has_star = exports.iter().any(|e| e.kind == ExportKind::StarReExport);
            if has_star {
                continue;
            }
            let names: HashSet<String> = exports.iter().map(|e| e.name.clone()).collect();
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
