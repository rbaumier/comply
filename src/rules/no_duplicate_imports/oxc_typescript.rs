//! no-duplicate-imports OXC backend — flag multiple imports from the same module.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use rustc_hash::FxHashMap;
use std::sync::Arc;

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
        // (module source, is_type_import) -> first line number
        let mut seen: FxHashMap<(&str, bool), usize> = FxHashMap::default();

        for node in semantic.nodes().iter() {
            let AstKind::ImportDeclaration(import) = node.kind() else {
                continue;
            };
            let module = import.source.value.as_str();
            if module.is_empty() {
                continue;
            }
            let is_type = import.import_kind.is_type();
            let key = (module, is_type);
            let (line, column) =
                byte_offset_to_line_col(ctx.source, import.span.start as usize);
            if let Some(&first_line) = seen.get(&key) {
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Duplicate import from `{}` \u{2014} already imported on line {}. Merge into a single statement.",
                        module, first_line
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            } else {
                seen.insert(key, line);
            }
        }
        diagnostics
    }
}
