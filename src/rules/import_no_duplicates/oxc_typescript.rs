use rustc_hash::FxHashMap;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_ast::ast::Statement;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [crate::rules::backend::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut seen: FxHashMap<(&str, bool), usize> = FxHashMap::default();
        let mut diagnostics = Vec::new();

        for stmt in &semantic.nodes().program().body {
            let Statement::ImportDeclaration(import) = stmt else {
                continue;
            };
            let spec = import.source.value.as_str();
            let is_type = import.import_kind.is_type();
            let key = (spec, is_type);
            if let Some(&first_line) = seen.get(&key) {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, import.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Module `{spec}` is imported multiple times (first at line {first_line}). Merge into a single import."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            } else {
                let (first_line, _) =
                    byte_offset_to_line_col(ctx.source, import.span.start as usize);
                seen.insert(key, first_line);
            }
        }
        diagnostics
    }
}
